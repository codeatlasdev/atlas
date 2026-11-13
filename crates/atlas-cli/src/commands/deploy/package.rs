//! Build once, locally: sha, dirty check, build command, tarball, sha256.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use atlas_tui::config::DeployConfig;
use sha2::{Digest, Sha256};

pub struct Release {
    pub id: String,
    pub sha: String,
    pub sha256: String,
    pub tarball: PathBuf,
}

impl Drop for Release {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tarball);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Which `tar` is on PATH. Decides the flags that keep macOS metadata out of
/// the artifact and make it reproducible: bsdtar and GNU tar spell them
/// differently, and busybox rejects most of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarFlavor {
    /// libarchive's bsdtar (macOS, FreeBSD, Windows).
    Bsd,
    /// GNU tar.
    Gnu,
    /// busybox or anything unrecognised: only portable flags.
    Other,
}

impl TarFlavor {
    /// Classify the output of `tar --version`.
    pub fn detect(version_output: &str) -> Self {
        let v = version_output.to_ascii_lowercase();
        if v.contains("bsdtar") || v.contains("libarchive") {
            Self::Bsd
        } else if v.contains("gnu tar") {
            Self::Gnu
        } else {
            Self::Other
        }
    }

    /// Flags for `tar -c`. `mtime` (unix seconds) pins every entry's mtime where
    /// the flavour can; bsdtar has no such option and keeps file mtimes.
    pub fn create_flags(self, mtime: Option<&str>) -> Vec<String> {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        match self {
            // `-` as the archive: gzip happens in-process (see `tar_artifact`).
            Self::Bsd => s(&[
                "--no-xattrs",
                "--no-mac-metadata",
                "--uid",
                "0",
                "--gid",
                "0",
                "--numeric-owner",
            ]),
            Self::Gnu => {
                let mut f = s(&[
                    "--no-xattrs",
                    "--owner=0",
                    "--group=0",
                    "--numeric-owner",
                    "--sort=name",
                ]);
                if let Some(t) =
                    mtime.filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
                {
                    // Every entry gets the commit time (no `--clamp-mtime`, which only caps newer ones).
                    f.push(format!("--mtime=@{t}"));
                }
                f
            }
            Self::Other => vec![],
        }
    }
}

