//! The files of a folder that is not a git repository, for Quick Open (`git ls-files` has nothing to say there).
//!
//! The same walk the task cache uses for its inputs (the `ignore` crate: `.gitignore` applies without a `.git`), with
//! the folders nobody searches skipped, and bounded in files and time, so `~/Downloads` or a home folder cannot hang
//! the palette.

use std::path::Path;
use std::time::{Duration, Instant};

use ignore::WalkBuilder;

/// Folders skipped wherever they are: dependencies, build output and caches.
const SKIP_DIRS: [&str; 12] = [
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    "coverage",
    "vendor",
    "__pycache__",
    "venv",
    "Pods",
    "DerivedData",
    "Library",
];

/// Dot folders that are worth searching; every other one (`.git`, `.cache`, `.next`, `.venv`, `.Trash`…) is skipped.
const DOT_DIRS: [&str; 4] = [".github", ".vscode", ".atlas", ".husky"];

/// Files that are never worth listing.
const SKIP_FILES: [&str; 2] = [".DS_Store", "Thumbs.db"];

/// What a walk found: paths relative to the root with `/` separators, sorted by name within each folder, and whether
/// the file or time bound cut it short.
#[derive(Debug, PartialEq, Eq)]
pub struct Listing {
    pub files: Vec<String>,
    pub truncated: bool,
}

fn wanted_dir(name: &str) -> bool {
    if name.starts_with('.') {
        DOT_DIRS.contains(&name)
    } else {
        !SKIP_DIRS.contains(&name)
    }
}

/// The files under `root`, at most `limit`, for at most `budget`. Symlinks are not followed. A folder that cannot be
/// read is skipped, not an error.
pub fn list_files(root: &Path, limit: usize, budget: Duration) -> Listing {
    let start = Instant::now();
    let walk = WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .parents(false)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|e| {
            e.depth() == 0
                || match e.file_type() {
                    Some(t) if t.is_dir() => e.file_name().to_str().is_some_and(wanted_dir),
                    _ => e
                        .file_name()
                        .to_str()
                        .is_some_and(|n| !SKIP_FILES.contains(&n)),
                }
        })
        .build();
    let mut files = Vec::new();
    for entry in walk.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if files.len() >= limit || start.elapsed() >= budget {
            return Listing {
                files,
                truncated: true,
            };
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Listing {
        files,
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const LONG: Duration = Duration::from_secs(30);

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "x").unwrap();
    }

    // Issue #499: Quick Open listed nothing in a plain folder (it only knew `git ls-files`).
    #[test]
    fn lists_the_files_of_a_folder_with_no_git_sorted_and_relative() {
        let t = tempfile::tempdir().unwrap();
        for f in [
            "b.md",
            "a.txt",
            "docs/guide.md",
            "docs/img/logo.png",
            ".env.example",
        ] {
            touch(t.path(), f);
        }
        let l = list_files(t.path(), 100, LONG);
        assert!(!l.truncated);
        let mut sorted = l.files.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            [
                ".env.example",
                "a.txt",
                "b.md",
                "docs/guide.md",
                "docs/img/logo.png"
            ]
        );
        // Empty folders are not files.
        fs::create_dir_all(t.path().join("empty")).unwrap();
        assert_eq!(list_files(t.path(), 100, LONG).files.len(), 5);
    }

    #[test]
    fn skips_dependencies_build_output_caches_and_dot_folders() {
        let t = tempfile::tempdir().unwrap();
        for f in [
            "keep.md",
            "src/main.rs",
            ".github/workflows/ci.yml",
            ".vscode/settings.json",
            "node_modules/pkg/index.js",
            "target/debug/app",
            "build/out.o",
            "dist/a.js",
            "sub/node_modules/x.js",
            "__pycache__/a.pyc",
            ".git/config",
            ".cache/blob",
            ".next/server.js",
            ".venv/bin/python",
            ".DS_Store",
            "sub/.DS_Store",
        ] {
            touch(t.path(), f);
        }
        let mut got = list_files(t.path(), 100, LONG).files;
        got.sort();
        assert_eq!(
            got,
            [
                ".github/workflows/ci.yml",
                ".vscode/settings.json",
                "keep.md",
                "src/main.rs"
            ]
        );
    }

    // `.gitignore` applies without a `.git`, like in the task cache's walk.
    #[test]
    fn honours_a_gitignore_without_a_git_folder() {
        let t = tempfile::tempdir().unwrap();
        touch(t.path(), ".gitignore");
        fs::write(t.path().join(".gitignore"), "*.log\nsecret/\n").unwrap();
        for f in [
            "a.txt",
            "run.log",
            "secret/key.pem",
            "sub/b.log",
            "sub/c.txt",
        ] {
            touch(t.path(), f);
        }
        let mut got = list_files(t.path(), 100, LONG).files;
        got.sort();
        assert_eq!(got, [".gitignore", "a.txt", "sub/c.txt"]);
    }

    #[test]
    fn stops_at_the_file_limit_and_says_so() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..10 {
            touch(t.path(), &format!("f{i}.txt"));
        }
        let l = list_files(t.path(), 3, LONG);
        assert_eq!(l.files.len(), 3);
        assert!(l.truncated);
        // Exactly the limit is not truncated: there was nothing more.
        let full = list_files(t.path(), 10, LONG);
        assert_eq!((full.files.len(), full.truncated), (10, false));
    }

    #[test]
    fn stops_when_the_time_budget_is_spent() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..50 {
            touch(t.path(), &format!("f{i}.txt"));
        }
        let l = list_files(t.path(), 1000, Duration::ZERO);
        assert!(l.truncated);
        assert!(
            l.files.is_empty(),
            "a spent budget lists nothing: {:?}",
            l.files
        );
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks_out_of_the_folder() {
        let t = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        touch(outside.path(), "secret.txt");
        touch(t.path(), "a.txt");
        std::os::unix::fs::symlink(outside.path(), t.path().join("link")).unwrap();
        assert_eq!(list_files(t.path(), 100, LONG).files, ["a.txt"]);
    }

    #[test]
    fn a_missing_folder_lists_nothing() {
        let t = tempfile::tempdir().unwrap();
        let l = list_files(&t.path().join("gone"), 10, LONG);
        assert_eq!((l.files.len(), l.truncated), (0, false));
    }
}
