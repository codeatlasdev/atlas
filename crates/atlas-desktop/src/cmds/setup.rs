//! The dependency-install step of a project: what it would run, its last result, and
//! "Run now".

use std::collections::BTreeSet;
use std::path::Path;

use atlas_tui::config;
use atlas_tui::runtime::{self, deps};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use super::{load_for, root_of};

/// The dependency-install step: effective command (auto-detected when no
/// `pre_command`), schedule and whether it is auto-detected. `command` is null
/// when there is nothing to run (`pre_command: ""`, or no `package.json`).
pub(super) fn setup_info(root: &Path, cfg: &config::ProjectConfig) -> Value {
    let pre = cfg.pre_command.as_deref();
    let command = match pre {
        Some("") => None,
        Some(c) => Some(c.to_string()),
        None => deps::detect_install_command(root).map(str::to_string),
    };
    let when = match deps::effective_when(pre, cfg.pre_command_when) {
        config::PreCommandWhen::Always => "always",
        config::PreCommandWhen::Lockfile => "lockfile",
    };
    json!({ "command": command, "when": when, "auto": pre.is_none() })
}

/// Projects with a `setup_run` in flight (one install per project at a time).
static SETUP_RUNNING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Releases the project's slot in `SETUP_RUNNING` on every exit path.
struct SetupSlot(String);

impl SetupSlot {
    fn take(id: &str) -> Option<Self> {
        SETUP_RUNNING
            .lock()
            .insert(id.to_string())
            .then(|| Self(id.to_string()))
    }
}

impl Drop for SetupSlot {
    fn drop(&mut self) {
        SETUP_RUNNING.lock().remove(&self.0);
    }
}

/// Output lines `setup_status` returns.
const SETUP_OUTPUT_LINES: usize = 200;

/// `{ status: <.logs/setup.json or null>, output: <last 200 lines of setup.log> }`.
#[tauri::command]
pub fn setup_status(id: String) -> Result<Value, String> {
    let dir = runtime::log_dir(&root_of(&id)?);
    let status = std::fs::read_to_string(dir.join(deps::SETUP_STATUS))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let log = std::fs::read_to_string(dir.join(deps::SETUP_LOG)).unwrap_or_default();
    let lines: Vec<&str> = log.lines().collect();
    let output = lines[lines.len().saturating_sub(SETUP_OUTPUT_LINES)..].join("\n");
    Ok(json!({ "status": status, "output": output }))
}

/// "Run now": the dependency install, skipping the `when` gate. Returns
/// `{ ok, reason }` and emits `projects://setup` with the id when it finishes.
#[tauri::command]
pub async fn setup_run(app: AppHandle, id: String) -> Result<Value, String> {
    let (root, cfg) = load_for(&id)?;
    let _slot = SetupSlot::take(&id).ok_or("setup is already running")?;
    // Seen by setup_status/`projects://setup` listeners before the process ends.
    let outcome = deps::ensure_deps(
        &root,
        cfg.pre_command.as_deref(),
        cfg.pre_command_when,
        true,
    )
    .await;
    let _ = app.emit("projects://setup", &id);
    Ok(match outcome {
        // The setup strip shows the reason to a person: the engine's sentence in the display language.
        deps::DepsOutcome::Failed(reason) => json!({ "ok": false, "reason": reason.render() }),
        deps::DepsOutcome::Installed => json!({ "ok": true }),
        deps::DepsOutcome::UpToDate => json!({ "ok": true, "skipped": true }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(yaml: &str) -> config::ProjectConfig {
        serde_norway::from_str(&format!("name: t\nservices: {{}}\n{yaml}")).unwrap()
    }

    #[test]
    fn setup_info_reports_effective_command_and_schedule() {
        let dir = tempfile::tempdir().unwrap();
        // No pre_command, no package.json: nothing to run.
        let none = setup_info(dir.path(), &cfg(""));
        assert!(none["command"].is_null());
        assert_eq!(none["when"], "lockfile");
        assert_eq!(none["auto"], true);

        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("bun.lock"), "").unwrap();
        let auto = setup_info(dir.path(), &cfg(""));
        assert_eq!(auto["command"], "bun install");
        assert_eq!(auto["when"], "lockfile");

        let explicit = setup_info(dir.path(), &cfg("pre_command: bun install\n"));
        assert_eq!(explicit["command"], "bun install");
        assert_eq!(explicit["when"], "always");
        assert_eq!(explicit["auto"], false);

        let gated = setup_info(
            dir.path(),
            &cfg("pre_command: make deps\npre_command_when: lockfile\n"),
        );
        assert_eq!(gated["when"], "lockfile");

        let off = setup_info(dir.path(), &cfg("pre_command: \"\"\n"));
        assert!(off["command"].is_null());
    }
}