fn tar_flavor() -> TarFlavor {
    Command::new("tar")
        .arg("--version")
        .output()
        .map(|o| TarFlavor::detect(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(TarFlavor::Other)
}

/// `tar -c` of `paths` (relative to `root`) into a gzip file at `out`.
/// macOS xattrs (`LIBARCHIVE.xattr.*` pax headers) and AppleDouble files stay
/// out, so GNU tar on the host does not warn per file and the same content
/// hashes the same on any build machine. The gzip header is written without a
/// name or timestamp for the same reason.
fn tar_artifact(root: &Path, paths: &[String], mtime: Option<&str>, out: &Path) -> Result<()> {
    let mut child = Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
        .args(tar_flavor().create_flags(mtime))
        .arg("-cf")
        .arg("-")
        .arg("-C")
        .arg(root)
        .arg("--")
        .args(paths)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("running tar")?;
    let mut gz = flate2::GzBuilder::new().write(
        std::fs::File::create(out).context("creating artifact")?,
        flate2::Compression::default(),
    );
    let mut stdout = child.stdout.take().context("tar stdout")?;
    // stderr is small; drain it on a thread so a chatty tar cannot block on a full pipe.
    let mut stderr = child.stderr.take().context("tar stderr")?;
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let copied = std::io::copy(&mut stdout, &mut gz).map(drop);
    let finished = gz.finish().map(drop);
    let st = child.wait().context("waiting for tar")?;
    let msg = err.join().unwrap_or_default();
    if !st.success() {
        bail!("packaging failed: {}", msg.trim());
    }
    copied.and(finished).context("compressing artifact")?;
    Ok(())
}

/// Full sha of `HEAD`.
pub fn head(root: &Path) -> Result<String> {
    git(root, &["rev-parse", "HEAD"])
}

pub fn package(
    root: &Path,
    cfg: &DeployConfig,
    sha: &str,
    allow_dirty: bool,
    skip_build: bool,
) -> Result<Release> {
    let sha = sha.to_string();
    let dirty = !git(root, &["status", "--porcelain"])?.is_empty();
    if dirty && !allow_dirty {
        bail!("working tree dirty: commit or pass --allow-dirty");
    }
    let suffix = if dirty {
        format!("{}-dirty", &sha[..7])
    } else {
        sha[..7].to_string()
    };
    let id = format!("{}-{suffix}", chrono::Utc::now().format("%Y%m%d%H%M%S"));

    if let Some(build) = cfg.build.as_deref().filter(|_| !skip_build) {
        eprintln!("▸ build: {build}");
        // The project's own build command, in this machine's shell (`cmd` on Windows).
        let st = atlas_core::sys::shell(build)
            .envs(atlas_core::login_env::vars())
            .current_dir(root)
            .status()
            .context("running build")?;
        if !st.success() {
            bail!("build failed (exit {})", st.code().unwrap_or(-1));
        }
    }

    let tarball = std::env::temp_dir().join(format!("atlas-deploy-{id}.tar.gz"));
    let mut release = Release {
        id,
        sha,
        sha256: String::new(),
        tarball,
    };
    match &cfg.artifact {
        Some(paths) => {
            for p in paths {
                if !root.join(p).exists() {
                    bail!("artifact path not found: {p}");
                }
            }
            let mtime = git(root, &["show", "-s", "--format=%ct", &release.sha]).ok();
            tar_artifact(root, paths, mtime.as_deref(), &release.tarball)?;
        }
        None => {
            let out = Command::new("git")
                .args(["archive", "--format=tar.gz", "-o"])
                .arg(&release.tarball)
                .arg("HEAD")
                .current_dir(root)
                .output()
                .context("creating artifact")?;
            if !out.status.success() {
                bail!(
                    "packaging failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
    }
    release.sha256 = sha256_file(&release.tarball)?;
    Ok(release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tar_flavor_is_detected_from_version_output() {
        for (out, want) in [
            (
                "bsdtar 3.5.3 - libarchive 3.7.4 zlib/1.2.12 liblzma/5.4.3 bz2lib/1.0.8",
                TarFlavor::Bsd,
            ),
            ("tar (GNU tar) 1.35\nCopyright (C) 2023", TarFlavor::Gnu),
            // busybox prints its own banner (and exits non-zero on --version).
            (
                "tar: unrecognized option: version\nBusyBox v1.36.1",
                TarFlavor::Other,
            ),
            ("", TarFlavor::Other),
        ] {
            assert_eq!(TarFlavor::detect(out), want, "{out:?}");
        }
    }

    // Each flavour rejects the other's spelling (`bsdtar: --sort is not supported`,
    // GNU: `--no-mac-metadata` unknown), so a wrong pick breaks every deploy.
    #[test]
    fn create_flags_never_mix_flavours() {
        let bsd = TarFlavor::Bsd.create_flags(Some("1700000000"));
        assert!(bsd.contains(&"--no-xattrs".into()) && bsd.contains(&"--no-mac-metadata".into()));
        assert!(
            !bsd.iter()
                .any(|f| f.starts_with("--mtime") || f.starts_with("--sort")),
            "{bsd:?}"
        );

        let gnu = TarFlavor::Gnu.create_flags(Some("1700000000"));
        assert!(gnu.contains(&"--no-xattrs".into()));
        assert!(gnu.contains(&"--mtime=@1700000000".into()) && gnu.contains(&"--sort=name".into()));
        assert!(!gnu.contains(&"--no-mac-metadata".into()), "{gnu:?}");

        // Unknown tar: nothing it might reject.
        assert!(TarFlavor::Other.create_flags(Some("1700000000")).is_empty());
    }

    #[test]
    fn gnu_mtime_must_be_plain_digits() {
        // The value comes from `git show`; anything else must not reach argv.
        for bad in ["", "1700000000; rm -rf /", "-1", "@5"] {
            let f = TarFlavor::Gnu.create_flags(Some(bad));
            assert!(
                !f.iter().any(|a| a.starts_with("--mtime")),
                "{bad:?}: {f:?}"
            );
        }
        assert!(
            !TarFlavor::Gnu
                .create_flags(None)
                .iter()
                .any(|a| a.starts_with("--mtime"))
        );
    }

    fn gunzip(path: &Path) -> Vec<u8> {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
            .read_to_end(&mut raw)
            .unwrap();
        raw
    }

    #[cfg(target_os = "macos")]
    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    // Regression: artifacts built on macOS carried `LIBARCHIVE.xattr.com.apple.provenance`
    // pax headers, so GNU tar on the host warned once per file (issue #171).
    #[cfg(target_os = "macos")]
    #[test]
    fn artifact_has_no_apple_xattr_headers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("dist")).unwrap();
        let f = dir.path().join("dist/app.js");
        std::fs::write(&f, "x").unwrap();
        let set = Command::new("xattr")
            .args(["-w", "com.apple.provenance", "x"])
            .arg(&f)
            .status();
        if !set.is_ok_and(|s| s.success()) {
            eprintln!("skipped: `xattr` tool unavailable or refused");
            return;
        }
        // Guard the guard: plain bsdtar really does leak the header here.
        if let Ok(o) = Command::new("/usr/bin/tar")
            .args(["-cf", "-", "-C"])
            .arg(dir.path())
            .arg("dist")
            .output()
        {
            assert!(
                contains(&o.stdout, b"LIBARCHIVE.xattr"),
                "fixture does not reproduce the leak; test would be vacuous"
            );
        }
        let out = dir.path().join("a.tar.gz");
        tar_artifact(dir.path(), &["dist".into()], Some("1700000000"), &out).unwrap();
        let raw = gunzip(&out);
        assert!(contains(&raw, b"dist/app.js"), "artifact lost its file");
        assert!(
            !contains(&raw, b"LIBARCHIVE.xattr"),
            "xattr pax header leaked"
        );
        assert!(!contains(&raw, b"._app.js"), "AppleDouble file leaked");
    }

    // Same content must hash the same on any build machine and any run. GNU can pin
    // mtime; bsdtar cannot, so there only the headers we control are compared.
    #[test]
    fn artifact_is_reproducible_across_runs_and_owners() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("dist")).unwrap();
        for n in ["b.js", "a.js", "c.js"] {
            std::fs::write(dir.path().join("dist").join(n), n).unwrap();
        }
        let paths = ["dist".to_string()];
        let (one, two) = (dir.path().join("1.tgz"), dir.path().join("2.tgz"));
        tar_artifact(dir.path(), &paths, Some("1700000000"), &one).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        // Touch every file: only mtimes change.
        for n in ["b.js", "a.js", "c.js"] {
            let p = dir.path().join("dist").join(n);
            let c = std::fs::read(&p).unwrap();
            std::fs::write(&p, c).unwrap();
        }
        tar_artifact(dir.path(), &paths, Some("1700000000"), &two).unwrap();

        // gzip header is nameless and timestampless in both flavours.
        let head = std::fs::read(&one).unwrap();
        assert_eq!(&head[4..8], &[0, 0, 0, 0], "gzip MTIME must be 0");
        assert_eq!(head[3], 0, "gzip FLG must carry no filename");
        assert_eq!(gunzip(&one).len(), gunzip(&two).len());
        if tar_flavor() == TarFlavor::Gnu {
            assert_eq!(
                sha256_file(&one).unwrap(),
                sha256_file(&two).unwrap(),
                "same content, different mtimes: hashes must match"
            );
        }
    }
}
