//! hostd decides every tool call of a crewmate whose agent can ask (claude, omp) (#642).
//!
//! The agent is always launched in ask mode, explicitly, and never with its bypass flags (`engine::claude`,
//! `engine::omp`, `triggers::yolo_for`), so each call that needs a decision reaches hostd: claude through its
//! permission-prompt tool (`atlas:approve`), omp through the approval extension, a terminal agent through its
//! hook. All of them end in [`decide`], in this order:
//!
//! 1. the **rails** no mode removes: protected paths (`.git` internals, agent config, shell rc files, `.env*`,
//!    key files) and tasks started by a stranger's text (`Task::no_auto`: GitHub, webhooks without `allow_auto`);
//! 2. the crewmate's **current** mode: `edits` lets file edits inside its work tree run, `bypass` lets everything run;
//! 3. the task's **grants** (`agent_approve` with `scope: task`): the same kind of call, for that task only;
//! 4. otherwise the call is held for a person ([`AgentHub::open`]), who may allow once, allow for the task, or deny,
//!    with a comment the agent gets. Nobody answering in [`CREWMATE_DECISION_WAIT`] is a deny, and the owner is told.
//!
//! Every decision is one `approval_decision` audit entry, whoever or whatever made it. A waiting call is
//! decided again the moment the mode, the crew's settings or the task's grants change ([`State::changed`]).
//!
//! What the agent is told (the AX): a short fragment states the mode, what runs without asking, the rails and
//! "if denied, find a safer path or ask" ([`fragment`]). It goes with the first prompt of a turn and again when the
//! mode changed since the agent was last told: with the prompt of the next turn (claude, terminal agents) and, for
//! omp, at once, as a steering message the approval extension injects with the next decision. A deny always carries
//! its reason and the person's comment; an allow's comment reaches omp as a steering message and claude with its
//! next turn (the permission-prompt tool has no channel for it).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde_json::{Map, Value, json};
use tokio::sync::watch;

use super::store::{Crewmate, Mode, Task};
use super::{events, pane, project};
use crate::agents::hook_server::CREWMATE_DECISION_WAIT;
use crate::agents::{Answer, ApproveScope};
use crate::protocol::{
    APPROVAL_BYPASS_DISABLED, APPROVAL_FORBIDDEN, APPROVAL_UNSUPPORTED_ENGINE, Failure,
};
use crate::server::Hub;
use atlas_core::progress::Coded;

/// The longest comment that reaches the agent (characters): it is text a person typed into its context.
const MAX_COMMENT: usize = 2000;

// ── the call ─────────────────────────────────────────────────────────────────

/// One tool call, as the agent reported it.
pub(crate) struct Call<'a> {
    pub tool: &'a str,
    pub input: &'a Value,
    /// The crewmate's work tree; `None` when it is not known (nothing then counts as inside it).
    pub work_dir: Option<&'a Path>,
    /// The task came from text strangers can write and its trigger did not opt in (`Task::no_auto`).
    pub no_auto: bool,
}

/// What the rails, the mode and the grants make of a call, before any person is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Auto {
    /// Runs without asking; the source is `mode` or `grant`.
    Allow(&'static str),
    /// A person decides; the rail that forced the question, when one did.
    Hold(Option<&'static str>),
}

/// The rails, then the mode, then the grants (`granted` says whether the task holds a grant for a key).
pub(crate) fn classify(mode: Mode, call: &Call, granted: impl Fn(&str) -> bool) -> Auto {
    if let Some(rail) = rail(call) {
        return Auto::Hold(Some(rail));
    }
    match mode {
        Mode::Bypass => return Auto::Allow("mode"),
        Mode::Edits if is_tree_edit(call) => return Auto::Allow("mode"),
        _ => {}
    }
    if granted(&grant_key(call)) {
        return Auto::Allow("grant");
    }
    Auto::Hold(None)
}

/// The rail a call is under, if any. No mode removes one.
pub(crate) fn rail(call: &Call) -> Option<&'static str> {
    if call.no_auto {
        return Some("no_auto");
    }
    let work = call.work_dir;
    let hit = if is_edit_tool(call.tool) {
        paths_of(call.input).iter().any(|p| protected(p, work))
    } else {
        // A command or any other tool: every word of its input that names a protected path counts, so
        // `echo x >> .git/config` is as protected as the edit tool writing it.
        let mut hit = paths_of(call.input).iter().any(|p| protected(p, work));
        strings_of(call.input, &mut |s| {
            hit = hit || words(s).any(|w| protected(w, work));
        });
        hit
    };
    hit.then_some("protected_path")
}

/// Whether a call destroys data the way a person must see before it runs (#648): a spoken "allow" never approves
/// one, only a tap does. Word-based like [`rail`]: every command of every string of the input is read (split where
/// a shell chains them), so `ls && rm -rf build` counts and `git status` does not. An edit tool names files, it
/// does not run commands, so what it writes is never read as one.
///
/// `rm -r`/`-rf`, `git push --force`/`-f`/`--force-with-lease` (or a `+ref`), `git reset --hard`, `git clean -f`,
/// `git branch -D`, `DROP TABLE|DATABASE`, `TRUNCATE`, `mkfs`, `dd of=`, `chmod -R 777`, a redirect into `/dev/sd*`
/// (and its NVMe, IDE and virtio kin). Over-matching only asks for a tap, which is the safe side.
pub(crate) fn destructive(call: &Call) -> bool {
    if is_edit_tool(call.tool) {
        return false;
    }
    let mut hit = false;
    commands_of(call.input, &mut |s| {
        hit = hit
            || s.split(['\n', ';', '|', '&', '(', ')', '`'])
                .any(destructive_command);
    });
    hit
}

/// Every string in `v` that could be a command line: each string, and a list of strings joined as an argv
/// (`["rm", "-rf", "x"]` is one command).
fn commands_of(v: &Value, f: &mut dyn FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => {
            let argv: Option<Vec<&str>> = a.iter().map(Value::as_str).collect();
            if let Some(argv) = argv {
                f(&argv.join(" "));
            }
            a.iter().for_each(|x| commands_of(x, f));
        }
        Value::Object(o) => o.values().for_each(|x| commands_of(x, f)),
        _ => {}
    }
}

