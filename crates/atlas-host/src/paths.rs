//! Files of one hostd instance. Base dir is `~/.atlas`, overridable with
//! `ATLAS_HOSTD_DIR` (tests, remote hosts with `--dir`).

use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const DIR_ENV: &str = "ATLAS_HOSTD_DIR";

pub fn base() -> PathBuf {
    std::env::var_os(DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".atlas"))
}

pub fn socket(dir: &Path) -> PathBuf {
    dir.join("hostd.sock")
}
pub fn token(dir: &Path) -> PathBuf {
    dir.join("hostd.token")
}
pub fn pid(dir: &Path) -> PathBuf {
    dir.join("hostd.pid")
}
pub fn log(dir: &Path) -> PathBuf {
    dir.join("hostd.log")
}

/// The daemon log, opened for appending (created if missing): its stdout/stderr.
pub fn open_log(dir: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log(dir))
}
pub fn scrollback_dir(dir: &Path) -> PathBuf {
    dir.join("hostd").join("scrollback")
}

/// Read the token, creating a random one (0600) when absent.
pub fn ensure_token(dir: &Path) -> std::io::Result<String> {
    if let Ok(t) = read_token(dir) {
        return Ok(t);
    }
    std::fs::create_dir_all(dir)?;
    let t = random_hex(32)?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    // Windows: the file inherits the per-user ACL of `~/.atlas` (the profile is private).
    #[cfg(unix)]
    opts.mode(0o600);
    let mut f = opts.open(token(dir))?;
    f.write_all(t.as_bytes())?;
    Ok(t)
}

pub fn read_token(dir: &Path) -> std::io::Result<String> {
    let t = std::fs::read_to_string(token(dir))?.trim().to_string();
    if t.is_empty() {
        return Err(std::io::Error::other("empty token"));
    }
    Ok(t)
}

pub fn random_hex(bytes: usize) -> std::io::Result<String> {
    let mut buf = vec![0u8; bytes];
    atlas_core::sys::random_bytes(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Random UUIDv4-shaped id (no `uuid` dependency).
pub fn new_id() -> String {
    let h = random_hex(16).unwrap_or_else(|_| {
        format!(
            "{:032x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}
