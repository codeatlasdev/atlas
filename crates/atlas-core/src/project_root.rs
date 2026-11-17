//! Which folder is "the project" for a path: what `atlas <path>` and `atlas -- <cmd>` open.

use std::path::{Path, PathBuf};

/// Files that make a folder a project when no `atlas.yaml` and no `.git` is above it.
const PACKAGE_MARKERS: [&str; 4] = ["package.json", "Cargo.toml", "pyproject.toml", "go.mod"];

/// More manifests that mark a folder as a project (build files of other stacks, compose files). They do not move the
/// root `resolve` picks for a path; they only keep [`is_project_root`] from calling such a folder a plain folder.
const OTHER_MARKERS: [&str; 14] = [
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
    "Gemfile",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "deno.json",
    "pubspec.yaml",
    "Package.swift",
    "mix.exs",
    "CMakeLists.txt",
];

/// Is `dir` a project (a git checkout, an `atlas.yaml`, or a recognised manifest) rather than a plain folder such as
/// `~/Downloads`? Atlas Desktop opens a plain folder as an explorer and an editor only (no services, no `atlas.yaml`
/// prompt, no Changes or PR); the user can still promote it. Pure: reads the file system, never writes.
pub fn is_project_root(dir: &Path) -> bool {
    dir.join("atlas.yaml").is_file()
        || dir.join(".git").exists()
        || PACKAGE_MARKERS
            .iter()
            .chain(OTHER_MARKERS.iter())
            .any(|m| dir.join(m).is_file())
}

/// The project root of `start` (a folder or a file): the nearest ancestor with an `atlas.yaml`;
/// else the nearest one with `.git` or a package marker; else `start` itself (its parent when
/// `start` is a file). Pure: reads the file system, never writes.
pub fn resolve(start: &Path) -> PathBuf {
    let dir = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let nearest = |is_root: &dyn Fn(&Path) -> bool| dir.ancestors().find(|d| is_root(d));
    nearest(&|d| d.join("atlas.yaml").is_file())
        .or_else(|| {
            nearest(&|d| {
                d.join(".git").exists() || PACKAGE_MARKERS.iter().any(|m| d.join(m).is_file())
            })
        })
        .unwrap_or(dir)
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "").unwrap();
    }

    #[test]
    fn atlas_yaml_wins_over_a_nearer_package_marker() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        touch(&r.join("atlas.yaml"));
        touch(&r.join("apps/web/package.json"));
        assert_eq!(resolve(&r.join("apps/web")), r);
    }

    #[test]
    fn folder_with_atlas_yaml_resolves_to_itself() {
        let t = tempfile::tempdir().unwrap();
        touch(&t.path().join("atlas.yaml"));
        assert_eq!(resolve(t.path()), t.path());
    }

    #[test]
    fn node_folder_without_atlas_yaml_resolves_to_itself() {
        let t = tempfile::tempdir().unwrap();
        touch(&t.path().join("package.json"));
        assert_eq!(resolve(t.path()), t.path());
    }

    #[test]
    fn nested_file_resolves_to_the_nearest_marker_folder() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        touch(&r.join("pkg/Cargo.toml"));
        touch(&r.join("pkg/src/deep/lib.rs"));
        assert_eq!(resolve(&r.join("pkg/src/deep/lib.rs")), r.join("pkg"));
        assert_eq!(resolve(&r.join("pkg/src/deep")), r.join("pkg"));
    }

    #[test]
    fn git_folder_marks_a_root() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        fs::create_dir_all(r.join(".git")).unwrap();
        touch(&r.join("notes/a.md"));
        assert_eq!(resolve(&r.join("notes/a.md")), r);
    }

    #[test]
    fn a_folder_with_no_marker_anywhere_resolves_to_itself_and_a_file_to_its_parent() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        touch(&r.join("loose/a.txt"));
        // The tempdir may sit under a marker on the host (a `.git` above /tmp is rare but legal):
        // only assert when none of the ancestors has one.
        if r.ancestors().any(|d| {
            d.join(".git").exists()
                || d.join("atlas.yaml").exists()
                || PACKAGE_MARKERS.iter().any(|m| d.join(m).exists())
        }) {
            return;
        }
        assert_eq!(resolve(&r.join("loose")), r.join("loose"));
        assert_eq!(resolve(&r.join("loose/a.txt")), r.join("loose"));
    }

    // Issue #499: opening `~/Downloads` treated it as a project (the header asked for an atlas.yaml). A folder is a
    // project only with a marker; documents, images and archives do not make one.
    #[test]
    fn a_folder_of_loose_files_is_not_a_project() {
        let t = tempfile::tempdir().unwrap();
        touch(&t.path().join("Downloads/report.pdf"));
        touch(&t.path().join("Downloads/notes.md"));
        touch(&t.path().join("Downloads/photos/a.png"));
        assert!(!is_project_root(&t.path().join("Downloads")));
        assert!(!is_project_root(&t.path().join("Downloads/photos")));
        // A folder that does not exist is not one either.
        assert!(!is_project_root(&t.path().join("gone")));
    }

    #[test]
    fn every_marker_makes_a_folder_a_project() {
        let markers = ["atlas.yaml"]
            .into_iter()
            .chain(PACKAGE_MARKERS)
            .chain(OTHER_MARKERS);
        for m in markers {
            let t = tempfile::tempdir().unwrap();
            touch(&t.path().join(m));
            assert!(is_project_root(t.path()), "{m} should mark a project");
        }
        // `.git` is a directory in a checkout and a file in a linked worktree.
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".git")).unwrap();
        assert!(is_project_root(repo.path()));
        let linked = tempfile::tempdir().unwrap();
        touch(&linked.path().join(".git"));
        assert!(is_project_root(linked.path()));
    }

    // A marker must be a file: a folder that happens to be named `package.json` (or a `Cargo.toml/` directory some
    // tool made) is not a manifest.
    #[test]
    fn a_directory_named_like_a_manifest_does_not_make_a_project() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("package.json")).unwrap();
        fs::create_dir_all(t.path().join("atlas.yaml")).unwrap();
        assert!(!is_project_root(t.path()));
    }

    // The markers are looked for in the folder itself, never above it: a loose folder inside a project is a plain
    // folder when opened on its own (the project root is `resolve`'s job, not this function's).
    #[test]
    fn only_the_folder_itself_is_inspected_not_its_ancestors() {
        let t = tempfile::tempdir().unwrap();
        touch(&t.path().join("Cargo.toml"));
        touch(&t.path().join("docs/guide.md"));
        assert!(is_project_root(t.path()));
        assert!(!is_project_root(&t.path().join("docs")));
    }
}
