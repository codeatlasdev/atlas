//! Bot definitions and task log stored under <dir>/bots/<id>/.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths;

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    Ask,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub enabled: bool,
    pub last_run: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    pub id: String,
    /// 1–40 characters.
    pub name: String,
    /// Up to 2 000 characters.
    pub role: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    pub approval: Approval,
    #[serde(default)]
    pub routines: Vec<Routine>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub ts: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub ts: u64,
    pub source: String,
    pub prompt: String,
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
}

impl Task {
    pub fn new(source: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: paths::new_id(),
            ts: now_s(),
            source: source.into(),
            prompt: prompt.into(),
            state: TaskState::Queued,
            pane: None,
            result: None,
            error: None,
            finished_at: None,
            started_at: None,
            steps: vec![],
        }
    }
}

pub struct BotStore {
    base: PathBuf,
}

impl BotStore {
    pub fn new(base: &Path) -> Self {
        Self {
            base: base.to_path_buf(),
        }
    }

    /// `<base>/bots/<id>/`
    pub fn bot_dir(&self, id: &str) -> PathBuf {
        self.base.join("bots").join(id)
    }

    /// `<bot_dir>/memory.md`
    pub fn memory_path(&self, id: &str) -> PathBuf {
        self.bot_dir(id).join("memory.md")
    }

    /// `<bot_dir>/tasks.jsonl`
    pub fn tasks_path(&self, id: &str) -> PathBuf {
        self.bot_dir(id).join("tasks.jsonl")
    }

    /// Write `bot.json` atomically, 0600.
    pub fn save_bot(&self, bot: &Bot) -> io::Result<()> {
        let dir = self.bot_dir(&bot.id);
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_vec_pretty(bot)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&dir.join("bot.json"), &data)
    }

    pub fn load_bot(&self, id: &str) -> io::Result<Bot> {
        let data = std::fs::read(self.bot_dir(id).join("bot.json"))?;
        serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn list_bots(&self) -> Vec<Bot> {
        let dir = self.base.join("bots");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return vec![];
        };
        rd.filter_map(|e| {
            let id = e.ok()?.file_name().to_string_lossy().to_string();
            self.load_bot(&id).ok()
        })
        .collect()
    }

    pub fn delete_bot_dir(&self, id: &str) -> io::Result<()> {
        std::fs::remove_dir_all(self.bot_dir(id))
    }

    /// Append a JSON line to `tasks.jsonl`.
    pub fn append_task(&self, bot_id: &str, task: &Task) -> io::Result<()> {
        let dir = self.bot_dir(bot_id);
        std::fs::create_dir_all(&dir)?;
        let mut line = serde_json::to_string(task)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.tasks_path(bot_id))?;
        f.write_all(line.as_bytes())
    }

    /// Read `tasks.jsonl`: last state per id, newest-first, up to `limit`.
    pub fn read_tasks(&self, bot_id: &str, limit: usize) -> Vec<Task> {
        let Ok(f) = std::fs::File::open(self.tasks_path(bot_id)) else {
            return vec![];
        };
        // Build a map of id → latest Task (last write wins).
        let mut by_id: std::collections::HashMap<String, Task> = std::collections::HashMap::new();
        let mut order: Vec<String> = vec![];
        for line in io::BufReader::new(f).lines().map_while(Result::ok) {
            if let Ok(t) = serde_json::from_str::<Task>(&line) {
                let is_new = !by_id.contains_key(&t.id);
                by_id.insert(t.id.clone(), t.clone());
                if is_new {
                    order.push(t.id);
                }
            }
        }
        // Newest-first: reverse insertion order, take up to limit.
        order
            .iter()
            .rev()
            .filter_map(|id| by_id.get(id).cloned())
            .take(limit)
            .collect()
    }

    pub fn read_memory(&self, bot_id: &str) -> String {
        std::fs::read_to_string(self.memory_path(bot_id)).unwrap_or_default()
    }

    /// Write `memory.md`, truncating at 64 KiB.
    pub fn write_memory(&self, bot_id: &str, text: &str) -> io::Result<()> {
        let dir = self.bot_dir(bot_id);
        std::fs::create_dir_all(&dir)?;
        const MAX: usize = 64 * 1024;
        let bytes = text.as_bytes();
        let data = if bytes.len() > MAX {
            &bytes[..MAX]
        } else {
            bytes
        };
        paths::write_private(&self.memory_path(bot_id), data)
    }
}
