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

pub fn package(
    root: &Path,
    cfg: &DeployConfig,
    allow_dirty: bool,
    skip_build: bool,
) -> Result<Release> {
    let sha = git(root, &["rev-parse", "HEAD"])?;
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
        let st = Command::new("sh")
            .args(["-c", build])
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
    let out = match &cfg.artifact {
        Some(paths) => {
            for p in paths {
                if !root.join(p).exists() {
                    bail!("artifact path not found: {p}");
                }
            }
            Command::new("tar")
                .arg("czf")
                .arg(&release.tarball)
                .arg("-C")
                .arg(root)
                .arg("--")
                .args(paths)
                .output()
        }
        None => Command::new("git")
            .args(["archive", "--format=tar.gz", "-o"])
            .arg(&release.tarball)
            .arg("HEAD")
            .current_dir(root)
            .output(),
    }
    .context("creating artifact")?;
    if !out.status.success() {
        bail!(
            "packaging failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    release.sha256 = sha256_file(&release.tarball)?;
    Ok(release)
}
