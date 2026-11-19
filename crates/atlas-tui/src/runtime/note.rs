//! The sentences the engine says about a service, as a stable code plus the values behind them.
//!
//! A scheduler note (`waiting for api (healthy)`), a critical alert, the reason a service is `failed` or
//! `degraded`, a docker failure: four kinds of reader take these, and they need different things.
//!
//! - **Machines** (the status file `.logs/.tui-status.json`, MCP `atlas_status`, the `reason` of
//!   `atlas dev --ci`, the logs) read [`Note::message`]: English, byte for byte what the engine always wrote.
//!   Nothing there is translated and nothing is added to those files.
//! - **People** read [`Note::render`]: the sentence of `code` in the display language, from the catalog
//!   (`i18n/messages/tui_engine`). A code this build does not know shows the English `message`.
//! - **The runner's wire** (`note` and `alert` events, the `note`/`probe_error`/`memory_leak` of `state` and `rows`)
//!   carries `code` + `args` next to the English `message` ([`Note::wire`]): additive, an old client reads the
//!   message it always read and a new one renders the code.
//! - **The event-log classifier** (`tui::app::update::classify_note`) decides by `code`, never by words.
//!
//! The English is not written twice: [`Note::new`] renders the catalog's English for the code, so the wire
//! message and the English a person sees are one text, and `engine_text_tests` pins that text against the bytes the
//! engine wrote before the codes existed.
//!
//! A value that is itself a sentence travels as a nested note (`{"code","args","message"}` in an arg): the docker
//! exit (`exit 1` or `killed by a signal`), the two halves of a joined note. [`Note::render_in`] renders it first.

use atlas_i18n::t_in;
use serde_json::{Map, Value, json};

/// One coded sentence: a stable `code`, the `args` it is made of and its English `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Dotted id (`sched.waiting_dep`), never reworded. `""` for a text a peer sent without one (an older runner).
    pub code: String,
    /// The values behind the sentence: numbers, strings, byte counts, nested notes.
    pub args: Map<String, Value>,
    /// English. What the status file, MCP, `--ci` and the logs carry.
    pub message: String,
}

/// Every code this build writes. A producer names one of these (`Note::new` checks in debug builds, a test reads the
/// source for each literal), and the desktop's catalog carries the same list (`engineText` test over the fixture
/// `__fixtures__/engine-wire.json`).
pub const CODES: &[&str] = &[
    "sched.stable",
    "sched.restarting",
    "sched.dependency_failed",
    "sched.waiting_dep",
    "sched.exited_restart",
    "sched.crashed_restart",
    "sched.restarting_in",
    "sched.gave_up",
    "sched.completed",
    "sched.exited_cleanly",
    "sched.exited",
    "sched.exited_error",
    "sched.still_starting_port",
    "sched.still_starting_log",
    "sched.waiting_port",
    "sched.waiting_log",
    "sched.start_timeout_port",
    "sched.start_timeout_log",
    "sched.done",
    "sched.restarted",
    "note.pair",
    "note.lines",
    "health.unhealthy_for",
    "health.hang",
    "health.hang_restarting",
    "health.hang_spent",
    "probe.ready_failed",
    "probe.repeating",
    "memory.over_warn",
    "memory.over_max",
    "memory.notify",
    "memory.stopped",
    "memory.restarting",
    "memory.back_under",
    "memory.leak_reloads",
    "memory.leak_restarts",
    "infra.port_closed",
    "docker.permission",
    "docker.daemon_down",
    "docker.compose_missing",
    "docker.compose_failed",
    "docker.exit_code",
    "docker.exit_signal",
    "docker.no_output",
    "docker.not_found",
    "docker.permission_run",
    "docker.run_failed",
    "docker.unreachable",
    "docker.recovered",
    "spawn.empty_command",
    "spawn.failed",
    "deps.exit",
    "deps.killed",
    "deps.could_not_run",
    "deps.not_found",
    "port.held",
    "port.held_cwd",
    "tunnel.failed",
];

impl Note {
    /// A sentence of this build: `code` must be one of [`CODES`], `args` a JSON object. The English is the catalog's.
    pub fn new(code: &'static str, args: Value) -> Self {
        debug_assert!(CODES.contains(&code), "unknown engine code {code}");
        let args = match args {
            Value::Object(o) => o,
            _ => Map::new(),
        };
        let message = text("en", code, &args).unwrap_or_else(|| {
            debug_assert!(false, "engine code {code} has no English or misses an arg");
            code.to_string()
        });
        Self {
            code: code.to_string(),
            args,
            message,
        }
    }