/// `cmd` without the `NAME=value` words that start it (`FOO=1 rm -rf x` runs rm).
fn skip_assignments(cmd: &str) -> &str {
    let mut rest = cmd.trim_start();
    while let Some((first, tail)) = rest.split_once(char::is_whitespace) {
        let is_assignment = first.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if !is_assignment {
            break;
        }
        rest = tail.trim_start();
    }
    rest
}

/// One command of a chain: [`destructive`] for the words between two separators.
fn destructive_command(cmd: &str) -> bool {
    const DEVICES: [&str; 4] = ["/dev/sd", "/dev/nvme", "/dev/hd", "/dev/vd"];
    if cmd
        .split('>')
        .skip(1)
        .any(|after| DEVICES.iter().any(|d| after.trim_start().starts_with(d)))
    {
        return true;
    }
    let cmd = skip_assignments(cmd);
    let toks: Vec<&str> = words(cmd).collect();
    // The program names, whatever folder they are run from, in any case.
    let names: Vec<String> = toks
        .iter()
        .map(|t| t.rsplit('/').next().unwrap_or(t).to_ascii_lowercase())
        .collect();
    // `n` is what runs: the first word, or behind wrappers (`sudo`, `sh -c`, `xargs`, `find -exec`) and their
    // options. `grep -rn rm src` names rm; it does not run it.
    const WRAPPERS: [&str; 19] = [
        "sudo", "doas", "env", "nohup", "time", "nice", "xargs", "exec", "command", "builtin",
        "ionice", "timeout", "sh", "bash", "zsh", "dash", "eval", "-exec", "-execdir",
    ];
    let runs = |n: &str| {
        names.iter().enumerate().any(|(i, t)| {
            t == n
                && (i > 0 && matches!(names[i - 1].as_str(), "-exec" | "-execdir")
                    || toks[..i].iter().zip(&names).all(|(raw, w)| {
                        WRAPPERS.contains(&w.as_str())
                            || raw.starts_with('-')
                            || raw.chars().all(|c| c.is_ascii_digit())
                    }))
        })
    };
    let has = |n: &str| names.iter().any(|t| t == n);
    let flags = || toks.iter().filter(|t| t.starts_with('-') && t.len() > 1);
    // `-rf`, `-fr`, `-R`: a short flag cluster that holds the letter (case matters: `-D` is not `-d`).
    let short = |c: char| flags().any(|t| !t.starts_with("--") && t[1..].contains(c));
    let long = |name: &str| flags().any(|t| t.starts_with(name));
    if runs("rm") && (short('r') || short('R') || long("--recursive")) {
        return true;
    }
    if runs("git") {
        let forced = short('f') || long("--force");
        let pushed_ref = toks.iter().any(|t| t.starts_with('+') && t.len() > 1);
        if (has("push") && (forced || pushed_ref))
            || (has("reset") && toks.contains(&"--hard"))
            || (has("clean") && forced)
            || (has("branch") && (short('D') || ((short('d') || long("--delete")) && forced)))
        {
            return true;
        }
    }
    if names
        .windows(2)
        .any(|w| w[0] == "drop" && (w[1] == "table" || w[1] == "database"))
    {
        return true;
    }
    if runs("truncate") || names.iter().any(|n| n.starts_with("mkfs") && runs(n)) {
        return true;
    }
    if runs("dd") && has("of") {
        return true;
    }
    runs("chmod")
        && (short('R') || long("--recursive"))
        && toks.iter().any(|t| t.trim_start_matches('0') == "777")
}

/// Whether `tool` writes files by name.
fn is_edit_tool(tool: &str) -> bool {
    matches!(
        tool.to_ascii_lowercase().as_str(),
        "edit"
            | "write"
            | "multiedit"
            | "notebookedit"
            | "notebook_edit"
            | "ast_edit"
            | "apply_patch"
            | "str_replace_editor"
            | "str_replace_based_edit_tool"
    )
}

/// A file edit whose every target is inside the crewmate's work tree (symlinks followed, `..` resolved).
fn is_tree_edit(call: &Call) -> bool {
    let Some(work) = call.work_dir else {
        return false;
    };
    if !is_edit_tool(call.tool) {
        return false;
    }
    let paths = paths_of(call.input);
    let work = real(work);
    !paths.is_empty()
        && paths.iter().all(|p| {
            !p.contains("://") && !p.starts_with('~') && real(&resolve(&work, p)).starts_with(&work)
        })
}

/// What "allow for this task" covers: file edits inside the tree as one kind (any file, never a protected
/// one: the rails come first), and for everything else the same tool with the same command or target.
fn grant_key(call: &Call) -> String {
    if is_tree_edit(call) {
        return "edit".into();
    }
    let what = call.input["command"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| paths_of(call.input).join(","));
    format!("{}:{what}", call.tool.to_ascii_lowercase())
}

// ── paths ────────────────────────────────────────────────────────────────────

/// The file names a call's input gives: the usual keys, and for omp's `edit` the headers of its patch text.
fn paths_of(input: &Value) -> Vec<String> {
    let mut out = vec![];
    for key in [
        "file_path",
        "path",
        "notebook_path",
        "filePath",
        "target_file",
        "file",
    ] {
        if let Some(p) = input[key].as_str() {
            out.push(p.to_string());
        }
    }
    for key in ["paths", "files"] {
        if let Some(list) = input[key].as_array() {
            out.extend(list.iter().filter_map(|p| p.as_str().map(str::to_string)));
        }
    }
    if let Some(text) = input["input"].as_str() {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix('[')
                && let Some(header) = rest.strip_suffix(']')
            {
                // `[path#TAG]`
                out.push(
                    header
                        .rsplit_once('#')
                        .map_or(header, |(p, _)| p)
                        .to_string(),
                );
            } else if let Some(rest) = line.strip_prefix("*** ") {
                // `*** Add File: p`, `*** Update File: p`, `*** Delete File: p`, `*** Edit File: p`, `*** Move to: p`
                if let Some((_, p)) = rest
                    .split_once("File:")
                    .or_else(|| rest.split_once("Move to:"))
                {
                    out.push(p.trim().to_string());
                }
            } else if let Some(dest) = line.strip_prefix("MV ") {
                out.push(dest.trim().trim_matches(['"', '\'']).to_string());
            }
        }
    }
    out
}

