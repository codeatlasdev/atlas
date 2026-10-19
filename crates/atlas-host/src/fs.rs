//! Filesystem commands. Writes are atomic (tmp + rename in the same dir).

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

const READ_DEFAULT: u64 = 4 * 1024 * 1024;

fn err(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "not_found".into()
    } else {
        e.to_string()
    }
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

pub fn list(path: &str) -> Result<Value, String> {
    let mut entries: Vec<(bool, String, Value)> = std::fs::read_dir(path)
        .map_err(err)?
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
pub fn read(path: &str, max: Option<u64>, offset: Option<u64>) -> Result<Value, String> {
    use std::io::{Read, Seek, SeekFrom};
    let max = max.unwrap_or(READ_DEFAULT);
    let offset = offset.unwrap_or(0);
    let mut f = std::fs::File::open(path).map_err(err)?;
    let len = f.metadata().map_err(err)?.len();
    f.seek(SeekFrom::Start(offset)).map_err(err)?;
    let mut data = Vec::new();
    f.take(max).read_to_end(&mut data).map_err(err)?;
    let end = offset.saturating_add(data.len() as u64);
    Ok(json!({"data": B64.encode(&data), "truncated": len > end, "offset": offset}))
}

pub fn write(path: &str, data: &str, mode: Option<u32>) -> Result<Value, String> {
    let bytes = B64.decode(data).map_err(|e| e.to_string())?;
    let p = Path::new(path);
    let dir = p.parent().ok_or("no parent dir")?;
    let tmp = dir.join(format!(
        ".{}.atlas-tmp-{}",
        p.file_name().and_then(|n| n.to_str()).unwrap_or("f"),
        std::process::id()
    ));
    std::fs::write(&tmp, &bytes).map_err(err)?;
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
        err(e)
    })?;
    Ok(json!({}))
}

pub fn stat(path: &str) -> Result<Value, String> {
    let m = std::fs::symlink_metadata(path).map_err(err)?;
    Ok(json!({"kind": kind(m.file_type()), "size": m.len(), "mtime": mtime(&m)}))
}

pub fn mkdir(path: &str) -> Result<Value, String> {
    std::fs::create_dir_all(path).map_err(err)?;
    Ok(json!({}))
}

pub fn rename(from: &str, to: &str) -> Result<Value, String> {
    std::fs::rename(from, to).map_err(err)?;
    Ok(json!({}))
}

pub fn remove(path: &str, recursive: bool) -> Result<Value, String> {
    let m = std::fs::symlink_metadata(path).map_err(err)?;
    if m.is_dir() {
        if recursive {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_dir(path)
        }
    } else {
        std::fs::remove_file(path)
    }
    .map_err(err)?;
    Ok(json!({}))
}

pub const WATCH_DEBOUNCE: Duration = Duration::from_millis(100);

#[cfg(test)]
mod tests {
    use super::natural_cmp;

    #[test]
    fn natural_order() {
        let mut v = vec!["file10", "File2", "file1", "a"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["a", "file1", "File2", "file10"]);
    }
}
