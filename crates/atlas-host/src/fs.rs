//! Filesystem commands. Writes are atomic (tmp + rename in the same dir).

use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use atlas_core::progress::Coded;
use serde_json::{Value, json};

const READ_DEFAULT: u64 = 4 * 1024 * 1024;

/// The failure of an operation on `path`, as `fs.not_found {path}` or `fs.io {kind, path}` (see
/// `protocol::REPLY_CODES`). The text is what it has always been: `not_found` for a missing path (the CLI and old
/// desktops compare it), the OS's own words otherwise; `detail` carries those words for a kind with no sentence of
/// its own, so a client can show them apart from its own.
pub(crate) fn coded(e: &std::io::Error, path: &str) -> Coded {
    use crate::protocol::{FS_IO, FS_NOT_FOUND};
    let kind = match e.kind() {
        ErrorKind::NotFound => {
            return Coded::new(FS_NOT_FOUND, json!({"path": path}), "not_found");
        }
        ErrorKind::PermissionDenied => "permission_denied",
        ErrorKind::NotADirectory => "not_a_directory",
        ErrorKind::IsADirectory => "is_a_directory",
        ErrorKind::AlreadyExists => "already_exists",
        _ => {
            let args = json!({"kind": "other", "path": path, "detail": e.to_string()});
            return Coded::new(FS_IO, args, e.to_string());
        }
    };
    Coded::new(FS_IO, json!({"kind": kind, "path": path}), e.to_string())
}

fn err(path: &str) -> impl Fn(std::io::Error) -> Coded + '_ {
    move |e| coded(&e, path)
}

fn kind(ft: std::fs::FileType) -> &'static str {
    if ft.is_symlink() {
        "symlink"
    } else if ft.is_dir() {
        "dir"
    } else {
        "file"
    }
}

fn mtime(m: &std::fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Natural order: digit runs compare numerically, case-insensitive otherwise.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o.is_ne() {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o.is_ne() {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

pub fn list(path: &str) -> Result<Value, Coded> {
    let mut entries: Vec<(bool, String, Value)> = std::fs::read_dir(path)
        .map_err(err(path))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let ft = e.file_type().ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = ft.is_dir() || (ft.is_symlink() && e.path().is_dir());
            let m = e.metadata().ok();
            let v = json!({
                "name": name,
                "kind": kind(ft),
                "size": m.as_ref().map(|m| m.len()).unwrap_or(0),
                "mtime": m.as_ref().map(mtime).unwrap_or(0),
            });
            Some((is_dir, name, v))
        })
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| natural_cmp(&a.1, &b.1)));
    Ok(json!({"entries": entries.into_iter().map(|e| e.2).collect::<Vec<_>>()}))
}

/// Reads up to `max` bytes from `offset`. `truncated` is true when bytes remain after the chunk.
pub fn read(path: &str, max: Option<u64>, offset: Option<u64>) -> Result<Value, Coded> {
    use std::io::{Read, Seek, SeekFrom};
    let max = max.unwrap_or(READ_DEFAULT);
    let offset = offset.unwrap_or(0);
    let mut f = std::fs::File::open(path).map_err(err(path))?;
    let len = f.metadata().map_err(err(path))?.len();
    f.seek(SeekFrom::Start(offset)).map_err(err(path))?;
    let mut data = Vec::new();
    f.take(max).read_to_end(&mut data).map_err(err(path))?;
    let end = offset.saturating_add(data.len() as u64);
    Ok(json!({"data": B64.encode(&data), "truncated": len > end, "offset": offset}))
}

pub fn write(path: &str, data: &str, mode: Option<u32>) -> Result<Value, Coded> {
    let bytes = B64
        .decode(data)
        .map_err(|e| crate::protocol::invalid_argument("data", "bad_base64", e.to_string()))?;
    let p = Path::new(path);
    let dir = p
        .parent()
        .ok_or_else(|| crate::protocol::invalid_argument("path", "no_parent", "no parent dir"))?;
    let tmp = dir.join(format!(
        ".{}.atlas-tmp-{}",
        p.file_name().and_then(|n| n.to_str()).unwrap_or("f"),
        std::process::id()
    ));
    // The temporary file lives in the target's folder: when it cannot be made, the folder is the problem.
    std::fs::write(&tmp, &bytes).map_err(err(&dir.to_string_lossy()))?;
    // Unix modes only: Windows has no mode bits (the file inherits its folder's ACL).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perm = mode.or_else(|| std::fs::metadata(p).ok().map(|m| m.permissions().mode()));
        if let Some(m) = perm {
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(m));
        }
    }
    #[cfg(not(unix))]
    let _ = mode;
    std::fs::rename(&tmp, p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        coded(&e, path)
    })?;
    Ok(json!({}))
}

