//! Theme packages: unpack a user-supplied `.zip` / `.vsix` (or list a folder) so the
//! frontend can scan it for themes. The archive is the user's own file; we extract it
//! into a throw-away temp dir with the system `ditto`, after vetting the entry list, and
//! hand back the file list. The frontend reads single files with `theme_read_packed`
//! and always calls `theme_unpack_cleanup` when done.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use std::collections::HashSet;
use std::sync::LazyLock;

use parking_lot::Mutex;

/// Canonical roots issued by `theme_unpack` in this session. `theme_read_packed` serves
/// nothing else, so the webview can't use it to read arbitrary directories.
static ROOTS: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Default::default);

fn register(root: &Path) {
    if let Ok(real) = fs::canonicalize(root) {
        ROOTS.lock().insert(real);
    }
}

fn unregister(root: &Path) {
    let real = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    ROOTS.lock().remove(&real);
}

/// Prefix of every temp dir we create; cleanup refuses anything else.
const TEMP_PREFIX: &str = "atlas-theme-pack-";
const MAX_ENTRIES: usize = 5000;
const MAX_FILES: usize = 5000;
const MAX_BYTES: u64 = 200 * 1024 * 1024;
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const TOOL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Serialize, Debug)]
pub struct PackedFile {
    /// `/`-separated path relative to `UnpackedPackage::dir`.
    pub rel: String,
    pub size: u64,
}

#[derive(Serialize, Debug)]
pub struct UnpackedPackage {
    pub dir: String,
    /// True when `dir` is a temp dir we created (the caller must clean it up); false
    /// for a folder import, which is read in place and never deleted.
    pub temporary: bool,
    pub files: Vec<PackedFile>,
}

/// Run `cmd` to completion, killing it after `TOOL_TIMEOUT`. Returns its stdout.
fn run_tool(mut cmd: Command, what: &str) -> Result<Vec<u8>, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not run {what}: {e}"))?;
    // Drain stdout on a thread so a chatty tool can't block on a full pipe while we poll.
    let mut out = child.stdout.take().expect("stdout is piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(st) => break st,
            None if start.elapsed() > TOOL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{what} took longer than {}s: the archive is too big or damaged",
                    TOOL_TIMEOUT.as_secs()
                ));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let buf = reader.join().unwrap_or_default();
    if status.success() {
        Ok(buf)
    } else {
        Err(format!(
            "{what} failed: this does not look like a valid zip archive"
        ))
    }
}

/// An entry name that would land outside the extraction dir.
fn unsafe_entry(name: &str) -> bool {
    name.starts_with('/')
        || name.starts_with('\\')
        || name.split(['/', '\\']).any(|seg| seg == "..")
        || Path::new(name).components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

/// Windows' own archive tool (bsdtar, in System32 since Windows 10), which reads zip.
#[cfg(windows)]
fn bsdtar() -> Command {
    use atlas_core::sys::CommandExt as _;
    let exe = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32").join("tar.exe"))
        .unwrap_or_else(|| PathBuf::from("tar.exe"));
    let mut c = Command::new(exe);
    c.no_window();
    c
}

/// The archive's entry names, one per line.
fn list_entries(zip: &Path) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    let (cmd, what) = {
        let mut c = bsdtar();
        c.arg("-tf").arg(zip);
        (c, "tar")
    };
    #[cfg(unix)]
    let (cmd, what) = {
        let mut c = Command::new("/usr/bin/unzip");
        c.arg("-Z1").arg(zip);
        (c, "unzip")
    };
    run_tool(cmd, what)
}

/// Vet the archive's entry list and size before extracting anything. Windows' bsdtar reports no
/// uncompressed total up front: there the size cap is enforced by `walk` after extraction.
fn vet_archive(zip: &Path) -> Result<(), String> {
    let list = list_entries(zip)?;
    let list = String::from_utf8_lossy(&list);
    let mut count = 0usize;
    for name in list.lines().filter(|l| !l.is_empty()) {
        count += 1;
        if count > MAX_ENTRIES {
            return Err(format!(
                "This archive has more than {MAX_ENTRIES} entries: refusing to import it"
            ));
        }
        if unsafe_entry(name) {
            return Err(format!(
                "This archive has an entry outside its folder ({name}): refusing to import it"
            ));
        }
    }
    if cfg!(windows) {
        return Ok(());
    }
    let totals = run_tool(
        {
            let mut c = Command::new("/usr/bin/unzip");
            c.arg("-Zt").arg(zip);
            c
        },
        "unzip",
    )?;
    let totals = String::from_utf8_lossy(&totals);
    // "N files, B bytes uncompressed, C bytes compressed:  P%"
    let bytes = totals
        .split_whitespace()
        .skip_while(|w| !w.starts_with("file"))
        .nth(1)
        .and_then(|b| b.parse::<u64>().ok());
    if bytes.is_some_and(|b| b > MAX_BYTES) {
        return Err("This archive expands to more than 200 MB: refusing to import it".into());
    }
    Ok(())
}