    /// A text with no code: what a peer from before the codes sent, or a message a person typed.
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            code: String::new(),
            args: Map::new(),
            message: message.into(),
        }
    }

    /// `{"code","args","message"}`: the shape of a coded text wherever it travels as JSON (the same three keys as
    /// `atlas_core::progress::Coded::wire`).
    pub fn wire(&self) -> Value {
        json!({"code": self.code, "args": self.args, "message": self.message})
    }

    /// The note in `v` (`code`, `args`, `message` as keys of a `note`/`alert` event or of a nested value). Never
    /// fails: a payload with no code is a [plain](Self::plain) text.
    pub fn from_wire(v: &Value) -> Self {
        Self {
            code: v["code"].as_str().unwrap_or_default().to_string(),
            args: v["args"].as_object().cloned().unwrap_or_default(),
            message: v["message"].as_str().unwrap_or_default().to_string(),
        }
    }

    /// Write `<field>_code` and `<field>_args` next to a row's English `<field>` (the `state`/`rows` events).
    pub fn put_flat(row: &mut Value, field: &str, note: Option<&Note>) {
        row[format!("{field}_code")] = note.map_or(Value::Null, |n| n.code.clone().into());
        row[format!("{field}_args")] = note.map_or(Value::Null, |n| Value::Object(n.args.clone()));
    }

    /// The note of a row's `<field>` (English), with the `<field>_code`/`<field>_args` an up-to-date runner adds.
    /// `None` when the row has no such text.
    pub fn from_flat(row: &Value, field: &str) -> Option<Self> {
        let message = row[field].as_str()?.to_string();
        Some(Self {
            code: row[format!("{field}_code")]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            args: row[format!("{field}_args")]
                .as_object()
                .cloned()
                .unwrap_or_default(),
            message,
        })
    }

    pub fn is(&self, code: &str) -> bool {
        self.code == code
    }

    /// The first line of the English message: what a one-line reader (the card note, `reason`) always showed of a
    /// multi-line failure.
    pub fn line(&self) -> String {
        self.message.lines().next().unwrap_or_default().to_string()
    }

    /// The sentence in the display language.
    pub fn render(&self) -> String {
        self.render_in(atlas_i18n::lang())
    }

    /// The sentence in `tag`; a code this build does not know, or one whose args are missing or mistyped, shows the
    /// English message.
    pub fn render_in(&self, tag: &str) -> String {
        text(tag, &self.code, &self.args).unwrap_or_else(|| self.message.clone())
    }

    /// The first line of [`Self::render`].
    pub fn render_line(&self) -> String {
        self.render().lines().next().unwrap_or_default().to_string()
    }

    /// `first · second`: two sentences on one line (a start wait and who holds the port; a reason and a suspected leak).
    pub fn pair(first: Note, second: Note) -> Self {
        Self::new(
            "note.pair",
            json!({"first": first.wire(), "second": second.wire()}),
        )
    }

    /// Two lines (a docker hint over the compose failure it explains).
    pub fn lines(first: Note, second: Note) -> Self {
        Self::new(
            "note.lines",
            json!({"first": first.wire(), "second": second.wire()}),
        )
    }
}

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Typed reads of the args of one note, rendering nested notes in the same language.
struct Args<'a> {
    tag: &'a str,
    map: &'a Map<String, Value>,
}

impl Args<'_> {
    /// A string, a number as its digits, or a nested note rendered in this language.
    fn s(&self, key: &str) -> Option<String> {
        match self.map.get(key)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            v @ Value::Object(_) => Some(Note::from_wire(v).render_in(self.tag)),
            _ => None,
        }
    }

    fn n(&self, key: &str) -> Option<u64> {
        self.map.get(key)?.as_u64()
    }

    /// A byte count as the engine always wrote it (`600 MB`, `2.1 GB`), with the decimal comma of pt-BR.
    fn size(&self, key: &str) -> Option<String> {
        let s = super::memory::fmt_size(self.n(key)?);
        Some(if self.tag.starts_with("pt") {
            s.replace('.', ",")
        } else {
            s
        })
    }
}

