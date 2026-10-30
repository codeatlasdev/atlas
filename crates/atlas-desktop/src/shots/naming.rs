//! File names of captures: what the folder looks like after a week of use. Pure.

use std::path::{Path, PathBuf};

/// What was captured, for the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Screenshot,
    Recording,
    Gif,
}

impl Kind {
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Screenshot => "png",
            Kind::Recording => "mp4",
            Kind::Gif => "gif",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Screenshot => "Screenshot",
            Kind::Recording => "Screen Recording",
            Kind::Gif => "Screen Recording",
        }
    }
}

/// `Screenshot 2026-10-26 at 14.03.22.png`: the macOS convention (no colons: Finder shows `:` as `/`), sorted
/// by name = sorted by time. `stamp` is local time as (year, month, day, hour, minute, second).
pub fn file_name(kind: Kind, stamp: (i32, u32, u32, u32, u32, u32)) -> String {
    let (y, mo, d, h, mi, s) = stamp;
    format!(
        "{} {y:04}-{mo:02}-{d:02} at {h:02}.{mi:02}.{s:02}.{}",
        kind.label(),
        kind.extension()
    )
}

/// `name` in `dir`, or `name 2`, `name 3` … when it exists (two captures in one second must not overwrite each
/// other). `exists` is a parameter so the rule is testable without a disk.
pub fn unique(dir: &Path, name: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(name);
    if !exists(&first) {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|p| !exists(p))
        .expect("an unbounded range finds a free name")
}

/// The file the editor writes next to the original: `Screenshot … 2.png` becomes `Screenshot … 2 (edited).png`.
pub fn edited(original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Screenshot");
    let ext = original
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png");
    let stem = stem.strip_suffix(" (edited)").unwrap_or(stem);
    original.with_file_name(format!("{stem} (edited).{ext}"))
}

/// Give a finished capture the permissions of an ordinary file (`0644`). ScreenCaptureKit writes recordings as
/// `0600`, which Finder shows as a file only you may open, and which mail and chat apps then refuse to attach.
#[cfg(unix)]
pub fn make_shareable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
}

#[cfg(not(unix))]
pub fn make_shareable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[cfg(unix)]
    #[test]
    fn a_private_recording_becomes_an_ordinary_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("Screen Recording.mp4");
        std::fs::write(&f, b"x").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        make_shareable(&f).unwrap();
        assert_eq!(
            std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert!(
            make_shareable(&dir.path().join("missing")).is_err(),
            "a missing file is an error, not a silent pass"
        );
    }

    #[test]
    fn names_follow_the_macos_convention_without_colons() {
        let t = (2026, 10, 26, 14, 3, 22);
        assert_eq!(
            file_name(Kind::Screenshot, t),
            "Screenshot 2026-10-26 at 14.03.22.png"
        );
        assert_eq!(
            file_name(Kind::Recording, t),
            "Screen Recording 2026-10-26 at 14.03.22.mp4"
        );
        assert_eq!(
            file_name(Kind::Gif, t),
            "Screen Recording 2026-10-26 at 14.03.22.gif"
        );
        for k in [Kind::Screenshot, Kind::Recording, Kind::Gif] {
            let n = file_name(k, t);
            assert!(!n.contains(':') && !n.contains('/'), "{n}");
        }
    }

    #[test]
    fn single_digits_are_zero_padded_so_names_sort_by_time() {
        let a = file_name(Kind::Screenshot, (2026, 1, 2, 3, 4, 5));
        let b = file_name(Kind::Screenshot, (2026, 1, 2, 3, 4, 15));
        assert_eq!(a, "Screenshot 2026-01-02 at 03.04.05.png");
        assert!(a < b);
    }

    /// Two captures in one second must not overwrite each other (the second one is the one the user is looking at).
    #[test]
    fn a_name_taken_in_the_same_second_gets_a_counter() {
        let dir = Path::new("/shots");
        let taken: HashSet<PathBuf> = [
            dir.join("Screenshot 2026-10-26 at 14.03.22.png"),
            dir.join("Screenshot 2026-10-26 at 14.03.22 2.png"),
        ]
        .into();
        let p = unique(dir, "Screenshot 2026-10-26 at 14.03.22.png", |p| {
            taken.contains(p)
        });
        assert_eq!(p, dir.join("Screenshot 2026-10-26 at 14.03.22 3.png"));
        let free = unique(dir, "other.png", |p| taken.contains(p));
        assert_eq!(free, dir.join("other.png"));
    }

    #[test]
    fn a_name_without_an_extension_still_gets_a_counter() {
        let dir = Path::new("/x");
        let p = unique(dir, "noext", |p| p == dir.join("noext"));
        assert_eq!(p, dir.join("noext 2"));
    }

    #[test]
    fn the_edited_copy_sits_next_to_the_original_and_is_not_nested() {
        let p = Path::new("/shots/Screenshot 2026-10-26 at 14.03.22.png");
        let e = edited(p);
        assert_eq!(
            e,
            Path::new("/shots/Screenshot 2026-10-26 at 14.03.22 (edited).png")
        );
        assert_eq!(edited(&e), e, "editing an edited copy overwrites it");
    }
}
