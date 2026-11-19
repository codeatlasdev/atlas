//! The machine channel of a long CLI command: events a program reads, never translated.
//!
//! Human text (`▸ staging atlas binary`, `error: ...`) is for people and is translated, so no
//! consumer may parse it. A command that something else drives (Atlas Desktop running
//! `atlas crew install`) takes `--progress json` and prints one JSON object per line on stdout:
//!
//! ```json
//! {"v":1,"kind":"step","code":"service","args":{},"message":"service"}
//! {"v":1,"kind":"error","code":"sudo.password_rejected","args":{"target":"me@srv"},"message":"sudo password rejected: check your password and try again"}
//! ```
//!
//! - `kind`: `step` (a phase started), `info` (a fact the client needs, such as the user an install
//!   created), `hint` (a next action the reader can take), `error` (the command failed; always the
//!   last event).
//! - `code`: a stable dotted id, never translated, never reworded: the consumer branches on it.
//! - `args`: the values behind the message (a target, a path, a count), for a client that
//!   renders its own text from `code`.
//! - `message`: English, for logs and for clients that do not know the `code` yet.
//!
//! In this mode stdout carries events only; the human lines go to stderr. Add a `code` here and in
//! the consumer's table in the same change; a test in `atlas-cli` runs every command listed in
//! `tests/machine_output.rs` under `ATLAS_LANG=en` and `ATLAS_LANG=pt-BR` and compares the bytes.

use serde_json::{Map, Value, json};

/// The version of the event shape. Bump only for a change an old client cannot read.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Step,
    Info,
    Hint,
    Error,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Step => "step",
            Kind::Info => "info",
            Kind::Hint => "hint",
            Kind::Error => "error",
        }
    }
}

/// One event as the line printed on stdout (no trailing newline). `args` must be a JSON object.
pub fn event_line(kind: Kind, code: &str, args: Value, message: &str) -> String {
    debug_assert!(args.is_object(), "args of {code} must be an object");
    json!({
        "v": VERSION,
        "kind": kind.as_str(),
        "code": code,
        "args": args,
        "message": message,
    })
    .to_string()
}

/// An error that carries its stable code, so the machine channel never has to guess from text.
/// Wrap the English message at the place the failure is created.
#[derive(Debug, Clone, PartialEq)]
pub struct Coded {
    pub code: &'static str,
    pub args: Map<String, Value>,
    pub message: String,
}

impl Coded {
    pub fn new(code: &'static str, args: Value, message: impl Into<String>) -> Self {
        Self {
            code,
            args: args.as_object().cloned().unwrap_or_default(),
            message: message.into(),
        }
    }

    /// `{"code","args","message"}`: the three keys of a coded text wherever it travels as JSON (a `--progress json`
    /// event adds `v` and `kind` around them; the desktop's `host://toast` and `host://error` carry them as they
    /// are). A client renders its own sentence from `code` and `args`; `message` is for logs and for a client that
    /// does not know the code.
    pub fn wire(&self) -> Value {
        json!({"code": self.code, "args": self.args, "message": self.message})
    }
}

impl std::fmt::Display for Coded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Coded {}

/// The `error` event for `err`: the code of the first [`Coded`] in its chain, else `error`.
pub fn error_line(err: &anyhow::Error) -> String {
    match err.chain().find_map(|e| e.downcast_ref::<Coded>()) {
        Some(c) => event_line(
            Kind::Error,
            c.code,
            Value::Object(c.args.clone()),
            &err.to_string(),
        ),
        None => event_line(Kind::Error, "error", json!({}), &err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_is_one_json_line_with_the_documented_keys() {
        let line = event_line(
            Kind::Step,
            "check",
            json!({"target": "me@srv"}),
            "preflight",
        );
        assert!(!line.contains('\n'));
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["v"], 1);
        assert_eq!(v["kind"], "step");
        assert_eq!(v["code"], "check");
        assert_eq!(v["args"]["target"], "me@srv");
        assert_eq!(v["message"], "preflight");
    }

    #[test]
    fn the_error_event_takes_its_code_from_the_chain_not_from_the_text() {
        let coded = anyhow::Error::new(Coded::new(
            "sudo.password_rejected",
            json!({"target": "me@srv"}),
            "sudo password rejected",
        ))
        .context("installing");
        let v: Value = serde_json::from_str(&error_line(&coded)).unwrap();
        assert_eq!(v["code"], "sudo.password_rejected");
        assert_eq!(v["args"]["target"], "me@srv");
        // The message is the whole chain's top line, so it stays readable in a log.
        assert_eq!(v["message"], "installing");
        // An error nobody coded is still an event, with the generic code.
        let plain: Value = serde_json::from_str(&error_line(&anyhow::anyhow!("boom"))).unwrap();
        assert_eq!(
            (plain["code"].as_str(), plain["message"].as_str()),
            (Some("error"), Some("boom"))
        );
    }
}
