//! A crewmate's permission mode: what its agent may do without asking a person (#642).

use serde::{Deserialize, Serialize};

/// One neutral mode, mapped per engine by hostd (it decides every tool call, `crew::approval`).
///
/// | mode | what runs without asking |
/// |---|---|
/// | `ask` | nothing |
/// | `edits` | file edits inside the crewmate's work tree |
/// | `bypass` | every tool call, except the rails no mode removes |
///
/// On disk and on the wire it is `mode`. Files and clients from before the modes say `approval: ask|auto`:
/// `auto` reads as `bypass` (the alias), and the JSON hostd sends still carries that legacy `approval`
/// ([`Mode::legacy`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Ask,
    Edits,
    #[serde(alias = "auto")]
    Bypass,
}

impl Mode {
    /// The wire name (`ask`, `edits`, `bypass`).
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Ask => "ask",
            Mode::Edits => "edits",
            Mode::Bypass => "bypass",
        }
    }

    /// The pre-modes `approval` value: `auto` exactly when the mode is `bypass`.
    pub fn legacy(self) -> &'static str {
        if self == Mode::Bypass { "auto" } else { "ask" }
    }

    /// A mode named on the wire: `ask`, `edits`, `bypass`, or the legacy `auto`.
    pub fn parse(s: &str) -> Result<Self, String> {
        serde_json::from_value(serde_json::json!(s)).map_err(|_| format!("invalid mode: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_legacy_approval_values_read_as_modes_and_modes_write_the_legacy_value() {
        assert_eq!(Mode::parse("ask"), Ok(Mode::Ask));
        assert_eq!(Mode::parse("auto"), Ok(Mode::Bypass), "auto is bypass");
        assert_eq!(Mode::parse("edits"), Ok(Mode::Edits));
        assert!(Mode::parse("yolo").is_err());
        assert_eq!(Mode::Bypass.legacy(), "auto");
        assert_eq!((Mode::Ask.legacy(), Mode::Edits.legacy()), ("ask", "ask"));
        for m in [Mode::Ask, Mode::Edits, Mode::Bypass] {
            assert_eq!(Mode::parse(m.as_str()), Ok(m));
        }
    }
}
