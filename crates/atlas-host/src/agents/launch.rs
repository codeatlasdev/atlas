//! Starting an agent from a phone, done by hostd itself (the desktop may be closed): the argv, env and
//! folder of a new agent terminal. The terminal is an ordinary hostd pty whose pane key the agent's hooks
//! report under, exactly like a crewmate's; a desktop that opens later lists it with the other terminals.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::protocol::{Mode, PtyCreate, Scope};

/// omp resumes the folder's last session by default; a phone launch is a new task, so the same overlay as
/// the desktop's agent tab turns that off. Written once under the registry dir the desktop also uses.
fn omp_fresh_config(registry: &Path) -> Result<PathBuf, String> {
    let p = registry.join("desktop").join("omp-fresh.yml");
    if !p.exists() {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&p, "autoResume: false\n").map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(p)
}

/// Test-only: the program every launch starts instead of the real agent (the real ones are not on a CI box).
#[cfg(test)]
static TEST_BIN: parking_lot::Mutex<Option<PathBuf>> = parking_lot::Mutex::new(None);

/// Tests that override the launched program hold this for their whole run (the override is process-wide).
/// Its users are the Unix-only `mobile::tests`.
#[cfg(all(test, unix))]
pub(crate) static TEST_BIN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(all(test, unix))]
pub(crate) fn set_test_bin(bin: Option<PathBuf>) {
    *TEST_BIN.lock() = bin;
}

/// The agent program on the login-shell PATH (what the user's terminal would run).
pub fn resolve_bin(agent: &str) -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(b) = TEST_BIN.lock().clone() {
        return Ok(b);
    }
    let def = atlas_core::agents::get(agent).ok_or_else(|| format!("unknown agent {agent}"))?;
    atlas_core::login_env::which(def.bin)
        .ok_or_else(|| format!("{} is not installed (not on PATH)", def.bin))
}

/// The terminal request for `agent` in `cwd` (a registered worktree's folder) as pane `pane`. `bin` is the
/// resolved agent program; `worktree_id` and `pane` are what the agent's hooks report under.
pub fn plan(
    registry: &Path,
    agent: &str,
    bin: &Path,
    cwd: &Path,
    worktree_id: &str,
    pane: &str,
    prompt: Option<&str>,
) -> Result<PtyCreate, String> {
    let def = atlas_core::agents::get(agent).ok_or_else(|| format!("unknown agent {agent}"))?;
    let mut argv = def.argv(false, prompt);
    argv[0] = bin.to_string_lossy().into_owned();
    if def.id == "omp" {
        let overlay = omp_fresh_config(registry)?;
        argv.insert(1, format!("--config={}", overlay.display()));
    }
    let env = BTreeMap::from([
        ("ATLAS_WORKTREE_ID".to_string(), worktree_id.to_string()),
        ("ATLAS_TAB_ID".to_string(), pane.to_string()),
        ("ATLAS_AGENT".to_string(), def.id.to_string()),
    ]);
    Ok(PtyCreate {
        cwd: cwd.to_string_lossy().into_owned(),
        argv,
        env,
        cols: 120,
        rows: 40,
        scope: Scope {
            worktree_id: Some(worktree_id.to_string()),
            tab_id: Some(pane.to_string()),
            pane_key: Some(pane.to_string()),
        },
        title: Some(def.label.to_string()),
        mode: Mode::Pty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_starts_in_the_worktree_folder_and_reports_under_its_pane() {
        let reg = tempfile::tempdir().unwrap();
        let p = plan(
            reg.path(),
            "claude",
            Path::new("/bin/claude"),
            Path::new("/w/shop/fix"),
            "wt1",
            "pane1",
            Some("fix the bug"),
        )
        .unwrap();
        assert_eq!(p.cwd, "/w/shop/fix");
        assert_eq!(p.argv, ["/bin/claude", "fix the bug"]);
        assert_eq!(p.scope.pane_key.as_deref(), Some("pane1"));
        assert_eq!(p.scope.worktree_id.as_deref(), Some("wt1"));
        assert_eq!(p.env["ATLAS_WORKTREE_ID"], "wt1");
        assert_eq!(p.env["ATLAS_AGENT"], "claude");
        assert_eq!(p.title.as_deref(), Some("Claude Code"));
    }

    /// A phone never starts an agent without its permission prompts: no yolo flag whatever the prompt says.
    #[test]
    fn a_phone_launch_never_passes_the_yolo_flags() {
        let reg = tempfile::tempdir().unwrap();
        for a in atlas_core::agents::AGENTS {
            let p = plan(
                reg.path(),
                a.id,
                Path::new("/bin/x"),
                Path::new("/w"),
                "w",
                "p",
                Some("--dangerously-skip-permissions"),
            )
            .unwrap();
            for y in a.yolo_args {
                assert!(
                    !p.argv[..p.argv.len() - 1].contains(&y.to_string()),
                    "{}: {:?}",
                    a.id,
                    p.argv
                );
            }
        }
    }

    #[test]
    fn omp_gets_the_fresh_session_overlay_once_and_only_omp() {
        let reg = tempfile::tempdir().unwrap();
        let omp = plan(
            reg.path(),
            "omp",
            Path::new("/bin/omp"),
            Path::new("/w"),
            "w",
            "p",
            None,
        )
        .unwrap();
        let overlay = reg.path().join("desktop/omp-fresh.yml");
        assert_eq!(
            omp.argv,
            [
                "/bin/omp".to_string(),
                format!("--config={}", overlay.display())
            ]
        );
        assert_eq!(
            std::fs::read_to_string(&overlay).unwrap(),
            "autoResume: false\n"
        );
        let other = plan(
            reg.path(),
            "codex",
            Path::new("/bin/codex"),
            Path::new("/w"),
            "w",
            "p",
            None,
        )
        .unwrap();
        assert_eq!(other.argv, ["/bin/codex"]);
    }

    #[test]
    fn unknown_agents_and_blank_prompts_are_handled() {
        let reg = tempfile::tempdir().unwrap();
        assert!(
            plan(
                reg.path(),
                "rm -rf",
                Path::new("/b"),
                Path::new("/w"),
                "w",
                "p",
                None
            )
            .is_err()
        );
        let p = plan(
            reg.path(),
            "codex",
            Path::new("/bin/codex"),
            Path::new("/w"),
            "w",
            "p",
            Some("  "),
        )
        .unwrap();
        assert_eq!(p.argv, ["/bin/codex"], "a blank prompt adds nothing");
    }
}