/// Every string in `v`, however deep.
fn strings_of(v: &Value, f: &mut dyn FnMut(&str)) {
    match v {
        Value::String(s) => f(s),
        Value::Array(a) => a.iter().for_each(|x| strings_of(x, f)),
        Value::Object(o) => o.values().for_each(|x| strings_of(x, f)),
        _ => {}
    }
}

/// The words of a command line: split where a shell would, and at the characters that join a path to an option.
fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '\'' | '"' | ';' | '|' | '&' | '<' | '>' | '(' | ')' | '=' | ',' | '`'
            )
    })
    .filter(|w| !w.is_empty())
}

/// Whether `p` names something no mode may touch without asking. Relative to `work` when it is under it,
/// so a work tree that itself lives under a dot directory does not protect every file in it.
fn protected(p: &str, work: Option<&Path>) -> bool {
    let rel = work
        .and_then(|w| Path::new(p).strip_prefix(w).ok())
        .map(|r| r.to_string_lossy().into_owned());
    let p = rel.as_deref().unwrap_or(p);
    p.split(['/', '\\'])
        .map(|c| c.trim_start_matches(['!', '@', '^']).to_ascii_lowercase())
        .any(|c| protected_name(&c))
}

fn protected_name(c: &str) -> bool {
    const EXACT: [&str; 26] = [
        ".git",
        ".claude",
        ".omp",
        ".codex",
        ".mcp.json",
        ".ssh",
        ".gnupg",
        ".aws",
        ".netrc",
        ".npmrc",
        ".pgpass",
        ".bashrc",
        ".bash_profile",
        ".bash_login",
        ".bash_logout",
        ".profile",
        ".zshrc",
        ".zshenv",
        ".zprofile",
        ".zlogin",
        ".zlogout",
        ".cshrc",
        ".kshrc",
        "config.fish",
        "id_rsa",
        "id_ed25519",
    ];
    const KEY_EXT: [&str; 6] = ["pem", "key", "p12", "pfx", "jks", "keystore"];
    EXACT.contains(&c)
        || matches!(c, "id_dsa" | "id_ecdsa")
        || c.starts_with(".env")
        || c.rsplit_once('.')
            .is_some_and(|(stem, ext)| !stem.is_empty() && KEY_EXT.contains(&ext))
}

/// `p` against `work`, `..` and `.` resolved without touching the disk.
fn resolve(work: &Path, p: &str) -> PathBuf {
    let joined = work.join(p);
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// `p` with the symlinks of its longest existing part followed (a file that does not exist yet is judged by the
/// folder it would be created in).
fn real(p: &Path) -> PathBuf {
    let mut existing = p.to_path_buf();
    let mut tail = vec![];
    // tree-access: resolves where a path a call names points (existence and links, never a file's content, and
    // nothing is opened or run), to decide whether the call stays inside the crewmate's tree
    while !existing.exists() {
        let Some(name) = existing.file_name().map(|n| n.to_os_string()) else {
            return p.to_path_buf();
        };
        tail.push(name);
        existing.pop();
    }
    // tree-access: same: the canonical form of the longest part that exists, to follow links out of the tree
    let mut out = existing.canonicalize().unwrap_or(existing);
    out.extend(tail.iter().rev());
    out
}

// ── what the agent is told ───────────────────────────────────────────────────

/// The words a mode goes by in the fragment.
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Ask => "Ask",
        Mode::Edits => "Accept edits",
        Mode::Bypass => "Bypass",
    }
}

/// The AX fragment: the mode, what runs without asking, the rails, and what to do when denied. For the agent
/// (a prompt, never shown in a client), so it is not translated.
pub(crate) fn fragment(mode: Mode, no_auto: bool) -> String {
    let what = match mode {
        Mode::Ask => "Ask: every tool call waits for the owner to allow it.",
        Mode::Edits => {
            "Accept edits: file edits inside your working tree run without asking. Everything else (commands, \
             edits elsewhere, web, MCP tools) waits for the owner."
        }
        Mode::Bypass => "Bypass: every tool call runs without asking, except the rails below.",
    };
    let strangers = if no_auto {
        " This task started from GitHub or a webhook, so every tool call waits for the owner, whatever the mode."
    } else {
        ""
    };
    format!(
        "[Atlas permissions] Your permission mode is {name}. {what}\n\
         Rails, in every mode, always wait for the owner: protected paths (.git internals, .claude/.omp/.codex \
         config and .mcp.json, shell rc files, .env files, key files) and tasks started from GitHub or a \
         webhook.{strangers}\n\
         The owner can change your mode at any time and it applies from your next tool call; you are told when it \
         changes.\n\
         If a call is denied, the reason comes back with it: find a safer path, or ask the user. Do not retry the \
         same call.",
        name = mode_name(mode),
    )
}

/// Whether the crewmate's agent can pause for a person (`can_ask` in `data/agents.json`): its engine, else
/// its terminal agent. The mode of one that cannot is not enforced, and nothing is said about it.
pub(crate) fn can_ask(c: &Crewmate) -> bool {
    match &c.engine {
        Some(e) => super::engine::can_ask(&e.kind),
        None => atlas_core::agents::get(&c.agent).is_some_and(|a| a.can_ask),
    }
}

// ── shared state ─────────────────────────────────────────────────────────────

/// Something that can change a waiting call's decision happened; `device` is who did it.
#[derive(Clone, Default)]
pub(crate) struct Change {
    generation: u64,
    device: String,
}

