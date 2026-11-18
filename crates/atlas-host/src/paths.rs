//! Files of one hostd instance. Base dir is `~/.atlas`, overridable with
//! `ATLAS_HOSTD_DIR` (tests, remote hosts with `--dir`).

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
/// `<base>/desktop/app.log`: panics, host incidents and page errors of Atlas Desktop.
pub fn app_log(dir: &Path) -> PathBuf {
    dir.join("desktop").join("app.log")
}
/// `<base>/desktop/stderr.log`: what GTK, WebKit and the web processes printed (#273).
pub fn stderr_log(dir: &Path) -> PathBuf {
    dir.join("desktop").join("stderr.log")
}

/// The daemon log, opened for appending (created if missing): its stdout/stderr.
pub fn open_log(dir: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log(dir))
}

/// Where the output of the `atlas update` this daemon started goes (a file, not a pipe: the update
/// ends this daemon in its last step, and a pipe whose reader is gone makes the update's own
/// prints fail).
pub fn update_log(dir: &Path) -> PathBuf {
    dir.join("update.log")
}
pub fn scrollback_dir(dir: &Path) -> PathBuf {
    dir.join("hostd").join("scrollback")
}

/// The directory whose `hygiene/` registry covers the terminals of this scrollback dir: the hostd
/// dir for the real layout (`<dir>/hostd/scrollback`, the registry `atlas doctor --hygiene`
/// reads), the scrollback dir itself for any other (a test's temp dir stays self-contained).
pub fn hygiene_base(scroll_dir: &Path) -> PathBuf {
    let mut up = scroll_dir.ancestors();
    up.next();
    match (scroll_dir.file_name(), up.next(), up.next()) {
        (Some(a), Some(mid), Some(base))
            if a == "scrollback" && mid.file_name().is_some_and(|m| m == "hostd") =>
        {
            base.to_path_buf()
        }
        _ => scroll_dir.to_path_buf(),
    }
}

/// Where the agent hook server publishes `ATLAS_HOOK_PORT`/`ATLAS_HOOK_TOKEN`.
pub fn hook_endpoint(dir: &Path) -> PathBuf {
    dir.join("desktop").join("hook-endpoint.env")
}

/// Read the token, creating a random one (0600) when absent.
pub fn ensure_token(dir: &Path) -> std::io::Result<String> {
    if let Ok(t) = read_token(dir) {
        return Ok(t);
    }
    let t = random_hex(32)?;
    // Atomic: a client reading while hostd starts never sees half a token (`unauthorized`).
    write_private(&token(dir), t.as_bytes())?;
    Ok(t)
}

/// Replace `p` with `data`, readable by this user only, atomically (`atlas_core::fs::write_atomic`).
pub fn write_private(p: &Path, data: &[u8]) -> std::io::Result<()> {
    atlas_core::fs::write_atomic(p, data, atlas_core::fs::Mode::OwnerOnly)
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

/// What reading a state file found.
pub enum Loaded<T> {
    /// The file does not exist (first run): not an incident.
    Missing,
    Ok(T),
    /// The file exists but is not valid for `T`; it was renamed to `.0` and the original path is free.
    Corrupt(PathBuf),
}

/// Move `p` to `<name>.corrupt-<unix_ms>` beside it and return the new path. The bytes stay recoverable by
/// hand; the original path is free for a clean file.
pub fn set_aside_corrupt(p: &Path) -> std::io::Result<PathBuf> {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut aside = p.as_os_str().to_os_string();
    aside.push(format!(".corrupt-{ms}"));
    let aside = PathBuf::from(aside);
    std::fs::rename(p, &aside)?;
    Ok(aside)
}

/// Read JSON state at `p`. A missing file is [`Loaded::Missing`]; one that cannot be parsed is moved aside
/// ([`set_aside_corrupt`]), logged, and reported as [`Loaded::Corrupt`], never read as defaults. An I/O
/// error other than NotFound (permissions, a directory in the way) is an error and the file is left alone.
pub fn read_json_or_aside<T: serde::de::DeserializeOwned>(p: &Path) -> std::io::Result<Loaded<T>> {
    let bytes = match std::fs::read(p) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(e) => return Err(e),
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => Ok(Loaded::Ok(v)),
        Err(e) => {
            let aside = set_aside_corrupt(p)?;
            // i18n-ignore: log
            eprintln!(
                "atlas crew: {} is unreadable ({e}); kept as {}",
                p.display(),
                aside.display()
            );
            Ok(Loaded::Corrupt(aside))
        }
    }
}
