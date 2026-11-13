//! Seeding a new worktree with the heavy, gitignored directories of its main checkout
//! (`node_modules`, `target`) by copy-on-write clone, so the setup command
//! (`bun install`, `cargo build`) starts from a warm tree and only does the difference.
//!
//! Planning ([`plan`]) is pure file-system inspection; [`run`] applies a plan through
//! [`crate::sys::clone_dir`]. A file system that cannot clone is reported and skipped: this
//! module never falls back to a full copy (slow, and the opposite of the point) and never
//! hardlinks (a write in the worktree would change the main checkout). A Python virtualenv
//! (a directory with `pyvenv.cfg`) is refused: it is not relocatable, a clone would keep
//! running the main checkout's interpreter.
//!
//! Cargo: a cloned `target/` is safe. Cargo's metadata hash and fingerprints include the
//! workspace's absolute path, so every workspace crate and test binary is rebuilt under new
//! names in the worktree (the baked `CARGO_BIN_EXE_*` paths of the main checkout stay in dead
//! files nobody runs), while registry dependencies, the expensive part, are reused. The clone
//! owns its inodes, so removing the main checkout later breaks nothing.

use std::io;
use std::path::{Component, Path};
use std::time::Instant;

use serde::Serialize;

/// Marker that keeps Spotlight out of a tree (macOS indexes new `target/` dirs by the
/// hundred thousand files otherwise).
const NEVER_INDEX: &str = ".metadata_never_index";

/// Which heavy directory a marker file in the repo root implies, in the order seeded.
const DETECT: [(&str, &[&str]); 2] = [
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
];

/// Why a directory is not seeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason", content = "detail")]
pub enum Skip {
    /// The main checkout has no such directory.
    Missing,
    /// The worktree already has something at that path (git checked it out, or a `shared` link).
    Exists,
    /// The path is absolute or climbs out of the repo.
    Invalid,
    /// A Python virtualenv (`pyvenv.cfg` inside): its scripts and config hold the absolute path
    /// of the main checkout, so a clone would run the main checkout's interpreter.
    NotRelocatable,
    /// The file system cannot clone (not APFS, another volume, ext4, Windows without ReFS).
    Unsupported,
    /// The clone failed; the partial copy was removed.
    Failed(String),
}

/// One directory to seed, relative to both checkouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub rel: String,
}

/// What [`plan`] decided: directories to clone and directories left out, with the reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub items: Vec<Item>,
    pub skipped: Vec<(String, Skip)>,
}

/// Outcome of [`run`], serialized into the worktree progress payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Directories cloned.
    pub seeded: Vec<String>,
    /// Directories left out and why.
    pub skipped: Vec<(String, Skip)>,
    /// Wall time of the whole seeding step.
    pub ms: u64,
}

/// Which directories to seed. `configured` is `worktree.seed` from `atlas.yaml`: `None` detects
/// from the repo root (`package.json` → `node_modules`, `Cargo.toml` → `target`), `Some(&[])`
/// turns seeding off. Entries are deduplicated; a path that is missing in `root`, already
/// present in `wt`, not a plain relative path, or a Python virtualenv is skipped with its
/// reason.
pub fn plan(root: &Path, wt: &Path, configured: Option<&[String]>) -> Plan {
    let wanted: Vec<String> = match configured {
        Some(list) => list.to_vec(),
        None => DETECT
            .iter()
            .filter(|(_, markers)| markers.iter().any(|m| root.join(m).is_file()))
            .map(|(dir, _)| (*dir).to_string())
            .collect(),
    };
    let mut plan = Plan::default();
    let mut seen = std::collections::HashSet::new();
    for rel in wanted {
        let rel = rel.trim().trim_end_matches('/').to_string();
        if rel.is_empty() || !seen.insert(rel.clone()) {
            continue;
        }
        let skip = if !is_plain_relative(&rel) {
            Some(Skip::Invalid)
        } else if root.join(&rel).symlink_metadata().is_err() {
            Some(Skip::Missing)
        } else if root.join(&rel).join("pyvenv.cfg").is_file() {
            Some(Skip::NotRelocatable)
        } else if wt.join(&rel).symlink_metadata().is_ok() {
            Some(Skip::Exists)
        } else {
            None
        };
        match skip {
            Some(s) => plan.skipped.push((rel, s)),
            None => plan.items.push(Item { rel }),
        }
    }
    plan
}

