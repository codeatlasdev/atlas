//! Crewmate definitions and task log stored under `<dir>/crew/<id>/`.
//!
//! Servers that ran Atlas Crew as "Atlas Bot" keep their data under `<dir>/bots/<id>/bot.json`:
//! [`CrewStore::new`] adopts that directory and `load_crewmate` still reads a `bot.json`.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths;

/// `<hostd dir>/crew/<id>/`: where a crewmate keeps everything (definition, tasks, memory, work tree).
pub(crate) fn crewmate_dir_in(base: &Path, id: &str) -> PathBuf {
    base.join(DIR).join(id)
}

/// The store directory under the hostd dir.
const DIR: &str = "crew";
/// Its name before Atlas Crew was called that.
const LEGACY_DIR: &str = "bots";
/// A crewmate's definition file, and its name before the rename.
const FILE: &str = "crewmate.json";
const LEGACY_FILE: &str = "bot.json";

/// How many tasks `crewmate_tasks` returns when the client gives no limit.
pub(crate) fn default_task_limit() -> usize {
    50
}

/// Unix seconds now: the one clock of the crew module.
pub(crate) fn now_s() -> u64 {
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
    /// Fires when another crewmate finishes. Servers from before the rename stored it as
    /// `{"type":"bot","from_bot":…}`: read, then written back in the new shape.
    #[serde(alias = "bot")]
    Crewmate {
        #[serde(alias = "from_bot")]
        from_crewmate: String,
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
    /// IANA timezone ("America/Sao_Paulo"). Cron is evaluated in it; None = UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    /// Unix seconds of the next scheduled fire; computed on list, never persisted.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub next_fire: Option<u64>,
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
pub struct CrewmateSession {
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

/// Avatar the user picks in the crewmate editor. Clients own the catalogue of eye styles, hats,
/// tools, outfits and eyewear (an unknown id renders as the default), so hostd only checks shape:
/// the values land in SVG attributes on every client. `tool`, `outfit` and `face` are absent from
/// looks saved by older clients and stay absent until a client sets them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrewmateLook {
    pub color: String,
    pub eyes: String,
    pub hat: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outfit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
}

impl CrewmateLook {
    pub fn validate(&self) -> Result<(), String> {
        let hex = self.color.len() == 7
            && self.color.starts_with('#')
            && self.color[1..].bytes().all(|b| b.is_ascii_hexdigit());
        if !hex {
            return Err(format!("look.color must be #rrggbb, got '{}'", self.color));
        }
        let ids = [
            ("eyes", Some(&self.eyes)),
            ("hat", Some(&self.hat)),
            ("tool", self.tool.as_ref()),
            ("outfit", self.outfit.as_ref()),
            ("face", self.face.as_ref()),
        ];
        for (field, v) in ids {
            let Some(v) = v else { continue };
            let ok = (1..=24).contains(&v.len())
                && v.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
                });
            if !ok {
                return Err(format!(
                    "look.{field} must be 1–24 of [a-z0-9_-], got '{v}'"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crewmate {
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
    pub look: Option<CrewmateLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineConfig>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectRef>,
    /// Current setup conversation step; None once setup is complete or not needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_replies: Option<bool>,
    pub created_at: u64,
}

/// Structured action a crewmate task is waiting for before it can continue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskAction {
    ConnectGithub {
        reason: String,
    },
    ProvideSecret {
        name: String,
        label: String,
        reason: String,
    },
    Input {
        label: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Choose {
        question: String,
        options: Vec<String>,
    },
    SignInEngine {
        engine: String,
    },
    /// Agent proposes adding a new automation trigger; user must confirm via `crewmate_action_answer`.
    AddTrigger {
        trigger: TriggerKind,
        prompt: String,
        reason: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
    WaitingAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub ts: u64,
    pub text: String,
    /// The agent's title when the step ran ("CART: Testar store api e carrinho"): the chat groups steps under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
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
    pub reply_full: Option<String>,
    #[serde(default)]
    pub summarized: bool,
    /// What the user picked or typed for this task's `choose`/`input` action, shown in the chat as their message
    /// (it was sent to the agent but never recorded, so the choice vanished from the conversation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// What the agent says it is working on (omp's session title), live while the task runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// While the crewmate's project is still being downloaded before the task can start ("codeatlasdev/ui"); the chat says
    /// "Downloading …" instead of a bare "Queued" (a first task on a big repo waited over a minute with no word).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<TaskAction>,
    /// The user's timezone when the task was created (IANA, e.g. "America/Sao_Paulo").
    /// Stamped onto any `add_trigger` action that lands in this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
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
            reply_full: None,
            summarized: false,
            answer: None,
            title: None,
            preparing: None,
            ask: None,
            trigger_id: None,
            usage: None,
            action: None,
            tz: None,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewSettings {
    #[serde(default = "default_true")]
    pub short_replies: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_replies_model: Option<String>,
}

impl Default for CrewSettings {
    fn default() -> Self {
        Self {
            short_replies: true,
            short_replies_model: None,
        }
    }
}

pub struct CrewStore {
    base: PathBuf,
}

impl CrewStore {
    /// Adopts `<base>/bots` when it is the only store dir (see the module docs).
    pub fn new(base: &Path) -> Self {
        use atlas_core::crew::{Adopted, adopt_legacy_path};
        match adopt_legacy_path(&base.join(LEGACY_DIR), &base.join(DIR)) {
            Ok(Adopted::BothExist) => eprintln!(
                "atlas crew: {LEGACY_DIR}/ and {DIR}/ both exist in {}; using {DIR}/, {LEGACY_DIR}/ is left untouched",
                base.display()
            ),
            Ok(Adopted::Moved) => eprintln!("atlas crew: {LEGACY_DIR}/ moved to {DIR}/"),
            Ok(Adopted::Nothing) => {}
            Err(e) => eprintln!("atlas crew: cannot move {LEGACY_DIR}/ to {DIR}/: {e}"),
        }
        Self {
            base: base.to_path_buf(),
        }
    }

    /// `<base>/crew/<id>/`
    pub fn crewmate_dir(&self, id: &str) -> PathBuf {
        crewmate_dir_in(&self.base, id)
    }

    /// `<crewmate_dir>/memory.md`
    pub fn memory_path(&self, id: &str) -> PathBuf {
        self.crewmate_dir(id).join("memory.md")
    }

    /// `<crewmate_dir>/tasks.jsonl`
    pub fn tasks_path(&self, id: &str) -> PathBuf {
        self.crewmate_dir(id).join("tasks.jsonl")
    }

    /// Write `crewmate.json` atomically, 0600.
    pub fn save_crewmate(&self, crewmate: &Crewmate) -> io::Result<()> {
        let dir = self.crewmate_dir(&crewmate.id);
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_vec_pretty(crewmate)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&dir.join(FILE), &data)?;
        // The pre-Crew name must not shadow what was just written.
        let _ = std::fs::remove_file(dir.join(LEGACY_FILE));
        Ok(())
    }

    pub fn load_crewmate(&self, id: &str) -> io::Result<Crewmate> {
        let dir = self.crewmate_dir(id);
        let data = match std::fs::read(dir.join(FILE)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::read(dir.join(LEGACY_FILE))?,
            other => other?,
        };
        serde_json::from_slice(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Every crewmate on disk. One that cannot be read is skipped so the rest still list, and named
    /// in hostd's log (`atlas crew logs`): a file this version cannot parse must never vanish silently.
    pub fn list_crewmates(&self) -> Vec<Crewmate> {
        let dir = self.base.join(DIR);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return vec![];
        };
        rd.filter_map(|e| {
            let id = e.ok()?.file_name().to_string_lossy().to_string();
            self.load_crewmate(&id)
                .inspect_err(|err| {
                    eprintln!("atlas crew: crewmate {id} skipped, unreadable: {err}")
                })
                .ok()
        })
        .collect()
    }

    pub fn delete_crewmate_dir(&self, id: &str) -> io::Result<()> {
        std::fs::remove_dir_all(self.crewmate_dir(id))
    }

    /// Append a JSON line to `tasks.jsonl`.
    pub fn append_task(&self, crewmate_id: &str, task: &Task) -> io::Result<()> {
        let dir = self.crewmate_dir(crewmate_id);
        std::fs::create_dir_all(&dir)?;
        let mut line = serde_json::to_string(task)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.tasks_path(crewmate_id))?;
        f.write_all(line.as_bytes())
    }

    /// Read `tasks.jsonl`: last state per id, newest-first, up to `limit`.
    pub fn read_tasks(&self, crewmate_id: &str, limit: usize) -> Vec<Task> {
        let Ok(f) = std::fs::File::open(self.tasks_path(crewmate_id)) else {
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

    pub fn read_memory(&self, crewmate_id: &str) -> String {
        std::fs::read_to_string(self.memory_path(crewmate_id)).unwrap_or_default()
    }

    /// Write `memory.md`, truncating at 64 KiB.
    pub fn write_memory(&self, crewmate_id: &str, text: &str) -> io::Result<()> {
        let dir = self.crewmate_dir(crewmate_id);
        std::fs::create_dir_all(&dir)?;
        const MAX: usize = 64 * 1024;
        let bytes = text.as_bytes();
        let data = if bytes.len() > MAX {
            &bytes[..MAX]
        } else {
            bytes
        };
        paths::write_private(&self.memory_path(crewmate_id), data)
    }

    /// `<crewmate_dir>/session.json`
    pub fn session_path(&self, id: &str) -> PathBuf {
        self.crewmate_dir(id).join("session.json")
    }

    pub fn load_session(&self, id: &str) -> CrewmateSession {
        let Ok(data) = std::fs::read(self.session_path(id)) else {
            return CrewmateSession::default();
        };
        serde_json::from_slice(&data).unwrap_or_default()
    }

    pub fn save_session(&self, id: &str, session: &CrewmateSession) -> io::Result<()> {
        let dir = self.crewmate_dir(id);
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_vec(session)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&self.session_path(id), &data)
    }

    /// `<crewmate_dir>/secrets.json` — 0600, never returned in any reply.
    pub fn secrets_path(&self, id: &str) -> PathBuf {
        self.crewmate_dir(id).join("secrets.json")
    }

    pub fn set_secret(&self, id: &str, key: &str, value: &str) -> io::Result<()> {
        let mut secrets = self.load_secrets_raw(id);
        secrets.insert(key.to_string(), value.to_string());
        let data = serde_json::to_vec(&secrets)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let dir = self.crewmate_dir(id);
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

    /// Append a usage record to `<crewmate_dir>/usage.jsonl` with engine_kind + timestamp.
    pub fn append_usage(
        &self,
        crewmate_id: &str,
        engine_kind: &str,
        ts: u64,
        usage: &UsageRecord,
    ) -> io::Result<()> {
        let dir = self.crewmate_dir(crewmate_id);
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

    /// Read usage records for aggregation. `crewmate_id = None` → all crewmates.
    pub fn read_usage(&self, crewmate_id: Option<&str>, since: u64) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        let dirs: Vec<_> = if let Some(id) = crewmate_id {
            vec![self.crewmate_dir(id)]
        } else {
            let Ok(rd) = std::fs::read_dir(self.base.join(DIR)) else {
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
                        rec["crewmate_id"] = serde_json::json!(id);
                    }
                    out.push(rec);
                }
            }
        }
        out
    }

    /// `<base>/crew.json`
    pub fn crew_settings_path(&self) -> std::path::PathBuf {
        self.base.join("crew.json")
    }

    pub fn load_crew_settings(&self) -> CrewSettings {
        let Ok(data) = std::fs::read(self.crew_settings_path()) else {
            return CrewSettings::default();
        };
        serde_json::from_slice(&data).unwrap_or_default()
    }

    pub fn save_crew_settings(&self, settings: &CrewSettings) -> io::Result<()> {
        let data = serde_json::to_vec_pretty(settings)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&self.crew_settings_path(), &data)
    }
}

#[cfg(test)]
mod tests;
