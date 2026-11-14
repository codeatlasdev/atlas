//! The shell line that runs an atlas.yaml action through the cache: the one place that turns the
//! `cache:` block into `atlas run --cache` flags, so the TUI and the runner agree.

use crate::heavy::{atlas_cli_word, sh_quote};

/// What the `cache:` block of an action declares (see [`Spec`](super::Spec) for the meaning).
#[derive(Debug, Clone, Copy, Default)]
pub struct Declared<'a> {
    pub inputs: &'a [String],
    pub outputs: &'a [String],
    pub env: &'a [String],
    pub tools: &'a [String],
}

/// `command` (a shell command line) run through the cache by the `atlas` CLI this machine uses,
/// behind the heavy queue too when `heavy`. The command stays ONE argument.
pub fn cached(label: &str, command: &str, heavy: bool, d: Declared<'_>) -> String {
    wrap_command(&atlas_cli_word(), label, command, heavy, d)
}

/// [`cached`] with the CLI to call given (a path or `atlas`).
pub fn wrap_command(
    atlas: &str,
    label: &str,
    command: &str,
    heavy: bool,
    d: Declared<'_>,
) -> String {
    let mut line = format!("{} run --cache", sh_quote(atlas));
    if heavy {
        line.push_str(" --heavy");
    }
    line.push_str(&format!(" --label {}", sh_quote(label)));
    for (flag, values) in [
        ("--input", d.inputs),
        ("--output", d.outputs),
        ("--env", d.env),
        ("--tool", d.tools),
    ] {
        for v in values {
            line.push_str(&format!(" {flag} {}", sh_quote(v)));
        }
    }
    line.push_str(&format!(" -- {}", sh_quote(command)));
    line
}