/// Walk `root` without following symlinks: drop symlinks, enforce the caps, collect
/// regular files. Anything that is neither a dir nor a regular file is dropped.
fn walk(root: &Path, drop_symlinks: bool) -> Result<Vec<PackedFile>, String> {
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd =
            fs::read_dir(&dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;
        for ent in rd {
            let ent = ent.map_err(|e| e.to_string())?;
            let path = ent.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            let ty = meta.file_type();
            if ty.is_symlink() {
                if drop_symlinks {
                    let _ = fs::remove_file(&path);
                }
                continue;
            }
            if ty.is_dir() {
                // Finder noise: never offered to the scanner.
                if dir == root && ent.file_name() == "__MACOSX" {
                    continue;
                }
                stack.push(path);
            } else if ty.is_file() {
                total += meta.len();
                if files.len() >= MAX_FILES || total > MAX_BYTES {
                    return Err(
                        "This package is too big (over 5000 files or 200 MB): refusing to import it"
                            .into(),
                    );
                }
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.push(PackedFile {
                    rel,
                    size: meta.len(),
                });
            } else if drop_symlinks {
                let _ = fs::remove_file(&path);
            }
        }
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(files)
}

fn is_zip_like(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("vsix"))
}

fn unpack_blocking(path: &str) -> Result<UnpackedPackage, String> {
    let src = PathBuf::from(path);
    let meta = fs::metadata(&src).map_err(|e| format!("could not open {path}: {e}"))?;
    if meta.is_dir() {
        let root = fs::canonicalize(&src).map_err(|e| e.to_string())?;
        // Folder import: read-only listing, symlinks are skipped, nothing is deleted.
        let files = walk(&root, false)?;
        register(&root);
        return Ok(UnpackedPackage {
            dir: root.to_string_lossy().into_owned(),
            temporary: false,
            files,
        });
    }
    if !is_zip_like(&src) {
        return Err("Pick a .zip or .vsix archive, or a folder".into());
    }
    vet_archive(&src)?;
    let tmp = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .tempdir()
        .map_err(|e| format!("could not create a temp folder: {e}"))?;
    // Until extraction is vetted the TempDir guard removes the dir on every error path.
    let dir = tmp.path().to_path_buf();
    #[cfg(windows)]
    run_tool(
        {
            let mut c = bsdtar();
            c.arg("-xf").arg(&src).arg("-C").arg(&dir);
            c
        },
        "tar",
    )?;
    #[cfg(unix)]
    run_tool(
        {
            let mut c = Command::new("/usr/bin/ditto");
            c.arg("-x").arg("-k").arg(&src).arg(&dir);
            c
        },
        "ditto",
    )?;
    let root = fs::canonicalize(&dir).map_err(|e| e.to_string())?;
    let files = walk(&root, true)?;
    // Defence in depth: every collected file must resolve inside the temp dir.
    for f in &files {
        let real = fs::canonicalize(root.join(&f.rel)).map_err(|e| e.to_string())?;
        if !real.starts_with(&root) {
            return Err(format!(
                "This archive has an entry outside its folder ({}): refusing to import it",
                f.rel
            ));
        }
    }
    let kept = tmp.keep();
    register(&root);
    Ok(UnpackedPackage {
        dir: kept.to_string_lossy().into_owned(),
        temporary: true,
        files,
    })
}

