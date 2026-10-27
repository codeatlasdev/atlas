//! Which folder is "the project" for a path: what `atlas <path>` and `atlas -- <cmd>` open.

use std::path::{Path, PathBuf};

/// Files that make a folder a project when no `atlas.yaml` and no `.git` is above it.
const PACKAGE_MARKERS: [&str; 4] = ["package.json", "Cargo.toml", "pyproject.toml", "go.mod"];

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
}