pub fn stat(path: &str) -> Result<Value, Coded> {
    let m = std::fs::symlink_metadata(path).map_err(err(path))?;
    Ok(json!({"kind": kind(m.file_type()), "size": m.len(), "mtime": mtime(&m)}))
}

pub fn mkdir(path: &str) -> Result<Value, Coded> {
    std::fs::create_dir_all(path).map_err(err(path))?;
    Ok(json!({}))
}

pub fn rename(from: &str, to: &str) -> Result<Value, Coded> {
    std::fs::rename(from, to).map_err(|e| {
        // Which of the two the failure is about: a missing source is `from`; else the destination (a missing
        // folder to move into, a name taken, no permission there).
        let about = if std::fs::symlink_metadata(from).is_err() {
            from
        } else if e.kind() == ErrorKind::NotFound {
            Path::new(to).parent().and_then(Path::to_str).unwrap_or(to)
        } else {
            to
        };
        coded(&e, about)
    })?;
    Ok(json!({}))
}

pub fn remove(path: &str, recursive: bool) -> Result<Value, Coded> {
    let m = std::fs::symlink_metadata(path).map_err(err(path))?;
    if m.is_dir() {
        if recursive {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_dir(path)
        }
    } else {
        std::fs::remove_file(path)
    }
    .map_err(err(path))?;
    Ok(json!({}))
}

pub const WATCH_DEBOUNCE: Duration = Duration::from_millis(100);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["file10", "File2", "file1", "a"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["a", "file1", "File2", "file10"]);
    }

    fn code_args(c: &Coded) -> (&'static str, Value) {
        (c.code, Value::Object(c.args.clone()))
    }

    /// Incident: a missing file toasted the literal `not_found`. The code names the path; the text stays `not_found`.
    #[test]
    fn removing_a_missing_file_is_fs_not_found_with_the_old_text() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("gone.txt");
        let p = p.to_str().unwrap();
        let e = remove(p, false).unwrap_err();
        assert_eq!(code_args(&e), ("fs.not_found", json!({"path": p})));
        assert_eq!(e.to_string(), "not_found");
    }

    /// A folder the user may not write to: the code says which, whatever language the OS speaks.
    #[cfg(unix)]
    #[test]
    fn a_folder_without_write_permission_is_fs_io_permission_denied() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("locked");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        // root ignores the mode bits: nothing to prove there.
        if std::fs::write(dir.join("probe"), "").is_ok() {
            return;
        }
        let file = dir.join("new.txt");
        let e = write(file.to_str().unwrap(), "", None).unwrap_err();
        assert_eq!(
            code_args(&e),
            ("fs.io", json!({"kind": "permission_denied", "path": dir.to_str().unwrap()}))
        );
        assert!(e.message.contains("Permission denied"), "{}", e.message);
        let e = mkdir(dir.join("sub").to_str().unwrap()).unwrap_err();
        assert_eq!(e.args["kind"], "permission_denied");
        let e = remove(file.to_str().unwrap(), false).unwrap_err();
        assert_eq!(e.code, "fs.not_found");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    /// A failure with no kind of its own carries the OS's words as `detail`, so the app can show them apart from
    /// its own sentence; a kind it has a sentence for needs none.
    #[test]
    fn only_other_carries_the_os_text_as_detail() {
        let other = coded(&std::io::Error::other("Read-only file system (os error 30)"), "/w/x");
        assert_eq!(
            code_args(&other),
            ("fs.io", json!({"kind": "other", "path": "/w/x", "detail": "Read-only file system (os error 30)"}))
        );
        let known = coded(&std::io::Error::new(ErrorKind::AlreadyExists, "File exists (os error 17)"), "/w/x");
        assert_eq!(code_args(&known), ("fs.io", json!({"kind": "already_exists", "path": "/w/x"})));
    }

    /// Renaming into a folder that does not exist is about that folder; a missing source is about the source.
    #[test]
    fn a_rename_names_the_path_that_is_missing() {
        let d = tempfile::tempdir().unwrap();
        let from = d.path().join("a.txt");
        let to = d.path().join("no-such-dir").join("b.txt");
        let (from_s, to_s) = (from.to_str().unwrap(), to.to_str().unwrap());
        assert_eq!(rename(from_s, to_s).unwrap_err().args["path"], from_s);
        std::fs::write(&from, "x").unwrap();
        assert_eq!(
            rename(from_s, to_s).unwrap_err().args["path"],
            d.path().join("no-such-dir").to_str().unwrap()
        );
    }
}
