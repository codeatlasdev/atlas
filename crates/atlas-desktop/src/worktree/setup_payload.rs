//! The `setup` half of the `worktree://progress` `done` payload: the argv the frontend runs in the
//! "Setup" terminal and its environment. Owns two #130 behaviours: `{{action: X}}` steps of
//! `worktree.setup` become cached `atlas run --cache` lines, and a bun/pnpm project's install is
//! pointed at the machine-wide package store (`atlas_core::pkgstore`).

use std::path::Path;

use atlas_tui::config::ProjectConfig;
use serde_json::{Value, json};

use super::create::hook_env;
use super::exec::shell_argv;

/// `Ok(None)` when there is no setup to run. `store_vars` are added to the hook environment
/// (the package-store variables for a local checkout, none elsewhere).
pub(super) fn build(
    host: &str,
    cfg: &ProjectConfig,
    root: &Path,
    wt: &Path,
    name: &str,
    offset: u16,
    store_vars: &[(&'static str, String)],
) -> Result<Option<Value>, String> {
    let Some(cmd) = cfg.setup_command()? else {
        return Ok(None);
    };
    let mut env = hook_env(root, wt, name, offset);
    if let Some(map) = env.as_object_mut() {
        for (k, v) in store_vars {
            map.insert((*k).into(), json!(v));
        }
    }
    Ok(Some(json!({"argv": shell_argv(host, &cmd), "env": env})))
}

/// The package-store variables for `root` as configured: local checkouts only (the store is a
/// directory of this machine), skipped when the project opted out or the user's own environment
/// already names a store.
pub(super) fn store_vars(
    host: &str,
    cfg: &ProjectConfig,
    root: &Path,
) -> Vec<(&'static str, String)> {
    if host != crate::host::LOCAL {
        return Vec::new();
    }
    let enabled = cfg.worktree.as_ref().is_none_or(|w| w.package_store);
    atlas_core::pkgstore::env(root, &atlas_core::pkgstore::default_root(), enabled, |k| {
        std::env::var_os(k).is_some_and(|v| !v.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(yaml: &str) -> (tempfile::TempDir, ProjectConfig) {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("atlas.yaml"), yaml).unwrap();
        let c = atlas_tui::config::load(d.path()).unwrap();
        (d, c)
    }

    const CACHED: &str = "name: t\nservices:\n  a:\n    command: x\nactions:\n  - key: b\n    label: Build\n    command: bun run build\n    cache: { inputs: ['src/**'], outputs: [dist] }\nworktree:\n  setup: 'bun install && {{action: Build}}'\n";

    // #130: the payload the Setup terminal runs carries the cached build line, and the env keeps
    // the four hook variables next to the store variables.
    #[test]
    fn the_setup_payload_runs_the_cached_action_and_carries_hook_and_store_env() {
        let (d, c) = cfg(CACHED);
        let store = vec![("BUN_INSTALL_CACHE_DIR", "/s/bun".to_string())];
        let p = build(
            crate::host::LOCAL,
            &c,
            d.path(),
            Path::new("/wt/amber"),
            "amber",
            100,
            &store,
        )
        .unwrap()
        .unwrap();
        let script = p["argv"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_str()
            .unwrap();
        assert!(script.starts_with("bun install && "), "{script}");
        assert!(script.contains(" run --cache --label Build"), "{script}");
        assert_eq!(p["env"]["BUN_INSTALL_CACHE_DIR"], "/s/bun");
        assert_eq!(p["env"]["ATLAS_PORT_OFFSET"], "100");
        assert_eq!(p["env"]["ATLAS_WORKSPACE_NAME"], "amber");
    }

    #[test]
    fn no_setup_means_no_payload_and_a_bad_reference_is_an_error() {
        let (d, c) = cfg("name: t\nservices:\n  a:\n    command: x\n");
        let none = build(crate::host::LOCAL, &c, d.path(), d.path(), "n", 0, &[]).unwrap();
        assert!(none.is_none());
        let (d, mut c) = cfg(CACHED);
        c.actions.clear();
        let err = build(crate::host::LOCAL, &c, d.path(), d.path(), "n", 0, &[]).unwrap_err();
        assert!(err.contains("Build"), "{err}");
    }

    // #130: the package store is local-only and honours `package_store: false`.
    #[test]
    fn the_package_store_is_local_only_and_honours_the_opt_out() {
        let yaml = "name: t\nservices:\n  a:\n    command: x\nworktree:\n  setup: bun install\n";
        let (d, c) = cfg(yaml);
        std::fs::write(d.path().join("bun.lock"), "").unwrap();
        // The test process's own env may set a store; only assert the shape when it does not.
        if std::env::var_os("BUN_INSTALL_CACHE_DIR").is_none() {
            let v = store_vars(crate::host::LOCAL, &c, d.path());
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].0, "BUN_INSTALL_CACHE_DIR");
        }
        assert!(store_vars("ssh://box", &c, d.path()).is_empty());
        let (d, c) = cfg(&format!("{yaml}  package_store: false\n"));
        std::fs::write(d.path().join("bun.lock"), "").unwrap();
        assert!(store_vars(crate::host::LOCAL, &c, d.path()).is_empty());
    }
}