/// The approval state of a crew server (in memory: a restart asks again, which is the safe side).
pub(crate) struct State {
    /// Task grants by pane key (`crew:<crewmate>:<task>`): the keys ([`grant_key`]) the owner allowed for it.
    grants: Mutex<HashMap<String, Vec<String>>>,
    /// The mode each pane's agent was last told ([`fragment`]).
    announced: Mutex<HashMap<String, Mode>>,
    /// What an allow's comment says, for a claude crewmate's next turn (by crewmate id).
    notes: Mutex<HashMap<String, Vec<String>>>,
    /// Bumped when a mode, the crew's settings or a grant changed: every waiting call decides again.
    changed: watch::Sender<Change>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            grants: Mutex::default(),
            announced: Mutex::default(),
            notes: Mutex::default(),
            changed: watch::channel(Change::default()).0,
        }
    }
}

impl State {
    fn bump(&self, device: &str) {
        self.changed.send_modify(|c| {
            c.generation += 1;
            c.device = device.to_string();
        });
    }

    /// A task ended: its grants and what its agent was told end with it.
    pub(crate) fn forget(&self, pane: &str) {
        self.grants.lock().remove(pane);
        self.announced.lock().remove(pane);
    }

    /// Hand over (and clear) what an allow's comment left for the crewmate's next turn.
    fn take_notes(&self, crewmate_id: &str) -> Vec<String> {
        self.notes.lock().remove(crewmate_id).unwrap_or_default()
    }
}

/// The text to put before a turn's prompt: the [`fragment`] when the agent was not told the current mode yet,
/// and the notes an allow's comment left. Empty when there is nothing (or the engine cannot ask). Marks the mode
/// as told.
pub(crate) fn prelude(hub: &Hub, crewmate_id: &str, pane: &str) -> String {
    let Ok(c) = hub.crew.store.load_crewmate(crewmate_id) else {
        return String::new();
    };
    if !can_ask(&c) {
        return String::new();
    }
    let mut out = String::new();
    if hub
        .crew
        .approvals
        .announced
        .lock()
        .insert(pane.to_string(), c.mode)
        != Some(c.mode)
    {
        out = fragment(c.mode, task_of(hub, pane).is_some_and(|t| t.no_auto));
    }
    for note in hub.crew.approvals.take_notes(crewmate_id) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&note);
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out
}

/// The fragment, when the agent of `pane` was last told another mode than `mode` (marks it told).
fn notice(hub: &Hub, pane: &str, mode: Mode) -> Option<String> {
    (hub.crew
        .approvals
        .announced
        .lock()
        .insert(pane.to_string(), mode)
        != Some(mode))
    .then(|| fragment(mode, task_of(hub, pane).is_some_and(|t| t.no_auto)))
}

fn task_of(hub: &Hub, pane: &str) -> Option<Task> {
    let id = pane::crewmate_id(pane)?;
    hub.crew.store.task(id, pane::task_id(pane, id)?)
}

// ── the decision ─────────────────────────────────────────────────────────────

/// What hostd answers the hook of a permission prompt (`POST /hook/<agent>/decision`).
pub(crate) struct Reply {
    /// `Some(true)` allow, `Some(false)` deny, `None` no decision (nothing treats that as an allow).
    pub allow: Option<bool>,
    /// For a deny: why, and what the person said. It reaches the agent as the reason of the block.
    pub message: Option<String>,
    /// Text the agent should read as a steering message after an allow (omp's extension injects it).
    pub steer: Option<String>,
}

impl Reply {
    /// The reply body: `{"decision": "allow"|"deny"|null, "message"?, "steer"?}`.
    pub(crate) fn body(&self) -> String {
        let mut v = json!({"decision": self.allow.map(|a| if a { "allow" } else { "deny" })});
        if let Some(m) = &self.message {
            v["message"] = json!(m);
        }
        if let Some(s) = &self.steer {
            v["steer"] = json!(s);
        }
        v.to_string()
    }

    fn deny(message: String) -> Self {
        Self {
            allow: Some(false),
            message: Some(message),
            steer: None,
        }
    }

    /// Nobody decided. The wire says `decision: null`, as it always has for this: an engine reads it as a deny and
    /// says why (`message`), a terminal agent's hook falls back to its own prompt. It is audited as a deny.
    fn unanswered(message: String) -> Self {
        Self {
            allow: None,
            message: Some(message),
            steer: None,
        }
    }
}

/// Where the crewmate's agent works: its project's tree, else its `workdir`, else its own folder (the same
/// answer `runner::prompt::prepare_work_dir` gives).
fn work_dir_of(hub: &Hub, c: &Crewmate) -> PathBuf {
    match (&c.project, &c.workdir) {
        (None, Some(w)) => PathBuf::from(w),
        _ => project::work_dir_for(
            &hub.crew.dir,
            &c.id,
            hub.crew.store.isolated(&c.id).as_ref(),
        ),
    }
}

/// [`classify`] for a call of `pane`: the crewmate's current mode and work tree, the task's marks and grants.
fn classify_pane(hub: &Hub, pane: &str, payload: &Value) -> Auto {
    let crewmate = pane::crewmate_id(pane).and_then(|id| hub.crew.store.load_crewmate(id).ok());
    let work = crewmate.as_ref().map(|c| work_dir_of(hub, c));
    let settings = hub.crew.store.load_crew_settings();
    // The crew can take Bypass away; a crewmate still in it (it was set before) is in Ask.
    let mode = match crewmate.as_ref().map_or(Mode::Ask, |c| c.mode) {
        Mode::Bypass if !settings.allow_bypass => Mode::Ask,
        m => m,
    };
    let call = Call {
        tool: payload["tool_name"].as_str().unwrap_or_default(),
        input: &payload["tool_input"],
        work_dir: work.as_deref(),
        no_auto: task_of(hub, pane).is_some_and(|t| t.no_auto),
    };
    classify(mode, &call, |key| {
        hub.crew
            .approvals
            .grants
            .lock()
            .get(pane)
            .is_some_and(|g| g.iter().any(|k| k == key))
    })
}

