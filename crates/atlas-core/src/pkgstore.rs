//! A machine-wide package store for the setup of a new worktree (#130).
//!
//! Every worktree runs its own `bun install` / `pnpm install`. Pointed at one store under
//! `~/.atlas/pkgstore`, the package manager downloads and unpacks a package once and every
//! worktree gets it by clone or hardlink out of that store (bun and pnpm both materialise
//! `node_modules` from their store; a warm store makes an install a link job, not a download).
//!
//! [`env`] says which variables to set for a project: it picks the manager from the lockfile,
//! never touches npm or yarn (no store with the same guarantees), never overrides a variable the
//! user already set, and is empty when the project opted out. Pure over its inputs, so it is
//! testable without a package manager.

use std::path::{Path, PathBuf};

/// Overrides the store root (an absolute path), like `ATLAS_CACHE_DIR` does for the task cache.
pub const ENV_DIR: &str = "ATLAS_PKGSTORE_DIR";

/// A package manager with a configurable content-addressed store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Bun,
    Pnpm,
}

impl Manager {
    /// The environment variable that moves the store, and the folder under the store root.
    /// pnpm reads `npm_config_*` variables as config, `store-dir` included.
    fn var(self) -> &'static str {
        match self {
            Manager::Bun => "BUN_INSTALL_CACHE_DIR",
            Manager::Pnpm => "npm_config_store_dir",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Manager::Bun => "bun",
            Manager::Pnpm => "pnpm",
        }
    }
}

/// The manager whose lockfile is in `root`. bun wins when both exist (its lockfile is the newer
/// choice in a repo that migrated); npm and yarn lockfiles give `None`.
pub fn detect(root: &Path) -> Option<Manager> {
    if root.join("bun.lock").is_file() || root.join("bun.lockb").is_file() {
        Some(Manager::Bun)
    } else if root.join("pnpm-lock.yaml").is_file() {
        Some(Manager::Pnpm)
    } else {
        None
    }
}

/// `$ATLAS_PKGSTORE_DIR` (absolute), else `~/.atlas/pkgstore`.
pub fn default_root() -> PathBuf {
    root_from(
        std::env::var_os(ENV_DIR).as_deref(),
        &crate::paths::atlas_home(),
    )
}

/// [`default_root`] for an explicit override and Atlas home (pure, for tests).
pub fn root_from(over: Option<&std::ffi::OsStr>, atlas_home: &Path) -> PathBuf {
    match over.map(Path::new) {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        _ => atlas_home.join("pkgstore"),
    }
}

/// Variables to add to the worktree setup so its package manager uses the shared store. Empty
/// when `enabled` is false (the `worktree.package_store: false` opt-out), when no bun/pnpm
/// lockfile is in `root`, or when `is_set` says the user already chose a store for that manager.
pub fn env(
    root: &Path,
    store: &Path,
    enabled: bool,
    is_set: impl Fn(&str) -> bool,
) -> Vec<(&'static str, String)> {
    let Some(manager) = detect(root).filter(|_| enabled) else {
        return Vec::new();
    };
    if is_set(manager.var()) {
        return Vec::new();
    }
    vec![(
        manager.var(),
        store.join(manager.folder()).to_string_lossy().into_owned(),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            std::fs::write(d.path().join(f), "").unwrap();
        }
        d
    }

    fn vars(files: &[&str], enabled: bool, set: &[&str]) -> Vec<(&'static str, String)> {
        let d = project(files);
        env(d.path(), Path::new("/store"), enabled, |k| set.contains(&k))
    }

    // #130: a bun project's setup installs from the machine-wide store.
    #[test]
    fn a_bun_project_gets_the_bun_cache_pointed_at_the_store() {
        let expect = Path::new("/store")
            .join("bun")
            .to_string_lossy()
            .into_owned();
        for lock in ["bun.lock", "bun.lockb"] {
            assert_eq!(
                vars(&["package.json", lock], true, &[]),
                vec![("BUN_INSTALL_CACHE_DIR", expect.clone())]
            );
        }
    }

    // #130: pnpm reads its store-dir from npm_config_store_dir.
    #[test]
    fn a_pnpm_project_gets_its_store_dir_pointed_at_the_store() {
        let expect = Path::new("/store")
            .join("pnpm")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            vars(&["package.json", "pnpm-lock.yaml"], true, &[]),
            vec![("npm_config_store_dir", expect)]
        );
    }

    // #130: npm and yarn have no store with these guarantees, and a project with no lockfile
    // is not a node project we can reason about: nothing is set.
    #[test]
    fn npm_yarn_and_lockfile_less_projects_are_left_alone() {
        assert!(vars(&["package.json", "package-lock.json"], true, &[]).is_empty());
        assert!(vars(&["package.json", "yarn.lock"], true, &[]).is_empty());
        assert!(vars(&["package.json"], true, &[]).is_empty());
        assert!(vars(&[], true, &[]).is_empty());
    }

    // #130: `worktree.package_store: false` is a hard off.
    #[test]
    fn the_opt_out_sets_nothing() {
        assert!(vars(&["bun.lock"], false, &[]).is_empty());
        assert!(vars(&["pnpm-lock.yaml"], false, &[]).is_empty());
    }

    // #130: a store the user already chose (their own CI-like setup) is never replaced.
    #[test]
    fn a_store_the_user_already_chose_is_respected() {
        assert!(vars(&["bun.lock"], true, &["BUN_INSTALL_CACHE_DIR"]).is_empty());
        assert!(vars(&["pnpm-lock.yaml"], true, &["npm_config_store_dir"]).is_empty());
        // A bun variable says nothing about pnpm.
        assert_eq!(
            vars(&["pnpm-lock.yaml"], true, &["BUN_INSTALL_CACHE_DIR"]).len(),
            1
        );
    }

    // Both lockfiles: one manager, deterministic.
    #[test]
    fn bun_wins_when_both_lockfiles_exist() {
        let d = project(&["bun.lock", "pnpm-lock.yaml"]);
        assert_eq!(detect(d.path()), Some(Manager::Bun));
    }

    // A relative override would fragment the store per worktree (the opposite of the point).
    #[test]
    fn the_store_root_honours_an_absolute_override_only() {
        let home = Path::new("/home/me/.atlas");
        // Absolute on every OS (`/mnt/...` is not absolute on Windows).
        let fast = std::env::temp_dir().join("fast-pkgs");
        assert_eq!(root_from(Some(fast.as_os_str()), home), fast);
        let rel = std::ffi::OsString::from("relative/dir");
        assert_eq!(
            root_from(Some(rel.as_os_str()), home),
            home.join("pkgstore")
        );
        assert_eq!(root_from(None, home), home.join("pkgstore"));
    }
}
