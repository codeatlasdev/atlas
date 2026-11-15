//! Crewmate definitions and task log stored under `<dir>/crew/<id>/`.
//!
//! Servers that ran Atlas Crew as "Atlas Bot" keep their data under `<dir>/bots/<id>/bot.json`:
//! [`CrewStore::new`] adopts that directory and `load_crewmate` still reads a `bot.json`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::{self, Loaded};

pub(crate) mod jsonl;
mod origin;
mod tasklog;
mod usage;
pub use origin::{GithubOrigin, GithubOriginKind};

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

/// Who may fire a GitHub trigger unless it says otherwise.
pub(crate) fn default_authors() -> Vec<String> {
    ["owner", "member", "collaborator"]
        .map(String::from)
        .to_vec()
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
        /// Run the task with the agent's bypass flags (an `auto` crewmate). Off: the task runs as `ask`.
        #[serde(default)]
        allow_auto: bool,
    },
    Github {
        repo: String,
        events: Vec<String>,
        /// Whose events may fire it, by GitHub `author_association`: "owner", "member",
        /// "collaborator", "contributor", "anyone".
        #[serde(default = "default_authors")]
        authors: Vec<String>,
        /// As for `Webhook`.
        #[serde(default)]
        allow_auto: bool,
        /// The `@handle` that fires a `mention` event. Unset: the crewmate's name as a slug.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handle: Option<String>,
        /// The GitHub login an issue must be assigned to for `issue_assigned` to fire.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
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
    /// Atlas's guards refused the work (`runner::guard`): the owner answers `override` or `retry`.
    Refused {
        reasons: Vec<String>,
    },
    /// A `ui` task finished without two images of its own (`runner::gate`): "could not prove it". The owner
    /// answers `override` or `retry`, as for `Refused`, and may attach new images to a retry.
    ProveUi {
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
    /// Why a queued task is not starting yet ("server busy": every agent slot of the crew server is taken); the chat
    /// says so instead of a bare "Queued". Cleared when the agent starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_for: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<TaskAction>,
    /// The user's timezone when the task was created (IANA, e.g. "America/Sao_Paulo").
    /// Stamped onto any `add_trigger` action that lands in this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    /// Started by text strangers can write (a GitHub event, a webhook body) from a trigger without
    /// `allow_auto`: the agent never runs with its bypass flags for it (`triggers::yolo_for`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_auto: bool,
    /// The GitHub issue, PR or comment this task answers (set by a GitHub automation): the runner
    /// posts and edits its progress comment there. Structured, never parsed from `prompt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<GithubOrigin>,
    /// How many times a hostd restart interrupted this task while it ran (`runner::recover`): it is
    /// resumed up to a cap, then fails, so a task that takes the daemon down cannot loop.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub restarts: u32,
    /// Set once a restart interrupted this task and it was started again: the chat can say so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resumed: bool,
    /// Images sent with the prompt (`crew::attachments`): records only, the bytes live next to the task
    /// and are fetched with `crewmate_attachment`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<super::attachments::Attachment>,
    /// The task changes what people see: it only ends `done` with at least `PROOF_IMAGES` images the agent
    /// attached (captures), else it parks on `TaskAction::ProveUi`. Set by the sender (`crewmate_task`, the
    /// `/ui` prefix on Telegram) or by the agent (`set_ui`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ui: bool,
    /// The crewmates whose finished tasks led to this one through `Crewmate` triggers, oldest first
    /// (empty for a task nobody chained). A chain trigger refuses to start a crewmate already in it,
    /// so two crewmates triggering each other on `done` run one round and stop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
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
            waiting_for: None,
            ask: None,
            trigger_id: None,
            usage: None,
            action: None,
            tz: None,
            no_auto: false,
            origin: None,
            restarts: 0,
            resumed: false,
            attachments: vec![],
            ui: false,
            chain: vec![],
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
    /// The kill switch (`crew_pause` / `crew_resume`): no task runs and no automation fires.
    #[serde(default)]
    pub paused: bool,
    /// How many agents the crew server runs at once; a task past that waits queued ("server busy").
    #[serde(default = "default_max_agents")]
    pub max_agents: usize,
}

/// Agents at once when `crew.json` says nothing: a handful, so one busy server is not swamped by every crewmate
/// starting an agent (each a full CLI plus the tools it runs) while a normal crew never waits.
pub const DEFAULT_MAX_AGENTS: usize = 4;

fn default_max_agents() -> usize {
    DEFAULT_MAX_AGENTS
}

impl Default for CrewSettings {
    fn default() -> Self {
        Self {
            short_replies: true,
            short_replies_model: None,
            paused: false,
            max_agents: DEFAULT_MAX_AGENTS,
        }
    }
}

/// `usage.jsonl` is rotated into `usage-<YYYY-MM-DD>.jsonl.gz` once it passes this, so the live file (read
/// whole by `crewmate_usage`) stays small and no total is lost.
const USAGE_MAX_BYTES: u64 = 1024 * 1024;

/// Rotated usage files kept per crewmate; the oldest are deleted past it. A day's rotation is a few hundred
/// KiB of gzip, so this bounds the history at about a year of heavy use in a few MiB.
const USAGE_KEEP_ROTATED: usize = 400;

/// Crewmates deleted by this process. Shared like the task logs (`tasklog::LOGS`), because handlers build their own
/// stores over the same files. Ids are never reused, so it only grows by one per delete.
static DELETED: parking_lot::Mutex<Vec<(PathBuf, String)>> = parking_lot::Mutex::new(Vec::new());

pub struct CrewStore {
    base: PathBuf,
    /// The task log size below which it is never compacted (see `tasklog`); tests lower it.
    compact_min: u64,
    /// The size of `usage.jsonl` that rotates it ([`USAGE_MAX_BYTES`]); tests lower it.
    usage_max: u64,
    /// Rotated usage files kept ([`USAGE_KEEP_ROTATED`]); tests lower it.
    usage_keep: usize,
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
            compact_min: tasklog::COMPACT_MIN,
            usage_max: USAGE_MAX_BYTES,
            usage_keep: USAGE_KEEP_ROTATED,
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
        let dir = self.writable_dir(&crewmate.id)?;
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

    /// The directory of every crewmate on disk, whether or not its definition can be read: housekeeping (the
    /// stale-upload sweep) has to reach the dir of a crewmate `list_crewmates` skips.
    pub(crate) fn list_crewmate_dirs(&self) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(self.base.join(DIR)) else {
            return vec![];
        };
        rd.filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect()
    }

    /// Remove `crew/<id>/` and refuse every later write to it (`is_deleted`): a turn that was still
    /// running recorded its end with `append_task`, which recreated the directory without its
    /// `crewmate.json`.
    pub fn delete_crewmate_dir(&self, id: &str) -> io::Result<()> {
        let mut deleted = DELETED.lock();
        deleted.push((self.base.clone(), id.to_string()));
        // The index of its task log goes with it (its lock is taken after `DELETED`, as by every write).
        if let Some(logs) = tasklog::LOGS.lock().as_mut() {
            logs.remove(&self.tasks_path(id));
        }
        match std::fs::remove_dir_all(self.crewmate_dir(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Whether this crewmate was deleted: nothing may be written for it any more.
    pub fn is_deleted(&self, id: &str) -> bool {
        DELETED
            .lock()
            .iter()
            .any(|(base, d)| *base == self.base && d == id)
    }

    /// `crew/<id>/`, created; an error for a deleted crewmate. Every write goes through here.
    fn writable_dir(&self, id: &str) -> io::Result<PathBuf> {
        let dir = self.crewmate_dir(id);
        // Under the lock `delete_crewmate_dir` holds while it removes, so a write is either before
        // the delete (and removed by it) or after it (and refused), never in between.
        let deleted = DELETED.lock();
        if deleted.iter().any(|(b, d)| *b == self.base && d == id) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("crewmate {id} was deleted"),
            ));
        }
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Run `f` on the task log of `crewmate_id`, brought up to date with the file, holding the one lock
    /// every task write takes. `create`: a log that does not exist yet is started (a write); a read of a
    /// crewmate with no log gets `None` and leaves nothing behind.
    fn with_log<R>(
        &self,
        crewmate_id: &str,
        create: bool,
        f: impl FnOnce(&Path, &mut tasklog::TaskLog) -> R,
    ) -> Option<R> {
        let path = self.tasks_path(crewmate_id);
        let mut guard = tasklog::LOGS.lock();
        let logs = guard.get_or_insert_with(Default::default);
        if !create && !logs.contains_key(&path) && !path.exists() {
            return None;
        }
        let log = logs.entry(path.clone()).or_default();
        log.sync(&path);
        Some(f(&path, log))
    }

    /// Append a JSON line to `tasks.jsonl`: `task` as it stands, the last line of an id wins.
    /// For a task that is already stored and may be changing under you, use [`Self::update_task`]
    /// (a change from a hook route or a handler) or [`Self::append_runner_state`] (the runner's copy).
    pub fn append_task(&self, crewmate_id: &str, task: &Task) -> io::Result<()> {
        self.writable_dir(crewmate_id)?;
        self.with_log(crewmate_id, true, |path, log| {
            log.push_task(path, task, self.compact_min)
        })
        .expect("a log is created on write")
    }

    /// Change the stored task `task_id` with `f` and append it, as one step: nothing else writes a task
    /// between the read and the append, so a change made meanwhile is never overwritten by a stale copy.
    /// `Ok(None)` when there is no such task (nothing is written); `f`'s error writes nothing either.
    /// `f` runs under the task-log lock: it must not call the store.
    pub fn update_task<R>(
        &self,
        crewmate_id: &str,
        task_id: &str,
        f: impl FnOnce(&mut Task) -> Result<R, String>,
    ) -> Result<Option<(Task, R)>, String> {
        self.with_log(crewmate_id, false, |path, log| {
            let Some(mut task) = log.get(task_id).cloned() else {
                return Ok(None);
            };
            let out = f(&mut task)?;
            log.push_task(path, &task, self.compact_min)
                .map_err(|e| format!("cannot record task {task_id}: {e}"))?;
            Ok(Some((task, out)))
        })
        .unwrap_or(Ok(None))
    }

    /// Append the runner's copy of a running task. The runner holds that copy for the whole run, while
    /// hook routes record on the stored task what only they write: the `ui` mark (it only ever turns
    /// on), the agent's captures (`attachments`) and the open question (`ask`). Those are taken from the
    /// stored task, so the runner's next title or step never takes them back (a `ui` task once ended
    /// `done` without its captures that way). Everything else is the runner's.
    pub fn append_runner_state(&self, crewmate_id: &str, task: &Task) -> io::Result<()> {
        self.writable_dir(crewmate_id)?;
        self.with_log(crewmate_id, true, |path, log| {
            let mut snapshot = task.clone();
            if let Some(stored) = log.get(&task.id) {
                // The store is the arbiter of a cancel: the runner's copy still says queued, preparing or running
                // (it was made before the cancel), and appending it would un-cancel the task and let its agent
                // start (#292). Only the runner's own end (`finish_task`) restates a cancelled task, as cancelled.
                if stored.state == TaskState::Cancelled && snapshot.state != TaskState::Cancelled {
                    return Ok(());
                }
                snapshot.ui |= stored.ui;
                snapshot.attachments = stored.attachments.clone();
                snapshot.ask = stored.ask.clone();
            }
            log.push_task(path, &snapshot, self.compact_min)
        })
        .expect("a log is created on write")
    }

    /// Record one more step of a task that already has a record: a line of the step alone, not the
    /// task again with every step so far (a 50-step task wrote 50 copies of its prompt). `NotFound`
    /// when the task has no record: the caller records it whole with [`Self::append_runner_state`].
    pub fn append_step(&self, crewmate_id: &str, task_id: &str, step: &Step) -> io::Result<()> {
        self.with_log(crewmate_id, false, |path, log| {
            log.push_step(path, task_id, step, self.compact_min)
        })
        .unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("crewmate {crewmate_id} has no task log"),
            ))
        })
    }

    /// The stored state of one task. Reads the index, never the file.
    pub fn task(&self, crewmate_id: &str, task_id: &str) -> Option<Task> {
        self.with_log(crewmate_id, false, |_, log| log.get(task_id).cloned())
            .flatten()
    }

    /// The tasks of a crewmate: last state per id, newest first (by creation), up to `limit`.
    pub fn read_tasks(&self, crewmate_id: &str, limit: usize) -> Vec<Task> {
        self.with_log(crewmate_id, false, |_, log| log.newest(limit))
            .unwrap_or_default()
    }

    pub fn read_memory(&self, crewmate_id: &str) -> String {
        std::fs::read_to_string(self.memory_path(crewmate_id)).unwrap_or_default()
    }

    /// Write `memory.md`, truncating at 64 KiB.
    pub fn write_memory(&self, crewmate_id: &str, text: &str) -> io::Result<()> {
        self.writable_dir(crewmate_id)?;
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
        self.writable_dir(id)?;
        let data = serde_json::to_vec(session)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&self.session_path(id), &data)
    }

    /// `<crewmate_dir>/secrets.json` — 0600, never returned in any reply.
    pub fn secrets_path(&self, id: &str) -> PathBuf {
        self.crewmate_dir(id).join("secrets.json")
    }

    /// Add one secret. A `secrets.json` that cannot be parsed is an error (and was set aside by the read):
    /// it holds every other secret of this crewmate and is never rewritten from `{}`.
    pub fn set_secret(&self, id: &str, key: &str, value: &str) -> io::Result<()> {
        let mut secrets = match self.read_secrets(id)? {
            Secrets::Ok(m) => m,
            Secrets::Corrupt(aside) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "secrets.json is unreadable and was kept as {}; set the secrets again",
                        aside.display()
                    ),
                ));
            }
        };
        secrets.insert(key.to_string(), value.to_string());
        let data = serde_json::to_vec(&secrets)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.writable_dir(id)?;
        paths::write_private(&self.secrets_path(id), &data)
    }

    /// Load secrets as env vars for the engine process (never expose to callers). A corrupt file is set aside
    /// (logged) and the run goes on without them, so the next `set_secret` starts a clean file.
    pub fn secret_env(&self, id: &str) -> std::collections::BTreeMap<String, String> {
        match self.read_secrets(id) {
            Ok(Secrets::Ok(m)) => m,
            Ok(Secrets::Corrupt(_)) => Default::default(),
            Err(e) => {
                eprintln!("atlas crew: cannot read secrets of {id}: {e}");
                Default::default()
            }
        }
    }

    fn read_secrets(&self, id: &str) -> io::Result<Secrets> {
        Ok(match paths::read_json_or_aside(&self.secrets_path(id))? {
            Loaded::Missing => Secrets::Ok(Default::default()),
            Loaded::Ok(m) => Secrets::Ok(m),
            Loaded::Corrupt(aside) => Secrets::Corrupt(aside),
        })
    }

    /// Append a usage record to `<crewmate_dir>/usage.jsonl` with engine_kind + timestamp. Past
    /// [`USAGE_MAX_BYTES`] the file is rotated (see [`usage::rotate`]); [`Self::read_usage`] sums both.
    pub fn append_usage(
        &self,
        crewmate_id: &str,
        engine_kind: &str,
        ts: u64,
        usage: &UsageRecord,
    ) -> io::Result<()> {
        let dir = self.writable_dir(crewmate_id)?;
        let mut rec = serde_json::to_value(usage).unwrap_or_default();
        rec["engine"] = serde_json::json!(engine_kind);
        rec["ts"] = serde_json::json!(ts);
        let path = dir.join(usage::LIVE);
        let len = jsonl::append_line(&path, &rec.to_string(), false)?;
        if len > self.usage_max
            && let Err(e) = usage::rotate(&dir, ts, self.usage_keep)
        {
            eprintln!("atlas crew: cannot rotate {}: {e}", path.display());
        }
        Ok(())
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
            let id_from_dir = d.file_name().map(|n| n.to_string_lossy().to_string());
            for line in usage::read_lines(&d) {
                if let Ok(mut rec) = serde_json::from_str::<serde_json::Value>(&line)
                    && rec["ts"].as_u64().unwrap_or(0) >= since
                {
                    if let Some(id) = &id_from_dir {
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

    /// The crew settings. A `crew.json` that cannot be read or parsed (a torn write, zero bytes after a crash)
    /// is NOT "defaults": the kill switch lives there, so the answer is `paused: true` and the file is left
    /// as it is (a restart reads it the same way) until [`Self::recover_crew_settings`].
    pub fn load_crew_settings(&self) -> CrewSettings {
        match self.read_crew_settings() {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "atlas crew: {} is unreadable ({e}); the crew stays paused until it is resumed",
                    self.crew_settings_path().display()
                );
                CrewSettings {
                    paused: true,
                    ..Default::default()
                }
            }
        }
    }

    fn read_crew_settings(&self) -> io::Result<CrewSettings> {
        match std::fs::read(self.crew_settings_path()) {
            Ok(data) => serde_json::from_slice(&data)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(CrewSettings::default()),
            Err(e) => Err(e),
        }
    }

    /// Whether `crew.json` cannot be read now (the crew is paused because of it).
    pub fn crew_settings_unreadable(&self) -> bool {
        self.read_crew_settings().is_err()
    }

    /// Replace an unreadable `crew.json` with a fresh paused one, the old bytes kept as
    /// `crew.json.corrupt-<unix_ms>`. A no-op for a healthy or missing file. The one explicit way out of
    /// the fail-closed state (`crew_resume` calls it before it writes).
    pub fn recover_crew_settings(&self) -> io::Result<()> {
        if self.read_crew_settings().is_ok() {
            return Ok(());
        }
        let aside = paths::set_aside_corrupt(&self.crew_settings_path())?;
        eprintln!(
            "atlas crew: unreadable crew.json kept as {}",
            aside.display()
        );
        self.save_crew_settings(&CrewSettings {
            paused: true,
            ..Default::default()
        })
    }

    /// Write `crew.json`. Refuses (InvalidData) while the file on disk is unreadable: writing the caller's
    /// view of it would persist `paused: false` from defaults. [`Self::recover_crew_settings`] first.
    pub fn save_crew_settings(&self, settings: &CrewSettings) -> io::Result<()> {
        if let Err(e) = self.read_crew_settings()
            && e.kind() == io::ErrorKind::InvalidData
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is unreadable and was not overwritten: {e}",
                    self.crew_settings_path().display()
                ),
            ));
        }
        let data = serde_json::to_vec_pretty(settings)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        paths::write_private(&self.crew_settings_path(), &data)
    }
}

enum Secrets {
    Ok(std::collections::BTreeMap<String, String>),
    Corrupt(PathBuf),
}

pub mod archive;
#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod log_tests;
#[cfg(test)]
mod tests;
