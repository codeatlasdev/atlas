//! Config validation: unknown-key detection (schema-driven, with "did you
//! mean" suggestions) and semantic checks (dependencies, cycles, ports, keys).
//!
//! Every problem carries a stable `code` and the `args` behind its sentence next to the English
//! `message`. A program reads `code`, `args` and `message` (never translated); a person reads the
//! sentence the catalog renders from `code` and `args` in the display language
//! ([`ConfigError::render_in`], [`render_errors_in`]).

mod schema;
mod semantic;

#[cfg(test)]
mod samples;
#[cfg(test)]
mod tests;

use std::fmt;

use serde_json::{Map, Value};

pub use schema::{Target, json_schema, unknown_keys};
pub use semantic::{semantic, warnings};

/// Keys bound by the dashboard (`tui/app/input.rs`); an action using one would
/// never fire (built-ins win) or would shadow navigation.
pub const BUILTIN_KEYS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'j', 'k', 'g', 'G', 'f', 'v', 'E', 'M', 'L',
    'P', 'q', 'r', 's', 'S', 't', 'm', 'i', '?', ':', '/', 'n', 'N', 'W', 'T', 'V', 'J', 'z', '-',
    'D', '[', ']', '{', '}',
];

/// The stable ids of the problems the validators emit. A code never changes meaning and is never
/// reused; each one has a message `tui_config_problems_<code without "config.", dots as _>` in the
/// catalog, whose English is the `message` the validator writes for the same `args`.
pub mod code {
    pub const UNKNOWN_KEY: &str = "config.unknown_key";
    pub const UNKNOWN_KEY_SUGGEST: &str = "config.unknown_key_suggest";
    pub const REMOTE_COMMAND: &str = "config.remote_command";
    pub const REMOTE_PORT_MISSING: &str = "config.remote_port_missing";
    pub const REMOTE_SSH_INVALID: &str = "config.remote_ssh_invalid";
    pub const REMOTE_HOST_INVALID: &str = "config.remote_host_invalid";
    pub const DEPENDENCY_SELF: &str = "config.dependency_self";
    pub const UNKNOWN_SERVICE: &str = "config.unknown_service";
    pub const UNKNOWN_SERVICE_SUGGEST: &str = "config.unknown_service_suggest";
    pub const CONDITION_LOG_READY: &str = "config.condition_log_ready";
    pub const CONDITION_COMPLETED: &str = "config.condition_completed";
    pub const DEPENDENCY_CYCLE: &str = "config.dependency_cycle";
    pub const SIZE_INVALID: &str = "config.size_invalid";
    pub const MEMORY_WARN_ABOVE_MAX: &str = "config.memory_warn_above_max";
    pub const READY_LOG_REGEX: &str = "config.ready_log_regex";
    pub const READY_EXCLUSIVE: &str = "config.ready_exclusive";
    pub const PORT_DUPLICATE: &str = "config.port_duplicate";
    pub const KEY_BUILTIN: &str = "config.key_builtin";
    pub const ACTION_KEY_OPEN_KEY: &str = "config.action_key_open_key";
    pub const ACTION_KEY_DUPLICATE: &str = "config.action_key_duplicate";
    pub const WORKTREE_SETUP_UNKNOWN_ACTION: &str = "config.worktree_setup_unknown_action";
    pub const OPEN_TUNNEL_WITHOUT_TUNNEL: &str = "config.open_tunnel_without_tunnel";
    pub const AFFECTED_COMMAND_BLANK: &str = "config.affected_command_blank";
    pub const AFFECTED_RULE_NO_MATCH: &str = "config.affected_rule_no_match";
    pub const AFFECTED_BAD_GLOB: &str = "config.affected_bad_glob";
    pub const AFFECTED_TASK_UNKNOWN: &str = "config.affected_task_unknown";
    pub const DEPLOY_APP_PATTERN: &str = "config.deploy_app_pattern";
    pub const DEPLOY_TARGETS_EMPTY: &str = "config.deploy_targets_empty";
    pub const DEPLOY_SERVICES_EMPTY: &str = "config.deploy_services_empty";
    pub const DEPLOY_KEEP_MIN: &str = "config.deploy_keep_min";
    pub const DEPLOY_PATH_ABSOLUTE: &str = "config.deploy_path_absolute";
    pub const DEPLOY_LINK_KEY: &str = "config.deploy_link_key";
    pub const DEPLOY_LINK_VALUE: &str = "config.deploy_link_value";
    pub const DEPLOY_SERVICE_DUPLICATE: &str = "config.deploy_service_duplicate";
    pub const DEPLOY_RESTART_REQUIRED: &str = "config.deploy_restart_required";
    pub const DEPLOY_RESTART_ONLY_COMMAND: &str = "config.deploy_restart_only_command";
    pub const DEPLOY_COMPOSE_FILE_ONLY_COMPOSE: &str = "config.deploy_compose_file_only_compose";
    pub const WARN_AUTOSTART_FALSE: &str = "config.warn_autostart_false";
    pub const WARN_HEALTHY_NO_PROBE: &str = "config.warn_healthy_no_probe";