/// Decide a permission prompt of a crewmate's pane: see the module docs for the order. Returns when the call
/// is decided: at once, or when a person answers, the mode lets it run, or nobody answered in time.
pub(crate) async fn decide(
    hub: &Arc<Hub>,
    agent: &str,
    pane: &str,
    payload: &Value,
    can_approve: bool,
) -> Reply {
    let started = Instant::now();
    // Subscribed before the first look: a change from here on wakes the wait below.
    let mut changes = hub.crew.approvals.changed.subscribe();
    changes.borrow_and_update();
    let tool = payload["tool_name"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let crewmate_id = pane::crewmate_id(pane).unwrap_or_default().to_string();
    let task_id = pane.splitn(3, ':').nth(2).unwrap_or_default().to_string();
    let record = |allow: bool,
                  source: &str,
                  device: Option<&str>,
                  rail: Option<&str>,
                  comment: Option<&str>,
                  req: Option<&str>,
                  voice: Option<&str>| {
        let mut f = Map::new();
        if let Some(r) = req {
            f.insert("req".into(), json!(r));
        }
        f.insert("task_id".into(), json!(task_id));
        f.insert("tool".into(), json!(tool));
        f.insert("tool_input".into(), payload["tool_input"].clone());
        f.insert(
            "decision".into(),
            json!(if allow { "allow" } else { "deny" }),
        );
        f.insert("source".into(), json!(source));
        if let Some(d) = device {
            f.insert("device".into(), json!(d));
        }
        f.insert(
            "wait_ms".into(),
            json!(started.elapsed().as_millis() as u64),
        );
        if let Some(r) = rail {
            f.insert("rail".into(), json!(r));
        }
        if let Some(c) = comment {
            f.insert("comment".into(), json!(c));
        }
        // Spoken, not tapped: the transcript is what was heard, so an audit can tell what the owner said.
        if let Some(t) = voice {
            f.insert("via".into(), json!("voice"));
            f.insert("transcript".into(), json!(t));
        }
        hub.crew
            .audit
            .record(Some(&crewmate_id), "approval_decision", Value::Object(f));
    };

    let mut reply;
    match classify_pane(hub, pane, payload) {
        Auto::Allow(source) => {
            record(true, source, None, None, None, None, None);
            reply = Reply {
                allow: Some(true),
                message: None,
                steer: None,
            };
        }
        Auto::Hold(rail) => {
            let Some(prompt) = hub.agents.open(pane, agent, payload, can_approve, rail) else {
                // Nobody is there to ask. A rail refused it on its own; otherwise nobody answered.
                let source = if rail.is_some() { "rail" } else { "timeout" };
                record(false, source, None, rail, None, None, None);
                notify_unanswered(hub, &crewmate_id, &task_id, &tool);
                return finish(
                    hub,
                    agent,
                    pane,
                    Reply::unanswered(if rail.is_some() {
                        "this call needs the owner's answer (a protected path, or a task started from GitHub or a webhook) and nobody is connected to give it".into()
                    } else {
                        "nobody can answer the approval request right now (no client is connected)"
                            .into()
                    }),
                );
            };
            let req = prompt.req.clone();
            let wait = hub.agents.wait(prompt, CREWMATE_DECISION_WAIT);
            tokio::pin!(wait);
            let answer = loop {
                let woke = tokio::select! {
                    a = &mut wait => Ok(a),
                    changed = changes.changed() => Err(changed),
                };
                match woke {
                    Ok(a) => break a,
                    Err(Err(_)) => break wait.await,
                    Err(Ok(())) => {
                        let device = changes.borrow_and_update().device.clone();
                        // The mode, the crew's settings or a grant moved: the call may run now.
                        if let Auto::Allow(source) = classify_pane(hub, pane, payload) {
                            hub.agents.resolve(
                                &req,
                                Answer {
                                    allow: true,
                                    by: device,
                                    scope: ApproveScope::Once,
                                    comment: None,
                                    source,
                                    voice: None,
                                },
                            );
                        }
                    }
                }
            };
            match answer {
                Some(a) => {
                    let comment = a
                        .comment
                        .as_deref()
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .map(|c| c.chars().take(MAX_COMMENT).collect::<String>());
                    if a.allow && a.scope == ApproveScope::Task && rail.is_none() {
                        hub.crew
                            .approvals
                            .grants
                            .lock()
                            .entry(pane.to_string())
                            .or_default()
                            .push(grant_key(&Call {
                                tool: &tool,
                                input: &payload["tool_input"],
                                work_dir: crewmate_work(hub, &crewmate_id).as_deref(),
                                no_auto: false,
                            }));
                        // Other calls of this task that wait now are covered too.
                        hub.crew.approvals.bump(&a.by);
                    }
                    record(
                        a.allow,
                        a.source,
                        Some(&a.by),
                        rail,
                        comment.as_deref(),
                        Some(&req),
                        a.voice.as_deref(),
                    );
                    reply = if a.allow {
                        Reply {
                            allow: Some(true),
                            message: None,
                            steer: comment
                                .map(|c| format!("[Atlas] The owner allowed {tool} and said: {c}")),
                        }
                    } else {
                        let mut m = String::from("the owner denied this tool call");
                        if let Some(c) = comment {
                            m.push_str(": ");
                            m.push_str(&c);
                        }
                        m.push_str(". Find a safer path, or ask the user.");
                        Reply::deny(m)
                    };
                }
                None => {
                    record(false, "timeout", None, rail, None, Some(&req), None);
                    notify_unanswered(hub, &crewmate_id, &task_id, &tool);
                    reply = Reply::unanswered(
                        "nobody answered the approval request in time (the owner was told). Do not retry the same call: ask the user, or find another way.".into(),
                    );
                }
            }
        }
    }
    // A claude allow cannot carry a comment or a notice: they wait for its next turn.
    if reply.allow == Some(true)
        && agent != "omp"
        && let Some(s) = reply.steer.take()
    {
        hub.crew
            .approvals
            .notes
            .lock()
            .entry(crewmate_id.clone())
            .or_default()
            .push(s);
    }
    finish(hub, agent, pane, reply)
}

/// The work tree of a crewmate by id.
fn crewmate_work(hub: &Hub, id: &str) -> Option<PathBuf> {
    hub.crew
        .store
        .load_crewmate(id)
        .ok()
        .map(|c| work_dir_of(hub, &c))
}

/// Add what the agent must be told about a mode change to a reply, by the channel its engine has: omp as a
/// steering message with any decision, claude in the reason of a deny (an allow has no channel; the next turn
/// carries it).
fn finish(hub: &Hub, agent: &str, pane: &str, mut reply: Reply) -> Reply {
    let current = pane::crewmate_id(pane)
        .and_then(|id| hub.crew.store.load_crewmate(id).ok())
        .map(|c| (c.mode, can_ask(&c)));
    let Some((mode, true)) = current else {
        return reply;
    };
    if agent == "omp" {
        if let Some(n) = notice(hub, pane, mode) {
            reply.steer = Some(match reply.steer.take() {
                Some(s) => format!("{s}\n{n}"),
                None => n,
            });
        }
    } else if reply.allow != Some(true)
        && let Some(n) = notice(hub, pane, mode)
    {
        let m = reply.message.take().unwrap_or_default();
        reply.message = Some(format!("{m}\n{n}"));
    }
    reply
}

/// An unanswered prompt ends as a deny; the owner is told on every client and on Telegram.
fn notify_unanswered(hub: &Hub, crewmate_id: &str, task_id: &str, tool: &str) {
    hub.broadcast(events::approval_unanswered(crewmate_id, task_id, tool));
    let name = hub
        .crew
        .store
        .load_crewmate(crewmate_id)
        .map(|c| c.name)
        .unwrap_or_default();
    hub.crew.telegram.send_message(&atlas_i18n::t!(
        "cmd_hostd_approval_unanswered",
        crewmate = name,
        tool = tool
    ));
}

// ── changing the mode ────────────────────────────────────────────────────────

/// `crew.approval.forbidden`: a crewmate's own process (or a task) asked to change a mode.
pub(crate) fn forbidden() -> Coded {
    Coded::new(
        APPROVAL_FORBIDDEN,
        json!({}),
        "only the owner can change a permission mode",
    )
}

/// `crew.approval.unsupported_engine`: `engine` has no approval channel, so only `bypass` is true of it.
pub(crate) fn unsupported_engine(engine: &str, mode: Mode) -> Coded {
    Coded::new(
        APPROVAL_UNSUPPORTED_ENGINE,
        json!({"engine": engine, "mode": mode}),
        format!("{engine} has no approval channel: it always runs unattended"),
    )
}

/// `crew.approval.bypass_disabled`: the crew does not allow Bypass (`allow_bypass`).
pub(crate) fn bypass_disabled() -> Coded {
    Coded::new(
        APPROVAL_BYPASS_DISABLED,
        json!({}),
        "bypass is not allowed on this crew",
    )
}

/// `crewmate_set_approval`: the owner switches a crewmate's mode. `origin` is who is asking
/// (`Hub::origin_of`): `desktop`, `phone:<node>`, `telegram:<chat>`; anything a crewmate or a task could be is
/// refused. Replies with the crewmate as `crewmate_list` shows it.
pub(crate) fn set_approval(
    hub: &Hub,
    id: &str,
    mode: &str,
    origin: &str,
) -> Result<Value, Failure> {
    if !hub.crew_host {
        return Err("this computer is not a crew server (atlas crew install)".into());
    }
    if origin.starts_with("crewmate:") {
        return Err(forbidden().into());
    }
    let mode = Mode::parse(mode)?;
    let mut c = hub
        .crew
        .store
        .load_crewmate(id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    if let Some(e) = &c.engine
        && mode != Mode::Bypass
        && !super::engine::can_ask(&e.kind)
    {
        return Err(unsupported_engine(&e.kind, mode).into());
    }
    if mode == Mode::Bypass && !hub.crew.store.load_crew_settings().allow_bypass {
        return Err(bypass_disabled().into());
    }
    if c.mode != mode {
        let from = c.mode;
        c.mode = mode;
        hub.crew
            .store
            .save_crewmate(&c)
            .map_err(|e| format!("save_crewmate: {e}"))?;
        mode_changed(hub, &c, from, origin);
    }
    Ok(super::handlers::crewmates::crewmate_json(hub, c))
}

/// A crewmate's mode changed (saved already): audit it, tell every client, and wake the calls that wait.
pub(crate) fn mode_changed(hub: &Hub, c: &Crewmate, from: Mode, device: &str) {
    hub.crew.audit.record(
        Some(&c.id),
        "approval_mode_changed",
        json!({"from": from, "to": c.mode, "device": device}),
    );
    hub.broadcast(events::crewmate_updated(&c.id));
    hub.crew.approvals.bump(device);
}

/// The crew no longer allows Bypass: every crewmate in it goes back to Ask (audited like any change).
pub(crate) fn revoke_bypass(hub: &Hub, device: &str) {
    for mut c in hub.crew.store.list_crewmates() {
        if c.mode == Mode::Bypass && can_ask(&c) {
            c.mode = Mode::Ask;
            if hub.crew.store.save_crewmate(&c).is_ok() {
                mode_changed(hub, &c, Mode::Bypass, device);
            }
        }
    }
    hub.crew.approvals.bump(device);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: &str = "/srv/crew/c1/work";

    fn call<'a>(tool: &'a str, input: &'a Value, no_auto: bool) -> Call<'a> {
        Call {
            tool,
            input,
            work_dir: Some(Path::new(WORK)),
            no_auto,
        }
    }

    fn run(mode: Mode, tool: &str, input: Value) -> Auto {
        classify(mode, &call(tool, &input, false), |_| false)
    }

    #[test]
    fn ask_holds_everything_edits_allows_tree_edits_only_and_bypass_allows_what_the_rails_let_through()
     {
        let edit = json!({"file_path": format!("{WORK}/src/a.rs")});
        let bash = json!({"command": "cargo test"});
        let outside = json!({"file_path": "/etc/hosts"});
        for (tool, input) in [("Edit", &edit), ("Bash", &bash), ("Write", &outside)] {
            assert_eq!(
                run(Mode::Ask, tool, input.clone()),
                Auto::Hold(None),
                "{tool}"
            );
        }
        assert_eq!(run(Mode::Edits, "Edit", edit.clone()), Auto::Allow("mode"));
        assert_eq!(
            run(Mode::Edits, "Bash", bash.clone()),
            Auto::Hold(None),
            "a command waits"
        );
        assert_eq!(
            run(Mode::Edits, "Write", outside.clone()),
            Auto::Hold(None),
            "an edit outside the tree waits"
        );
        for (tool, input) in [("Edit", edit), ("Bash", bash), ("Write", outside)] {
            assert_eq!(
                run(Mode::Bypass, tool, input),
                Auto::Allow("mode"),
                "{tool}"
            );
        }
    }

    #[test]
    fn a_relative_path_is_inside_and_dot_dot_or_a_url_or_home_is_not() {
        let rel = json!({"path": "src/new/file.rs"});
        assert_eq!(run(Mode::Edits, "edit", rel), Auto::Allow("mode"));
        for p in [
            "../other/a.rs",
            "src/../../x",
            "~/notes.md",
            "xd://mcp__tool",
            "/tmp/x",
        ] {
            assert_eq!(
                run(Mode::Edits, "edit", json!({"path": p})),
                Auto::Hold(None),
                "{p}"
            );
        }
        // No path at all (a shape this does not know): never proven inside.
        assert_eq!(run(Mode::Edits, "edit", json!({"x": 1})), Auto::Hold(None));
    }

    #[test]
    fn omp_edits_name_their_files_in_the_patch_text() {
        let hashline = json!({"input": "[src/a.rs#1A2B]\nPUT 1.=1:\n+let x = 1;\n"});
        assert_eq!(run(Mode::Edits, "edit", hashline), Auto::Allow("mode"));
        let two =
            json!({"input": "[src/a.rs#1A2B]\nPUT 1.=1:\n+x\n[.env#FFFF]\nPUT 1.=1:\n+KEY=1\n"});
        assert_eq!(
            run(Mode::Bypass, "edit", two),
            Auto::Hold(Some("protected_path")),
            "one protected file among the sections"
        );
        let moved = json!({"input": "[src/a.rs#1A2B]\nMV /tmp/elsewhere.rs\n"});
        assert_eq!(
            run(Mode::Edits, "edit", moved),
            Auto::Hold(None),
            "a move out of the tree"
        );
        let patch =
            json!({"input": "*** Begin Patch\n*** Update File: .git/config\n@@\n*** End Patch"});
        assert_eq!(
            run(Mode::Bypass, "apply_patch", patch),
            Auto::Hold(Some("protected_path"))
        );
        // The content of an edit that mentions a protected name is not a path.
        let prose = json!({"input": "[README.md#1A2B]\nPUT 1.=1:\n+copy .env.example to .env\n"});
        assert_eq!(run(Mode::Bypass, "edit", prose), Auto::Allow("mode"));
    }

    #[test]
    fn the_rails_hold_in_every_mode() {
        let protected = [
            json!({"file_path": format!("{WORK}/.git/config")}),
            json!({"file_path": ".git/hooks/pre-commit"}),
            json!({"file_path": format!("{WORK}/.env")}),
            json!({"file_path": "app/.env.production"}),
            json!({"file_path": ".claude/settings.json"}),
            json!({"file_path": ".omp/extensions/x.ts"}),
            json!({"file_path": ".codex/hooks.json"}),
            json!({"file_path": ".mcp.json"}),
            json!({"file_path": "/home/u/.zshrc"}),
            json!({"file_path": "~/.bashrc"}),
            json!({"file_path": "deploy/server.pem"}),
            json!({"file_path": "/home/u/.ssh/id_ed25519"}),
        ];
        for input in protected {
            for mode in [Mode::Ask, Mode::Edits, Mode::Bypass] {
                for tool in ["Edit", "Write", "Read"] {
                    let want = Auto::Hold(Some("protected_path"));
                    let got = run(mode, tool, input.clone());
                    if mode == Mode::Ask {
                        assert!(matches!(got, Auto::Hold(_)), "{mode:?} {tool} {input}");
                    } else {
                        assert_eq!(got, want, "{mode:?} {tool} {input}");
                    }
                }
            }
        }
        for cmd in [
            "echo 'x = 1' >> .git/config",
            "cat .env",
            "cp ~/.ssh/id_rsa /tmp/k",
            "sed -i s/a/b/ .bashrc",
            "tee ./.claude/settings.json",
            "echo x>>.env.local",
        ] {
            assert_eq!(
                run(Mode::Bypass, "Bash", json!({ "command": cmd })),
                Auto::Hold(Some("protected_path")),
                "{cmd}"
            );
        }
    }

    #[test]
    fn near_misses_are_not_protected() {
        for cmd in [
            "git status",
            "git commit -m 'fix .github workflow'",
            "cat .gitignore README.md",
            "cargo test --lib",
            "ls src/environment.rs",
        ] {
            assert_eq!(
                run(Mode::Bypass, "Bash", json!({ "command": cmd })),
                Auto::Allow("mode"),
                "{cmd}"
            );
        }
        for p in [
            "src/.github/ci.yml",
            ".gitignore",
            "src/environment.ts",
            "keys.md",
            "a.keyboard",
        ] {
            assert_eq!(
                run(Mode::Bypass, "Write", json!({"file_path": p})),
                Auto::Allow("mode"),
                "{p}"
            );
        }
    }

    #[test]
    fn a_work_tree_under_a_dot_folder_does_not_protect_its_files() {
        let work = Path::new("/home/u/.omp/crew/c1/work");
        let input = json!({"file_path": "/home/u/.omp/crew/c1/work/src/a.rs"});
        let c = Call {
            tool: "Edit",
            input: &input,
            work_dir: Some(work),
            no_auto: false,
        };
        assert_eq!(classify(Mode::Bypass, &c, |_| false), Auto::Allow("mode"));
        let git = json!({"file_path": "/home/u/.omp/crew/c1/work/.git/config"});
        let c = Call { input: &git, ..c };
        assert_eq!(
            classify(Mode::Bypass, &c, |_| false),
            Auto::Hold(Some("protected_path"))
        );
    }

    #[test]
    fn a_task_from_a_stranger_holds_in_every_mode() {
        let bash = json!({"command": "ls"});
        for mode in [Mode::Ask, Mode::Edits, Mode::Bypass] {
            assert_eq!(
                classify(mode, &call("Bash", &bash, true), |_| true),
                Auto::Hold(Some("no_auto")),
                "{mode:?}: not even a grant"
            );
        }
    }

    #[test]
    fn a_grant_covers_the_same_kind_of_call_and_comes_after_the_mode() {
        let bash = json!({"command": "cargo test"});
        let other = json!({"command": "rm -rf /"});
        let key = grant_key(&call("Bash", &bash, false));
        let granted = |k: &str| k == key;
        assert_eq!(
            classify(Mode::Ask, &call("Bash", &bash, false), granted),
            Auto::Allow("grant")
        );
        assert_eq!(
            classify(Mode::Ask, &call("Bash", &other, false), granted),
            Auto::Hold(None),
            "another command is not covered"
        );
        // Edits are one kind: any file in the tree, once one was allowed for the task.
        let a = json!({"file_path": format!("{WORK}/a.rs")});
        let b = json!({"file_path": format!("{WORK}/b/c.rs")});
        assert_eq!(
            grant_key(&call("Edit", &a, false)),
            grant_key(&call("Write", &b, false))
        );
        // The mode comes first: bypass says `mode`, not `grant`.
        assert_eq!(
            classify(Mode::Bypass, &call("Bash", &bash, false), granted),
            Auto::Allow("mode")
        );
    }

    #[test]
    fn the_fragment_states_the_mode_the_rails_and_what_to_do_when_denied() {
        for (mode, runs) in [
            (Mode::Ask, "every tool call waits"),
            (
                Mode::Edits,
                "file edits inside your working tree run without asking",
            ),
            (Mode::Bypass, "every tool call runs without asking"),
        ] {
            let f = fragment(mode, false);
            assert!(
                f.contains(&format!("Your permission mode is {}", mode_name(mode))),
                "{f}"
            );
            assert!(f.contains(runs), "{f}");
            assert!(
                f.contains(".git") && f.contains(".env") && f.contains("rc files"),
                "{f}"
            );
            assert!(f.contains("find a safer path, or ask the user"), "{f}");
            assert!(!f.contains("GitHub or a webhook, so"), "{f}");
        }
        assert!(
            fragment(Mode::Bypass, true).contains("GitHub or a webhook, so every tool call waits")
        );
    }

    #[test]
    fn a_symlink_out_of_the_tree_is_not_inside_it() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, work.join("link")).unwrap();
        let input = json!({"file_path": work.join("link/new.txt").to_string_lossy()});
        let c = Call {
            tool: "Write",
            input: &input,
            work_dir: Some(&work),
            no_auto: false,
        };
        #[cfg(unix)]
        assert_eq!(classify(Mode::Edits, &c, |_| false), Auto::Hold(None));
        let inside = json!({"file_path": work.join("sub/new.txt").to_string_lossy()});
        let c = Call {
            input: &inside,
            ..c
        };
        assert_eq!(classify(Mode::Edits, &c, |_| false), Auto::Allow("mode"));
    }

    fn bash(command: &str) -> bool {
        let input = json!({"command": command});
        destructive(&call("Bash", &input, false))
    }

    // #648: a spoken allow never approves these; one miss is a destructive command approved by voice, one false
    // hit only asks for a tap.
    #[test]
    fn destructive_calls_are_the_ones_that_destroy_data() {
        for cmd in [
            "rm -rf build",
            "rm -r dir",
            "rm -fr x",
            "rm -Rf x",
            "rm --recursive x",
            "sudo rm -rf /",
            "FOO=1 rm -rf x",
            "sh -c \"rm -rf x\"",
            "find . -name '*.o' -exec rm -rf {} \\;",
            "ls | xargs rm -r",
            "nice -n 10 rm -rf x",
            "git -C repo push --force",
            "/bin/rm -rf x",
            "RM -RF x",
            "ls && rm -rf build",
            "echo ok; rm -r x",
            "cd x\nrm -rf .",
            "git push --force",
            "git push -f origin main",
            "git push --force-with-lease origin main",
            "git push origin +main",
            "git reset --hard HEAD~1",
            "git clean -fd",
            "git clean -f",
            "git branch -D old",
            "git branch --delete --force old",
            "psql -c 'DROP TABLE users'",
            "mysql -e \"drop database prod\"",
            "TRUNCATE users",
            "mkfs.ext4 /dev/sda1",
            "dd if=/dev/zero of=/dev/sda",
            "chmod -R 777 /",
            "echo x > /dev/sda",
            "cat img >/dev/nvme0n1",
        ] {
            assert!(bash(cmd), "{cmd:?} is destructive");
        }
        for cmd in [
            "git status",
            "git push origin main",
            "git push -u origin feature/x",
            "git reset --soft HEAD~1",
            "git clean -n",
            "git branch -d merged",
            "git branch -a",
            "git log --format=%h",
            "rm file.txt",
            "rm -i file.txt",
            "chmod 777 file",
            "chmod -R 755 dir",
            "cargo test -p thing",
            "echo hello > /dev/null",
            "grep -rn rm src",
            "echo 'rm -rf is dangerous'",
            "ls rm -r",
            "ls -R",
        ] {
            assert!(!bash(cmd), "{cmd:?} is not destructive");
        }
    }

    // The command can hide anywhere in the input: another key, an argv list, an MCP tool. An edit tool only names
    // files, so what it writes is never read as a command.
    #[test]
    fn destructive_reads_every_string_and_argv_but_not_what_an_edit_writes() {
        let nested = json!({"steps": [{"run": "make && rm -rf dist"}]});
        assert!(destructive(&call("Custom", &nested, false)));
        let argv = json!({"cmd": ["rm", "-rf", "x"]});
        assert!(destructive(&call("Exec", &argv, false)));
        let safe_argv = json!({"cmd": ["rm", "x"]});
        assert!(!destructive(&call("Exec", &safe_argv, false)));
        let write = json!({"file_path": "notes.md", "content": "run rm -rf / to wipe"});
        assert!(!destructive(&call("Write", &write, false)));
    }
}