#[tauri::command]
pub async fn theme_unpack(path: String) -> Result<UnpackedPackage, String> {
    tokio::task::spawn_blocking(move || unpack_blocking(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// `dir` is a temp dir we made: lives directly under the system temp dir and carries
/// our prefix. Checked on the canonical path so `..` and symlinks can't fake it.
fn is_ours(dir: &Path) -> bool {
    let Ok(real) = fs::canonicalize(dir) else {
        return false;
    };
    let Ok(tmp) = fs::canonicalize(std::env::temp_dir()) else {
        return false;
    };
    real.parent() == Some(tmp.as_path())
        && real
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(TEMP_PREFIX))
}

fn cleanup(dir: &str) -> Result<(), String> {
    let p = Path::new(dir);
    // Drop the root from the registry first (folders included), even if it is gone.
    if let Ok(real) = fs::canonicalize(p) {
        unregister(&real);
    }
    if !p.exists() {
        return Ok(());
    }
    if !is_ours(p) {
        // Folder imports (and anything else that isn't ours) are left untouched.
        return Ok(());
    }
    fs::remove_dir_all(p).map_err(|e| format!("could not remove {dir}: {e}"))
}

#[tauri::command]
pub fn theme_unpack_cleanup(dir: String) -> Result<(), String> {
    cleanup(&dir)
}

fn read_packed(dir: &str, rel: &str) -> Result<String, String> {
    if rel.is_empty() || unsafe_entry(rel) {
        return Err(format!("{rel}: path is outside the package"));
    }
    let root = fs::canonicalize(dir).map_err(|e| format!("could not open the package: {e}"))?;
    if !ROOTS.lock().contains(&root) {
        return Err(format!("{dir}: not a package opened by Atlas"));
    }
    let target = root.join(rel);
    let meta = fs::symlink_metadata(&target).map_err(|e| format!("{rel}: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{rel}: symlinks are not followed"));
    }
    if !meta.is_file() {
        return Err(format!("{rel}: not a regular file"));
    }
    // A symlinked parent directory would still resolve outside; check the real path too.
    let real = fs::canonicalize(&target).map_err(|e| format!("{rel}: {e}"))?;
    if !real.starts_with(&root) {
        return Err(format!("{rel}: path is outside the package"));
    }
    if meta.len() > MAX_READ_BYTES {
        return Err(format!("{rel}: file is larger than 2 MB"));
    }
    let bytes = fs::read(&real).map_err(|e| format!("{rel}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{rel}: not a UTF-8 text file"))
}

#[tauri::command]
pub async fn theme_read_packed(dir: String, rel: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || read_packed(&dir, &rel))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn have_zip() -> bool {
        Path::new("/usr/bin/zip").exists()
            && Path::new("/usr/bin/unzip").exists()
            && Path::new("/usr/bin/ditto").exists()
    }

    /// Build `out` from `(name, bytes)` entries. Names are written literally (`..`,
    /// absolute) by a tiny stored-zip writer, since the `zip` CLI normalises them.
    fn write_zip(out: &Path, entries: &[(&str, &[u8])]) {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = !0u32;
            for &b in data {
                crc ^= b as u32;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        let mut body = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let off = body.len() as u32;
            let crc = crc32(data);
            let n = name.as_bytes();
            body.extend(0x0403_4b50u32.to_le_bytes());
            body.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // ver, flags, stored, time, date
            body.extend(crc.to_le_bytes());
            body.extend((data.len() as u32).to_le_bytes());
            body.extend((data.len() as u32).to_le_bytes());
            body.extend((n.len() as u16).to_le_bytes());
            body.extend(0u16.to_le_bytes());
            body.extend(n);
            body.extend(*data);
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend(crc.to_le_bytes());
            central.extend((data.len() as u32).to_le_bytes());
            central.extend((data.len() as u32).to_le_bytes());
            central.extend((n.len() as u16).to_le_bytes());
            central.extend([0u8; 12]); // extra, comment, disk, int attrs (2 each), ext attrs (4)
            central.extend(off.to_le_bytes());
            central.extend(n);
        }
        let cd_off = body.len() as u32;
        let mut zip = body;
        zip.extend(&central);
        zip.extend(0x0605_4b50u32.to_le_bytes());
        zip.extend([0, 0, 0, 0]);
        zip.extend((entries.len() as u16).to_le_bytes());
        zip.extend((entries.len() as u16).to_le_bytes());
        zip.extend((central.len() as u32).to_le_bytes());
        zip.extend(cd_off.to_le_bytes());
        zip.extend(0u16.to_le_bytes());
        fs::write(out, zip).unwrap();
    }

    fn rels(p: &UnpackedPackage) -> Vec<&str> {
        p.files.iter().map(|f| f.rel.as_str()).collect()
    }

    #[test]
    fn lists_normal_archive_with_nested_folders() {
        if !have_zip() {
            eprintln!("skipped: zip tools missing");
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let z = w.path().join("pack.zip");
        write_zip(
            &z,
            &[
                ("themes/night-test.json", b"{\"colors\":{}}"),
                ("themes/deep/dawn-test.itermcolors", b"<plist/>"),
                ("package.json", b"{}"),
            ],
        );
        let p = unpack_blocking(z.to_str().unwrap()).unwrap();
        assert!(p.temporary);
        assert_eq!(
            rels(&p),
            [
                "package.json",
                "themes/deep/dawn-test.itermcolors",
                "themes/night-test.json"
            ]
        );
        assert_eq!(p.files[0].size, 2);
        assert_eq!(
            read_packed(&p.dir, "themes/night-test.json").unwrap(),
            "{\"colors\":{}}"
        );
        cleanup(&p.dir).unwrap();
        assert!(!Path::new(&p.dir).exists());
    }

    #[test]
    fn vsix_extension_is_a_zip() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let z = w.path().join("x.vsix");
        write_zip(&z, &[("extension/themes/a.json", b"{}")]);
        let p = unpack_blocking(z.to_str().unwrap()).unwrap();
        assert_eq!(rels(&p), ["extension/themes/a.json"]);
        cleanup(&p.dir).unwrap();
    }

    #[test]
    fn macosx_noise_is_ignored_not_rejected() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let src = w.path().join("pkg");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("a.json"), "{}").unwrap();
        let z = w.path().join("pack.zip");
        // Finder-style archive: ditto sequesters resource forks into `__MACOSX/`.
        let st = Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(&src)
            .arg(&z)
            .status()
            .unwrap();
        assert!(st.success());
        let p = unpack_blocking(z.to_str().unwrap()).unwrap();
        assert_eq!(rels(&p), ["pkg/a.json"]);
        cleanup(&p.dir).unwrap();
    }

    #[test]
    fn traversal_entry_is_rejected() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let z = w.path().join("evil.zip");
        write_zip(&z, &[("ok.json", b"{}"), ("../x", b"nope")]);
        let err = unpack_blocking(z.to_str().unwrap()).unwrap_err();
        assert_eq!(
            err,
            "This archive has an entry outside its folder (../x): refusing to import it"
        );
        assert!(!w.path().join("x").exists());
    }

    #[test]
    fn absolute_entry_is_rejected() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let z = w.path().join("abs.zip");
        write_zip(&z, &[("/tmp/atlas-abs-test", b"nope")]);
        let err = unpack_blocking(z.to_str().unwrap()).unwrap_err();
        assert!(err.contains("/tmp/atlas-abs-test"), "{err}");
        assert!(err.contains("outside its folder"), "{err}");
    }

    // Unix symlinks (Windows needs Developer Mode to make one).
    #[cfg(unix)]
    #[test]
    fn symlink_is_dropped() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let src = w.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("a.json"), "{}").unwrap();
        std::os::unix::fs::symlink("/etc/hosts", src.join("link.json")).unwrap();
        let z = w.path().join("sym.zip");
        let st = Command::new("/usr/bin/zip")
            .current_dir(&src)
            .args(["-qry"])
            .arg(&z)
            .arg(".")
            .status()
            .unwrap();
        assert!(st.success());
        let p = unpack_blocking(z.to_str().unwrap()).unwrap();
        assert_eq!(rels(&p), ["a.json"]);
        assert!(fs::symlink_metadata(Path::new(&p.dir).join("link.json")).is_err());
        cleanup(&p.dir).unwrap();
    }

    #[test]
    fn entry_count_cap() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let z = w.path().join("many.zip");
        let names: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("f{i}.txt")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        write_zip(&z, &entries);
        let err = unpack_blocking(z.to_str().unwrap()).unwrap_err();
        assert!(err.contains("more than 5000 entries"), "{err}");
    }

    // Unix symlinks (Windows needs Developer Mode to make one).
    #[cfg(unix)]
    #[test]
    fn read_packed_rejects_traversal_symlink_and_non_utf8() {
        let w = tempfile::Builder::new()
            .prefix(TEMP_PREFIX)
            .tempdir()
            .unwrap();
        let d = w.path();
        fs::write(d.join("ok.txt"), "héllo").unwrap();
        fs::write(d.join("bin.dat"), [0xff, 0xfe, 0x00]).unwrap();
        fs::write(d.join("big.txt"), vec![b'a'; (MAX_READ_BYTES + 1) as usize]).unwrap();
        fs::write(
            d.parent().unwrap().join("atlas-theme-outside.txt"),
            "secret",
        )
        .unwrap();
        std::os::unix::fs::symlink("/etc/hosts", d.join("link")).unwrap();
        let dir = d.to_str().unwrap();
        register(d);
        assert_eq!(read_packed(dir, "ok.txt").unwrap(), "héllo");
        assert!(
            read_packed(dir, "../atlas-theme-outside.txt")
                .unwrap_err()
                .contains("outside")
        );
        assert!(
            read_packed(dir, "/etc/hosts")
                .unwrap_err()
                .contains("outside")
        );
        assert!(read_packed(dir, "link").unwrap_err().contains("symlink"));
        assert!(read_packed(dir, "bin.dat").unwrap_err().contains("UTF-8"));
        assert!(read_packed(dir, "big.txt").unwrap_err().contains("2 MB"));
        assert!(read_packed(dir, "missing.txt").is_err());
        let _ = fs::remove_file(d.parent().unwrap().join("atlas-theme-outside.txt"));
        unregister(d);
    }

    #[test]
    fn read_packed_only_serves_registered_roots() {
        let w = tempfile::tempdir().unwrap();
        fs::write(w.path().join("a.json"), "{}").unwrap();
        let dir = w.path().to_str().unwrap();
        // A real dir holding the right file, but never issued by theme_unpack.
        assert!(
            read_packed(dir, "a.json")
                .unwrap_err()
                .contains("not a package opened by Atlas")
        );
        // A folder import registers its root; cleanup unregisters it (and keeps the folder).
        let p = unpack_blocking(dir).unwrap();
        assert_eq!(read_packed(&p.dir, "a.json").unwrap(), "{}");
        cleanup(&p.dir).unwrap();
        assert!(w.path().join("a.json").exists());
        assert!(
            read_packed(&p.dir, "a.json")
                .unwrap_err()
                .contains("not a package opened by Atlas")
        );
    }

    #[test]
    fn read_packed_after_cleanup_of_temp_archive_fails() {
        if !have_zip() {
            return;
        }
        let w = tempfile::tempdir().unwrap();
        let z = w.path().join("p.zip");
        write_zip(&z, &[("a.json", b"{}")]);
        let p = unpack_blocking(z.to_str().unwrap()).unwrap();
        assert_eq!(read_packed(&p.dir, "a.json").unwrap(), "{}");
        cleanup(&p.dir).unwrap();
        assert!(read_packed(&p.dir, "a.json").is_err());
    }

    // Unix symlinks (Windows needs Developer Mode to make one).
    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_foreign_dir_and_folder_import_survives() {
        // A dir that is not ours (no prefix): left alone.
        let foreign = tempfile::tempdir().unwrap();
        fs::write(foreign.path().join("keep.json"), "{}").unwrap();
        cleanup(foreign.path().to_str().unwrap()).unwrap();
        assert!(foreign.path().join("keep.json").exists());

        // Our prefix but nested deeper than the temp root: also refused.
        let nested = foreign.path().join(format!("{TEMP_PREFIX}fake"));
        fs::create_dir(&nested).unwrap();
        cleanup(nested.to_str().unwrap()).unwrap();
        assert!(nested.exists());

        // Folder import: listed in place, not temporary, cleanup is a no-op.
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir(folder.path().join("themes")).unwrap();
        fs::write(folder.path().join("themes/a.json"), "{}").unwrap();
        std::os::unix::fs::symlink("/etc/hosts", folder.path().join("link")).unwrap();
        let p = unpack_blocking(folder.path().to_str().unwrap()).unwrap();
        assert!(!p.temporary);
        assert_eq!(rels(&p), ["themes/a.json"]);
        cleanup(&p.dir).unwrap();
        assert!(folder.path().join("themes/a.json").exists());
        // folder import must not delete symlinks either
        assert!(fs::symlink_metadata(folder.path().join("link")).is_ok());
    }

    #[test]
    fn non_archive_file_is_refused() {
        let w = tempfile::tempdir().unwrap();
        let f = w.path().join("notes.txt");
        fs::write(&f, "x").unwrap();
        assert!(
            unpack_blocking(f.to_str().unwrap())
                .unwrap_err()
                .contains(".zip")
        );
    }
}
