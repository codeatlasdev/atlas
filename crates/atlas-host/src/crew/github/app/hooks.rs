//! The git hooks of a crewmate that works as the GitHub App's bot: a `commit-msg` hook that adds the
//! `Co-authored-by:` trailer of the person who asked, when the task has one and the message lacks it. Enforced by git,
//! not asked of the agent in a prompt.
//!
//! `core.hooksPath` replaces the repository's own hooks, so the directory holds one dispatcher under every client-side
//! hook name: it does the one thing above for `commit-msg` and then runs the repository's own hook of that name, found
//! the way git would have found it without the override (its `core.hooksPath`, as husky sets, or `.git/hooks`). The
//! files belong to hostd and are never writable by a crewmate. `git commit --no-verify` skips `commit-msg` hooks,
//! as it skips every one.

use std::path::{Path, PathBuf};

/// Client-side hooks a repository can have (`githooks(5)`); the dispatcher is installed under each.
const HOOKS: [&str; 17] = [
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-auto-gc",
    "post-rewrite",
    "reference-transaction",
    "push-to-checkout",
    "post-index-change",
];

/// The dispatcher. `ATLAS_CO_AUTHOR` is `Name <email>` of the requester (not a secret); the repository's own
/// configuration is read with git's `GIT_CONFIG_COUNT` override switched off, which is what hides it from `git` here.
pub(crate) const DISPATCHER: &str = r#"#!/bin/sh
# Atlas Crew git hook dispatcher: see crates/atlas-host/src/crew/github/app/hooks.rs
hook=${0##*/}
if [ "$hook" = commit-msg ] && [ -n "$ATLAS_CO_AUTHOR" ]; then
  line="Co-authored-by: $ATLAS_CO_AUTHOR"
  grep -qxF -- "$line" "$1" || git interpret-trailers --in-place --if-exists addIfDifferent --trailer "$line" "$1" || exit 1
fi
own=$(GIT_CONFIG_COUNT=0 git config --get core.hooksPath 2>/dev/null)
[ -n "$own" ] || own=$(GIT_CONFIG_COUNT=0 git rev-parse --git-path hooks 2>/dev/null)
case "$own" in "~/"*) own="$HOME/${own#"~/"}" ;; esac
if [ -n "$own" ] && [ -x "$own/$hook" ] && ! [ "$own/$hook" -ef "$0" ]; then
  exec "$own/$hook" "$@"
fi
exit 0
"#;

/// `<dir>/githooks`, with the dispatcher under every hook name (mode 0755, replaced only when it differs). `dir` is
/// where the agent can read what hostd writes for it: the crewmate's directory, or `crew-runtime/` when isolated.
#[cfg(unix)]
pub(crate) fn ensure(dir: &Path) -> std::io::Result<PathBuf> {
    let hooks = dir.join("githooks");
    // tree-access: hostd's own runtime directory, written for the crewmate to read and never writable by it
    std::fs::create_dir_all(&hooks)?;
    for name in HOOKS {
        let p = hooks.join(name);
        // tree-access: hostd's own runtime file
        if std::fs::read_to_string(&p).is_ok_and(|s| s == DISPATCHER) {
            continue;
        }
        // tree-access: hostd's own runtime file, written atomically
        atlas_core::fs::write_atomic(
            &p,
            DISPATCHER.as_bytes(),
            // tree-access: as above
            atlas_core::fs::Mode::Exact(0o755),
        )?;
    }
    Ok(hooks)
}
