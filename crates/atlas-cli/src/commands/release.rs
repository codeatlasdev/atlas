//! Shared release plumbing for `atlas update` and `atlas desktop`: repo constants and a
//! checksum-verified download of public release assets.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atlas_core::github;
use sha2::{Digest, Sha256};

pub const REPO: &str = "codeatlasdev/atlas";

/// Download `asset` and its `.sha256` from the public release `tag` into `dir` and verify
/// the checksum. Fails closed: a missing asset, a missing checksum or a mismatch is an error.
pub fn download_verified(tag: &str, asset: &str, dir: &Path) -> Result<PathBuf> {
    let tarball = dir.join(asset);
    let sum = dir.join(format!("{asset}.sha256"));
    for (name, dest) in [
        (asset.to_string(), &tarball),
        (format!("{asset}.sha256"), &sum),
    ] {
        github::download_asset(REPO, tag, &name, dest)
            .with_context(|| format!("{tag} has no {name} (download failed or missing asset)"))?;
    }
    verify_sha256(&tarball)?;
    Ok(tarball)
}

/// Check `tarball` against the `<tarball>.sha256` file next to it (`shasum` format).
pub fn verify_sha256(tarball: &Path) -> Result<()> {
    let name = tarball
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut sum = tarball.as_os_str().to_owned();
    sum.push(".sha256");
    let sum = PathBuf::from(sum);
    if !sum.is_file() {
        bail!("{name} has no {name}.sha256; refusing unverified install");
    }
    let expected = std::fs::read_to_string(&sum)?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let actual = hex::encode(Sha256::digest(std::fs::read(tarball)?));
    if expected != actual {
        bail!("checksum mismatch for {name} (expected {expected}, got {actual})");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tarball(dir: &Path) -> PathBuf {
        let p = dir.join("a.tar.gz");
        std::fs::write(&p, b"payload").unwrap();
        p
    }

    #[test]
    fn verify_sha256_accepts_matching_checksum() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tarball(tmp.path());
        let digest = hex::encode(Sha256::digest(b"payload"));
        std::fs::write(
            tmp.path().join("a.tar.gz.sha256"),
            format!("{digest}  a.tar.gz\n"),
        )
        .unwrap();
        verify_sha256(&p).unwrap();
    }

    #[test]
    fn verify_sha256_rejects_wrong_checksum() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tarball(tmp.path());
        std::fs::write(
            tmp.path().join("a.tar.gz.sha256"),
            format!("{}  a.tar.gz\n", "0".repeat(64)),
        )
        .unwrap();
        let e = verify_sha256(&p).unwrap_err().to_string();
        assert!(e.contains("checksum mismatch"), "{e}");
    }

    #[test]
    fn verify_sha256_refuses_missing_checksum() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tarball(tmp.path());
        let e = verify_sha256(&p).unwrap_err().to_string();
        assert!(e.contains("refusing unverified install"), "{e}");
    }
}
