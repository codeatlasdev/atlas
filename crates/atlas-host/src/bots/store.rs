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
pub struct EngineConfig {
    pub kind: String, // "claude"|"codex"|"opencode"|"omp"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerKind {
    Schedule {
        cron: String,
    },
    Interval {
        every_secs: u64,
    },
    Webhook {
        token: String,
    },
    Github {
        repo: String,
        events: Vec<String>,
    },
    File {
        path: String,
        #[serde(default)]
        glob: Option<String>,
    },
    Bot {
        from_bot: String,
        /// "done"|"failed"
        on: String,
    },
    Service {
        project_dir: String,
        service: String,
        /// "failed"|"recovered"
        on: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub id: String,
    pub kind: TriggerKind,
    pub enabled: bool,
    /// Supports `{{event}}` placeholder.
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectRef {
    Git {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    Path {
        path: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotSession {
    /// claude UUID, codex thread_id, opencode ses_...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// omp .jsonl path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_file: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskOption {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AskAnswer {
    Key { value: String },
    Text { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskRequest {
    pub id: String,
    pub question: String,
    pub options: Vec<AskOption>,
    pub allow_text: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<AskAnswer>,
}

/// Avatar the user picks in the bot editor. Clients own the catalogue of eye
/// styles and hats (an unknown id renders as the default), so hostd only
/// checks shape: the values land in SVG attributes on every client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotLook {
    pub color: String,
    pub eyes: String,
    pub hat: String,
}

impl BotLook {
    pub fn validate(&self) -> Result<(), String> {
        let hex = self.color.len() == 7
            && self.color.starts_with('#')
            && self.color[1..].bytes().all(|b| b.is_ascii_hexdigit());
        if !hex {
            return Err(format!("look.color must be #rrggbb, got '{}'", self.color));
        }
        for (field, v) in [("eyes", &self.eyes), ("hat", &self.hat)] {
            let ok = (1..=24).contains(&v.len())
                && v.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            if !ok {
                return Err(format!("look.{field} must be 1–24 of [a-z0-9_], got '{v}'"));
            }
        }
        Ok(())
    }
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routines: Vec<Routine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<BotLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineConfig>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectRef>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<AskRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageRecord>,
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
            reply: None,
            ask: None,
            trigger_id: None,
            usage: None,
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

    /// `<bot_dir>/session.json`
    pub fn session_path(&self, id: &str) -> PathBuf {
        self.bot_dir(id).join("session.json")
    }

    pub fn load_session(&self, id: &str) -> BotSession {
        let Ok(data) = std::fs::read(self.session_path(id)) else {
            return BotSession::default();
        };
        serde_json::from_slice(&data).unwrap_or_default()
    }

    pub fn save_session(&self, id: &str, session: &BotSession) -> io::Result<()> {
        let dir = self.bot_dir(id);
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_vec(session)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&self.session_path(id), &data)
    }

    /// `<bot_dir>/secrets.json` — 0600, never returned in any reply.
    pub fn secrets_path(&self, id: &str) -> PathBuf {
        self.bot_dir(id).join("secrets.json")
    }

    pub fn set_secret(&self, id: &str, key: &str, value: &str) -> io::Result<()> {
        let mut secrets = self.load_secrets_raw(id);
        secrets.insert(key.to_string(), value.to_string());
        let data = serde_json::to_vec(&secrets)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let dir = self.bot_dir(id);
        std::fs::create_dir_all(&dir)?;
        paths::write_private(&self.secrets_path(id), &data)
    }

    /// Load secrets as env vars for the engine process (never expose to callers).
    pub fn secret_env(&self, id: &str) -> std::collections::BTreeMap<String, String> {
        self.load_secrets_raw(id)
    }

    fn load_secrets_raw(&self, id: &str) -> std::collections::BTreeMap<String, String> {
        let Ok(data) = std::fs::read(self.secrets_path(id)) else {
            return Default::default();
        };
        serde_json::from_slice(&data).unwrap_or_default()
    }

    /// Append a usage record to `<bot_dir>/usage.jsonl` with engine_kind + timestamp.
    pub fn append_usage(
        &self,
        bot_id: &str,
        engine_kind: &str,
        ts: u64,
        usage: &UsageRecord,
    ) -> io::Result<()> {
        let dir = self.bot_dir(bot_id);
        std::fs::create_dir_all(&dir)?;
        let mut rec = serde_json::to_value(usage).unwrap_or_default();
        rec["engine"] = serde_json::json!(engine_kind);
        rec["ts"] = serde_json::json!(ts);
        let mut line = rec.to_string();
        line.push('\n');
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("usage.jsonl"))?;
        f.write_all(line.as_bytes())
    }

    /// Read usage records for aggregation. `bot_id = None` → all bots.
    pub fn read_usage(&self, bot_id: Option<&str>, since: u64) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        let dirs: Vec<_> = if let Some(id) = bot_id {
            vec![self.bot_dir(id)]
        } else {
            let Ok(rd) = std::fs::read_dir(self.base.join("bots")) else {
                return out;
            };
            rd.filter_map(|e| e.ok().map(|e| e.path())).collect()
        };
        for d in dirs {
            let Ok(f) = std::fs::File::open(d.join("usage.jsonl")) else {
                continue;
            };
            for line in io::BufReader::new(f).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                    && v["ts"].as_u64().unwrap_or(0) >= since
                {
                    let id_from_dir = d.file_name().map(|n| n.to_string_lossy().to_string());
                    let mut rec = v;
                    if let Some(id) = id_from_dir {
                        rec["bot_id"] = serde_json::json!(id);
                    }
                    out.push(rec);
                }
            }
        }
        out
    }
}