    /// Every code above (a test checks that the validators' sources name no other).
    #[cfg(test)]
    pub const ALL: &[&str] = &[
        UNKNOWN_KEY,
        UNKNOWN_KEY_SUGGEST,
        REMOTE_COMMAND,
        REMOTE_PORT_MISSING,
        REMOTE_SSH_INVALID,
        REMOTE_HOST_INVALID,
        DEPENDENCY_SELF,
        UNKNOWN_SERVICE,
        UNKNOWN_SERVICE_SUGGEST,
        CONDITION_LOG_READY,
        CONDITION_COMPLETED,
        DEPENDENCY_CYCLE,
        SIZE_INVALID,
        MEMORY_WARN_ABOVE_MAX,
        READY_LOG_REGEX,
        READY_EXCLUSIVE,
        PORT_DUPLICATE,
        KEY_BUILTIN,
        ACTION_KEY_OPEN_KEY,
        ACTION_KEY_DUPLICATE,
        WORKTREE_SETUP_UNKNOWN_ACTION,
        OPEN_TUNNEL_WITHOUT_TUNNEL,
        AFFECTED_COMMAND_BLANK,
        AFFECTED_RULE_NO_MATCH,
        AFFECTED_BAD_GLOB,
        AFFECTED_TASK_UNKNOWN,
        DEPLOY_APP_PATTERN,
        DEPLOY_TARGETS_EMPTY,
        DEPLOY_SERVICES_EMPTY,
        DEPLOY_KEEP_MIN,
        DEPLOY_PATH_ABSOLUTE,
        DEPLOY_LINK_KEY,
        DEPLOY_LINK_VALUE,
        DEPLOY_SERVICE_DUPLICATE,
        DEPLOY_RESTART_REQUIRED,
        DEPLOY_RESTART_ONLY_COMMAND,
        DEPLOY_COMPOSE_FILE_ONLY_COMPOSE,
        WARN_AUTOSTART_FALSE,
        WARN_HEALTHY_NO_PROBE,
    ];
}

/// One problem found in the config. `path` is the dotted key path (`services.web.helth`),
/// `code` its stable id, `args` the values behind the sentence (a key, a service, a port) and
/// `message` the English sentence, kept byte for byte for every program that reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: String,
    pub path: String,
    pub code: &'static str,
    pub args: Map<String, Value>,
    pub message: String,
}

impl ConfigError {
    /// A problem of `code` at `path`; `args` must be a JSON object. The file is filled in by the loader.
    pub(crate) fn new(
        path: impl Into<String>,
        code: &'static str,
        args: Value,
        message: impl Into<String>,
    ) -> Self {
        debug_assert!(args.is_object(), "args of {code} must be an object");
        Self {
            file: String::new(),
            path: path.into(),
            code,
            args: args.as_object().cloned().unwrap_or_default(),
            message: message.into(),
        }
    }

    /// The sentence in `tag` rendered from the catalog; `None` when the catalog has no message for the code.
    fn catalog_text(&self, tag: &str) -> Option<String> {
        // i18n-keys: tui_config_problems_*
        let key = format!(
            "tui_config_problems_{}",
            self.code
                .strip_prefix("config.")
                .unwrap_or(self.code)
                .replace('.', "_")
        );
        let mut args = atlas_i18n::Args::new();
        for (name, value) in &self.args {
            match value {
                Value::String(s) => args.set(name, s.as_str()),
                Value::Number(n) => args.set(name, n.as_f64().unwrap_or_default()),
                other => args.set(name, other.to_string()),
            }
        }
        atlas_i18n::lookup(tag, &key, Some(&args))
    }

    /// The sentence in `tag`, rendered from the catalog; the English `message` for a code the catalog
    /// does not have.
    pub fn text_in(&self, tag: &str) -> String {
        self.catalog_text(tag)
            .unwrap_or_else(|| self.message.clone())
    }

    /// `path: sentence` in `tag` (the path is a key path, never translated).
    pub fn render_in(&self, tag: &str) -> String {
        if self.path.is_empty() {
            self.text_in(tag)
        } else {
            format!("{}: {}", self.path, self.text_in(tag))
        }
    }

    /// [`render_in`](Self::render_in) in the display language: what a person reads.
    pub fn render(&self) -> String {
        self.render_in(atlas_i18n::lang())
    }
}

/// The English line, `path: message`: for programs, logs and tests. A person reads [`ConfigError::render`].
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// One line per error, prefixed with the file it came from, in the display language.
pub fn render_errors(errors: &[ConfigError], default_file: &str) -> String {
    render_errors_in(atlas_i18n::lang(), errors, default_file)
}

/// [`render_errors`] in `tag`. A command whose output a program reads passes
/// `atlas_core::lang::DEFAULT`.
pub fn render_errors_in(tag: &str, errors: &[ConfigError], default_file: &str) -> String {
    let mut out = atlas_i18n::t_in!(tag, "tui_config_problems_header", count = errors.len());
    for e in errors {
        let file = if e.file.is_empty() {
            default_file
        } else {
            &e.file
        };
        out.push_str(&format!("\n  {file}: {}", e.render_in(tag)));
    }
    out
}

/// Closest candidate within a distance that scales with the key length.
pub fn suggest<'a>(key: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let max = (key.chars().count() / 3).clamp(1, 3);
    candidates
        .map(|c| (levenshtein(key, c), c))
        .filter(|(d, _)| *d <= max)
        .min_by_key(|(d, c)| (*d, *c))
        .map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