/// Only normal path components: no `..`, no root, no drive prefix, no `.`.
fn is_plain_relative(rel: &str) -> bool {
    Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

/// Clone every planned directory from `root` into `wt`, timing the step. A directory that fails
/// or cannot be cloned is skipped (partial copies removed) and does not stop the others. On
/// macOS the seeded `target` and `node_modules` get a `.metadata_never_index` marker.
pub fn run(root: &Path, wt: &Path, plan: Plan) -> Report {
    let started = Instant::now();
    let mut report = Report {
        skipped: plan.skipped,
        ..Report::default()
    };
    // Unsupported is a property of the volume pair, not of the directory: stop asking after one.
    let mut unsupported = false;
    for item in plan.items {
        if unsupported {
            report.skipped.push((item.rel, Skip::Unsupported));
            continue;
        }
        let (src, dst) = (root.join(&item.rel), wt.join(&item.rel));
        if let Some(parent) = dst.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            report.skipped.push((item.rel, Skip::Failed(e.to_string())));
            continue;
        }
        match crate::sys::clone_dir(&src, &dst) {
            Ok(()) => {
                hide_from_spotlight(&dst, &item.rel);
                report.seeded.push(item.rel);
            }
            Err(e) if e.kind() == io::ErrorKind::Unsupported => {
                unsupported = true;
                report.skipped.push((item.rel, Skip::Unsupported));
            }
            Err(e) => report.skipped.push((item.rel, Skip::Failed(e.to_string()))),
        }
    }
    report.ms = started.elapsed().as_millis() as u64;
    report
}