/// The sentence of `code` in `tag`, or `None` for a code this build does not know or an arg that is missing.
fn text(tag: &str, code: &str, args: &Map<String, Value>) -> Option<String> {
    let a = Args { tag, map: args };
    Some(match code {
        "sched.stable" => t_in!(tag, "tui_engine_sched_stable", secs = a.n("secs")?),
        "sched.restarting" => t_in!(tag, "tui_engine_sched_restarting"),
        "sched.dependency_failed" => {
            t_in!(tag, "tui_engine_sched_dependency_failed", dep = a.s("dep")?)
        }
        "sched.waiting_dep" => t_in!(
            tag,
            "tui_engine_sched_waiting_dep",
            dep = a.s("dep")?,
            condition = a.s("condition")?
        ),
        "sched.exited_restart" => t_in!(
            tag,
            "tui_engine_sched_exited_restart",
            secs = a.n("secs")?,
            attempt = a.n("attempt")?,
            limit = a.s("limit")?
        ),
        "sched.crashed_restart" => t_in!(
            tag,
            "tui_engine_sched_crashed_restart",
            secs = a.n("secs")?,
            attempt = a.n("attempt")?,
            limit = a.s("limit")?
        ),
        "sched.restarting_in" => t_in!(
            tag,
            "tui_engine_sched_restarting_in",
            secs = a.n("secs")?,
            attempt = a.n("attempt")?,
            limit = a.s("limit")?
        ),
        "sched.gave_up" => t_in!(tag, "tui_engine_sched_gave_up", attempts = a.n("attempts")?),
        "sched.completed" => t_in!(tag, "tui_engine_sched_completed"),
        "sched.exited_cleanly" => t_in!(tag, "tui_engine_sched_exited_cleanly"),
        "sched.exited" => t_in!(tag, "tui_engine_sched_exited"),
        "sched.exited_error" => t_in!(tag, "tui_engine_sched_exited_error"),
        "sched.still_starting_port" => t_in!(
            tag,
            "tui_engine_sched_still_starting_port",
            waited = a.n("waited")?,
            port = a.n("port")?
        ),
        "sched.still_starting_log" => t_in!(
            tag,
            "tui_engine_sched_still_starting_log",
            waited = a.n("waited")?
        ),
        "sched.waiting_port" => t_in!(
            tag,
            "tui_engine_sched_waiting_port",
            port = a.n("port")?,
            secs = a.n("secs")?
        ),
        "sched.waiting_log" => t_in!(tag, "tui_engine_sched_waiting_log", secs = a.n("secs")?),
        "sched.start_timeout_port" => t_in!(
            tag,
            "tui_engine_sched_start_timeout_port",
            port = a.n("port")?,
            secs = a.n("secs")?
        ),
        "sched.start_timeout_log" => t_in!(
            tag,
            "tui_engine_sched_start_timeout_log",
            secs = a.n("secs")?
        ),
        "sched.done" => t_in!(tag, "tui_engine_sched_done"),
        "sched.restarted" => t_in!(
            tag,
            "tui_engine_sched_restarted",
            attempts = a.n("attempts")?
        ),
        "note.pair" => t_in!(
            tag,
            "tui_engine_note_pair",
            first = a.s("first")?,
            second = a.s("second")?
        ),
        "note.lines" => t_in!(
            tag,
            "tui_engine_note_lines",
            first = a.s("first")?,
            second = a.s("second")?
        ),
        "health.unhealthy_for" => t_in!(
            tag,
            "tui_engine_health_unhealthy_for",
            secs = a.n("secs")?,
            misses = a.n("misses")?
        ),
        "health.hang" => t_in!(
            tag,
            "tui_engine_health_hang",
            misses = a.n("misses")?,
            timeout = a.n("timeout")?
        ),
        "health.hang_restarting" => t_in!(
            tag,
            "tui_engine_health_hang_restarting",
            misses = a.n("misses")?,
            timeout = a.n("timeout")?
        ),
        "health.hang_spent" => t_in!(tag, "tui_engine_health_hang_spent"),
        "probe.ready_failed" => {
            t_in!(tag, "tui_engine_probe_ready_failed", error = a.s("error")?)
        }
        "probe.repeating" => t_in!(tag, "tui_engine_probe_repeating", line = a.s("line")?),
        "memory.over_warn" => t_in!(
            tag,
            "tui_engine_memory_over_warn",
            used = a.size("used")?,
            warn = a.size("warn")?
        ),
        "memory.over_max" => t_in!(
            tag,
            "tui_engine_memory_over_max",
            used = a.size("used")?,
            max = a.size("max")?
        ),
        "memory.notify" => t_in!(
            tag,
            "tui_engine_memory_notify",
            used = a.size("used")?,
            max = a.size("max")?
        ),
        "memory.stopped" => t_in!(
            tag,
            "tui_engine_memory_stopped",
            used = a.size("used")?,
            max = a.size("max")?,
            restarts = a.n("restarts")?
        ),
        "memory.restarting" => t_in!(
            tag,
            "tui_engine_memory_restarting",
            used = a.size("used")?,
            max = a.size("max")?,
            attempt = a.n("attempt")?,
            limit = a.n("limit")?
        ),
        "memory.back_under" => t_in!(tag, "tui_engine_memory_back_under"),
        "memory.leak_reloads" => t_in!(
            tag,
            "tui_engine_memory_leak_reloads",
            growth = a.size("growth")?,
            cycles = a.n("cycles")?
        ),
        "memory.leak_restarts" => t_in!(
            tag,
            "tui_engine_memory_leak_restarts",
            growth = a.size("growth")?,
            cycles = a.n("cycles")?
        ),
        "infra.port_closed" => t_in!(
            tag,
            "tui_engine_infra_port_closed",
            port = a.n("port")?,
            secs = a.n("secs")?
        ),
        "docker.permission" => t_in!(tag, "tui_engine_docker_permission"),
        "docker.daemon_down" => t_in!(tag, "tui_engine_docker_daemon_down"),
        "docker.compose_missing" => t_in!(tag, "tui_engine_docker_compose_missing"),
        "docker.compose_failed" => t_in!(
            tag,
            "tui_engine_docker_compose_failed",
            exit = a.s("exit")?,
            detail = a.s("detail")?
        ),
        "docker.exit_code" => t_in!(tag, "tui_engine_docker_exit_code", code = a.n("code")?),
        "docker.exit_signal" => t_in!(tag, "tui_engine_docker_exit_signal"),
        "docker.no_output" => t_in!(tag, "tui_engine_docker_no_output"),
        "docker.not_found" => t_in!(tag, "tui_engine_docker_not_found", path = a.s("path")?),
        "docker.permission_run" => t_in!(
            tag,
            "tui_engine_docker_permission_run",
            error = a.s("error")?
        ),
        "docker.run_failed" => t_in!(tag, "tui_engine_docker_run_failed", error = a.s("error")?),
        "docker.unreachable" => t_in!(tag, "tui_engine_docker_unreachable"),
        "docker.recovered" => t_in!(tag, "tui_engine_docker_recovered"),
        "spawn.empty_command" => t_in!(tag, "tui_engine_spawn_empty_command"),
        "spawn.failed" => t_in!(
            tag,
            "tui_engine_spawn_failed",
            command = a.s("command")?,
            dir = a.s("dir")?,
            error = a.s("error")?
        ),
        "deps.exit" => t_in!(
            tag,
            "tui_engine_deps_exit",
            code = a.s("code")?,
            tail = a.s("tail")?
        ),
        "deps.killed" => t_in!(tag, "tui_engine_deps_killed", tail = a.s("tail")?),
        "deps.could_not_run" => t_in!(tag, "tui_engine_deps_could_not_run", error = a.s("error")?),
        "deps.not_found" => t_in!(
            tag,
            "tui_engine_deps_not_found",
            code = a.s("code")?,
            tail = a.s("tail")?,
            bin = a.s("bin")?,
            path = a.s("path")?
        ),
        "port.held" => t_in!(
            tag,
            "tui_engine_port_held",
            port = a.n("port")?,
            pid = a.n("pid")?,
            cmd = a.s("cmd")?,
            stop = a.s("stop")?
        ),
        "port.held_cwd" => t_in!(
            tag,
            "tui_engine_port_held_cwd",
            port = a.n("port")?,
            pid = a.n("pid")?,
            cmd = a.s("cmd")?,
            cwd = a.s("cwd")?,
            stop = a.s("stop")?
        ),
        "tunnel.failed" => t_in!(
            tag,
            "tui_engine_tunnel_failed",
            host = a.s("host")?,
            reason = a.s("reason")?
        ),
        _ => return None,
    })
}
