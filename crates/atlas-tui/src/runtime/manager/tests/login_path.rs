//! Binaries the login shell put on `PATH` are found by actions, services and session actions.

use super::*;

/// Seed the shared login-shell probe the way a desktop-entry launch would produce it: bare PATH, the
/// user's rc files add `~/.bun/bin`. Returns the fake HOME (keep it alive for the test).
#[cfg(unix)]
fn gui_login_env() -> Option<(tempfile::TempDir, atlas_core::login_env::Installed)> {
    use atlas_core::login_env::{fixtures, probe_with};
    let sh = fixtures::some_shell()?;
    let home = fixtures::fake_home();
    let p = probe_with(sh, home.path(), &fixtures::gui_env(home.path()));
    assert!(p.error.is_none(), "{:?}", p.error);
    let installed = atlas_core::login_env::install(p);
    Some((home, installed))
}

/// Incident (Linux, app launched from the menu): `exit 127: sh: 1: bun: not found`. Actions run with the
/// same env as services and `pre_command`.
// Login-shell probes of POSIX shells (bash, zsh, fish).
#[cfg(unix)]
#[tokio::test]
async fn an_action_finds_binaries_the_login_shell_put_on_path() {
    let Some(_env) = gui_login_env() else { return };
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let ok = run_action_streaming(
        "bun run thing",
        dir.path(),
        &HashMap::new(),
        &dir.path().join(".logs"),
        "Bun",
        &tx,
        None,
    )
    .await;
    drop(tx);
    let mut lines = Vec::new();
    while let Some(ev) = rx.recv().await {
        if let ManagerEvent::LogLine { line, .. } = ev {
            lines.push(line);
        }
    }
    assert!(ok, "action failed: {lines:?}");
    assert!(lines.iter().any(|l| l == "bun-ran run thing"), "{lines:?}");
}

// Login-shell probes of POSIX shells (bash, zsh, fish).
#[cfg(unix)]
#[tokio::test]
async fn a_service_finds_binaries_the_login_shell_put_on_path() {
    let Some(_env) = gui_login_env() else { return };
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cfg = one_service_cfg("bun dev; sleep 30", None);
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    mgr.start_service("svc").await;
    let line = tokio::time::timeout(atlas_core::scaling::HANG, async {
        while let Some(ev) = rx.recv().await {
            if let ManagerEvent::LogLine { line, .. } = ev {
                return line;
            }
        }
        String::new()
    })
    .await
    .unwrap_or_default();
    mgr.stop_service("svc").await;
    assert_eq!(line, "bun-ran dev");
}

// Login-shell probes of POSIX shells (bash, zsh, fish).
#[cfg(unix)]
#[tokio::test]
async fn a_session_action_finds_binaries_the_login_shell_put_on_path() {
    let Some(_env) = gui_login_env() else { return };
    let dir = tempfile::TempDir::new().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut cfg = one_service_cfg("sleep 30", None);
    cfg.actions = vec![
        serde_norway::from_str(
            "key: m\nlabel: Mob\ncommand: 'bun tauri ios dev'\nsession: true\nplatform: ios\n",
        )
        .unwrap(),
    ];
    let mut mgr = ServiceManager::new(dir.path().to_path_buf(), &cfg, tx);
    let _ = mgr.run_session('m').await;
    let line = tokio::time::timeout(atlas_core::scaling::HANG, async {
        while let Some(ev) = rx.recv().await {
            if let ManagerEvent::LogLine { line, .. } = ev
                && line.starts_with("bun-ran")
            {
                return Some(line);
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    mgr.stop_session("Mob").await;
    assert_eq!(line.as_deref(), Some("bun-ran tauri ios dev"));
}