/// `.metadata_never_index` inside a seeded `target` or `node_modules` (macOS only; the marker
/// is harmless elsewhere but pointless, so it is not written).
fn hide_from_spotlight(dst: &Path, rel: &str) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let leaf = Path::new(rel).file_name().and_then(|n| n.to_str());
    if matches!(leaf, Some("target" | "node_modules")) {
        let _ = std::fs::write(dst.join(NEVER_INDEX), "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write(p: &Path, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    /// A main checkout with a node_modules tree (nested dir, symlink, executable file) and a
    /// separate empty worktree dir, both under one temp dir (same volume).
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let (root, wt) = (t.path().join("main"), t.path().join("wt"));
        fs::create_dir_all(&wt).unwrap();
        write(&root.join("package.json"), "{}");
        write(&root.join("node_modules/a/index.js"), "module.exports = 1");
        write(&root.join("node_modules/a/deep/b.txt"), "b");
        write(&root.join("node_modules/.bin/tool"), "#!/bin/sh\necho hi\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                root.join("node_modules/.bin/tool"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            std::os::unix::fs::symlink("a/index.js", root.join("node_modules/link")).unwrap();
        }
        (t, root, wt)
    }

    fn rels(p: &Plan) -> Vec<&str> {
        p.items.iter().map(|i| i.rel.as_str()).collect()
    }

    #[test]
    fn detects_directories_from_the_repo_markers_and_skips_the_missing_ones() {
        let (_t, root, wt) = fixture();
        write(&root.join("Cargo.toml"), "");
        // package.json -> node_modules (exists); Cargo.toml -> target is implied but absent in
        // the main checkout, so it is skipped, not planned.
        let p = plan(&root, &wt, None);
        assert_eq!(rels(&p), ["node_modules"]);
        assert_eq!(p.skipped, [("target".to_string(), Skip::Missing)]);
    }

    // A Python virtualenv is not relocatable: bin/* shebangs and pyvenv.cfg hold the absolute
    // path of the main checkout, so a clone would run the MAIN checkout's interpreter and
    // site-packages (editable installs of the worktree silently unused). It is never detected,
    // and refused when listed, whatever the folder is called (`.venv`, `venv`, `env`, custom).
    #[test]
    fn a_python_virtualenv_is_never_detected_and_is_refused_when_listed() {
        let (_t, root, wt) = fixture();
        write(&root.join("pyproject.toml"), "");
        write(&root.join("requirements.txt"), "");
        for venv in [".venv", "venv", "tools/py-env"] {
            write(&root.join(venv).join("pyvenv.cfg"), "home = /usr/bin\n");
            write(&root.join(venv).join("bin/python"), "");
        }
        let detected = plan(&root, &wt, None);
        assert_eq!(rels(&detected), ["node_modules"]);
        assert!(detected.skipped.is_empty(), "{:?}", detected.skipped);

        let list: Vec<String> = [".venv", "venv", "tools/py-env", "node_modules"]
            .map(String::from)
            .into();
        let p = plan(&root, &wt, Some(&list));
        assert_eq!(rels(&p), ["node_modules"]);
        assert_eq!(
            p.skipped,
            [
                (".venv".to_string(), Skip::NotRelocatable),
                ("venv".to_string(), Skip::NotRelocatable),
                ("tools/py-env".to_string(), Skip::NotRelocatable),
            ]
        );
        let rep = run(&root, &wt, p);
        assert!(!wt.join(".venv").exists() && !wt.join("venv").exists());
        assert!(
            rep.skipped
                .iter()
                .all(|(_, s)| *s == Skip::NotRelocatable || *s == Skip::Unsupported)
        );
    }

    // The check is on content, not name: a plain `.venv` folder with no pyvenv.cfg is not a
    // virtualenv, and a pyvenv.cfg deeper down (not at the top level) does not make one.
    #[test]
    fn only_a_top_level_pyvenv_cfg_makes_a_directory_a_virtualenv() {
        let (_t, root, wt) = fixture();
        write(&root.join(".venv/lib/x.py"), "");
        write(&root.join("dist/deep/pyvenv.cfg"), "");
        let list: Vec<String> = [".venv", "dist"].map(String::from).into();
        let p = plan(&root, &wt, Some(&list));
        assert_eq!(rels(&p), [".venv", "dist"]);
    }

    #[test]
    fn detection_ignores_a_directory_whose_marker_file_is_absent() {
        let (_t, root, wt) = fixture();
        // A stray target/ with no Cargo.toml is not a Rust project: nothing implies it.
        write(&root.join("target/debug/x"), "");
        let p = plan(&root, &wt, None);
        assert_eq!(rels(&p), ["node_modules"]);
        assert!(p.skipped.is_empty());
    }

    #[test]
    fn configured_list_replaces_detection_and_empty_list_turns_seeding_off() {
        let (_t, root, wt) = fixture();
        write(&root.join("dist/out.js"), "x");
        let only_dist = ["dist".to_string()];
        assert_eq!(rels(&plan(&root, &wt, Some(&only_dist))), ["dist"]);
        let none: [String; 0] = [];
        let off = plan(&root, &wt, Some(&none));
        assert!(off.items.is_empty() && off.skipped.is_empty());
    }

    #[test]
    fn configured_entries_are_deduplicated_trimmed_and_validated() {
        let (_t, root, wt) = fixture();
        let list: Vec<String> = [
            "node_modules",
            "node_modules/",
            " node_modules ",
            "../outside",
            "/abs",
            "a/../b",
            "./node_modules",
            "",
        ]
        .map(String::from)
        .into();
        let p = plan(&root, &wt, Some(&list));
        assert_eq!(rels(&p), ["node_modules"]);
        let invalid: Vec<&str> = p
            .skipped
            .iter()
            .filter(|(_, s)| *s == Skip::Invalid)
            .map(|(r, _)| r.as_str())
            .collect();
        assert_eq!(invalid, ["../outside", "/abs", "a/../b", "./node_modules"]);
    }

    #[test]
    fn a_path_the_worktree_already_has_is_never_overwritten() {
        let (_t, root, wt) = fixture();
        write(&wt.join("node_modules/keep.txt"), "mine");
        let p = plan(&root, &wt, None);
        assert!(p.items.is_empty());
        assert_eq!(p.skipped, [("node_modules".to_string(), Skip::Exists)]);
        let rep = run(&root, &wt, p);
        assert!(rep.seeded.is_empty());
        assert_eq!(
            fs::read_to_string(wt.join("node_modules/keep.txt")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn a_dangling_symlink_at_the_destination_counts_as_existing() {
        let (_t, root, wt) = fixture();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/nonexistent/shared", wt.join("node_modules")).unwrap();
            assert_eq!(
                plan(&root, &wt, None).skipped,
                [("node_modules".to_string(), Skip::Exists)]
            );
        }
    }

    // The next tests need a file system that can clone (APFS, btrfs, XFS reflink). Where the
    // temp dir is on one that cannot (ext4, tmpfs CI), they assert the clean fallback instead,
    // so the suite is meaningful and green on every runner.
    fn clone_supported() -> bool {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("s");
        write(&src.join("f"), "x");
        crate::sys::clone_dir(&src, &t.path().join("d")).is_ok()
    }

    #[test]
    fn a_seeded_tree_has_the_same_content_and_is_independent_of_the_source() {
        let (_t, root, wt) = fixture();
        let rep = run(&root, &wt, plan(&root, &wt, None));
        if !clone_supported() {
            assert!(rep.seeded.is_empty());
            assert_eq!(
                rep.skipped,
                [("node_modules".to_string(), Skip::Unsupported)]
            );
            assert!(
                !wt.join("node_modules").exists(),
                "no half copy left behind"
            );
            return;
        }
        assert_eq!(rep.seeded, ["node_modules"]);
        let (s, d) = (root.join("node_modules"), wt.join("node_modules"));
        assert_eq!(
            fs::read_to_string(d.join("a/index.js")).unwrap(),
            "module.exports = 1"
        );
        assert_eq!(fs::read_to_string(d.join("a/deep/b.txt")).unwrap(), "b");
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            assert_eq!(
                fs::metadata(d.join(".bin/tool"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
            assert_eq!(
                fs::read_link(d.join("link")).unwrap(),
                Path::new("a/index.js")
            );
            // Not a hardlink: separate inodes, so a write cannot reach the main checkout.
            assert_ne!(
                fs::metadata(s.join("a/index.js")).unwrap().ino(),
                fs::metadata(d.join("a/index.js")).unwrap().ino()
            );
        }
        // Writing the clone leaves the source alone, and the other way round.
        fs::write(d.join("a/index.js"), "CHANGED").unwrap();
        fs::write(d.join("new.txt"), "n").unwrap();
        assert_eq!(
            fs::read_to_string(s.join("a/index.js")).unwrap(),
            "module.exports = 1"
        );
        assert!(!s.join("new.txt").exists());
        fs::write(s.join("a/deep/b.txt"), "SRC").unwrap();
        assert_eq!(fs::read_to_string(d.join("a/deep/b.txt")).unwrap(), "b");
        // Deleting the source (the main checkout going away) leaves the clone whole.
        fs::remove_dir_all(&s).unwrap();
        assert_eq!(fs::read_to_string(d.join("a/deep/b.txt")).unwrap(), "b");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seeded_target_and_node_modules_are_hidden_from_spotlight_but_other_dirs_are_not() {
        if !clone_supported() {
            return;
        }
        let (_t, root, wt) = fixture();
        write(&root.join("Cargo.toml"), "");
        write(&root.join("target/debug/x"), "x");
        write(&root.join("dist/o.js"), "o");
        let list: Vec<String> = ["node_modules", "target", "dist"].map(String::from).into();
        let rep = run(&root, &wt, plan(&root, &wt, Some(&list)));
        assert_eq!(rep.seeded, ["node_modules", "target", "dist"]);
        assert!(wt.join("node_modules").join(NEVER_INDEX).exists());
        assert!(wt.join("target").join(NEVER_INDEX).exists());
        assert!(!wt.join("dist").join(NEVER_INDEX).exists());
        // The marker is written into the clone only: the main checkout is untouched.
        assert!(!root.join("node_modules").join(NEVER_INDEX).exists());
    }

    #[test]
    fn report_serializes_with_the_keys_the_frontend_reads() {
        let rep = Report {
            seeded: vec!["node_modules".into()],
            skipped: vec![
                ("target".into(), Skip::Unsupported),
                ("cache".into(), Skip::Failed("boom".into())),
                (".venv".into(), Skip::NotRelocatable),
            ],
            ms: 12,
        };
        let v = serde_json::to_value(&rep).unwrap();
        assert_eq!(v["seeded"][0], "node_modules");
        assert_eq!(v["ms"], 12);
        assert_eq!(v["skipped"][0][0], "target");
        assert_eq!(v["skipped"][0][1]["reason"], "unsupported");
        assert_eq!(v["skipped"][1][1]["reason"], "failed");
        assert_eq!(v["skipped"][1][1]["detail"], "boom");
        assert_eq!(v["skipped"][2][1]["reason"], "not_relocatable");
    }

    #[test]
    fn a_failed_directory_is_reported_and_the_next_one_is_still_seeded() {
        let (_t, root, wt) = fixture();
        if !clone_supported() {
            return;
        }
        // `blocker` is a FILE in the worktree where the destination's parent should be.
        write(&root.join("a/b/c.txt"), "c");
        write(&wt.join("a"), "i am a file");
        let list: Vec<String> = ["a/b", "node_modules"].map(String::from).into();
        let rep = run(&root, &wt, plan(&root, &wt, Some(&list)));
        assert_eq!(rep.seeded, ["node_modules"]);
        assert!(matches!(&rep.skipped[0], (r, Skip::Failed(_)) if r == "a/b"));
    }

    /// The point of seeding, end to end on real files: a git worktree of a real repo is seeded,
    /// then its setup command (`sh -c`, cwd = worktree, as the desktop runs it) finds the warm
    /// `node_modules` and only adds the missing package, leaving the main checkout untouched.
    #[cfg(unix)]
    #[test]
    fn a_seeded_git_worktree_runs_its_setup_incrementally_without_touching_the_main_checkout() {
        if !clone_supported() {
            return;
        }
        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
                .arg(dir)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        let t = tempfile::tempdir().unwrap();
        let (root, wt) = (t.path().join("main"), t.path().join("wt"));
        write(&root.join("package.json"), "{}");
        write(&root.join(".gitignore"), "node_modules\n");
        write(&root.join("node_modules/old/index.js"), "old");
        git(t.path(), &["init", "-q", root.to_str().unwrap()]);
        git(&root, &["add", "package.json", ".gitignore"]);
        git(&root, &["commit", "-q", "-m", "init"]);
        git(
            &root,
            &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "feat"],
        );
        // Git checked out the tracked files only: node_modules is ignored, so it is absent.
        assert!(!wt.join("node_modules").exists());

        let rep = run(&root, &wt, plan(&root, &wt, None));
        assert_eq!(rep.seeded, ["node_modules"]);

        // The "install": keeps what is there, adds what is missing (what bun/npm do on a warm tree).
        let setup = "test -f node_modules/old/index.js && mkdir -p node_modules/new && echo added > node_modules/new/index.js";
        let out = crate::sys::shell(setup).current_dir(&wt).output().unwrap();
        assert!(
            out.status.success(),
            "setup ran against a cold tree: {out:?}"
        );
        assert_eq!(
            fs::read_to_string(wt.join("node_modules/new/index.js")).unwrap(),
            "added\n"
        );
        assert!(
            !root.join("node_modules/new").exists(),
            "setup leaked into the main checkout"
        );
        assert_eq!(
            fs::read_to_string(root.join("node_modules/old/index.js")).unwrap(),
            "old"
        );
    }
}
