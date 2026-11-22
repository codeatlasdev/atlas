//! Shared release plumbing for `atlas update` and `atlas desktop`: repo constants and a
//! verified download of public release assets (signed `SHA256SUMS` from the release policy's
//! `signed_since` on, the per-asset `.sha256` before it).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atlas_core::github;
use atlas_core::release_verify::{self, Policy};
use atlas_i18n::t;
use sha2::{Digest, Sha256};

pub const REPO: &str = "codeatlasdev/atlas";

/// Download `asset` from the public release `tag` into `dir` and verify it. Fails closed.
///
/// - A tag at or after `signed_since` must carry `SHA256SUMS` and `SHA256SUMS.sig`, signed by one
///   of the embedded maintainer keys, and the asset must match its line in that signed file. A
///   missing or invalid signature is an error with no override: the `.sha256` next to the asset
///   comes from the same release and proves nothing.
/// - An older tag (or any tag while `signed_since` is unset) keeps the `.sha256` check.
///
/// `on_progress(bytes_done, bytes_total)` reports the asset itself (total 0 = unknown).
pub fn download_verified(
    tag: &str,
    asset: &str,
    dir: &Path,
    on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf> {
    let policy = Policy::current()?;
    let tarball = dir.join(asset);
    let missing = |name: &str| {
        t!(
            "cmd_misc_release_ctx_missing_asset",
            tag = tag,
            asset = name
        )
    };
    if !policy.requires_signature(tag) {
        if policy.is_active() {
            eprintln!(
                "\x1b[33m◆\x1b[0m {}",
                t!("cmd_misc_release_predates_signing", tag = tag, asset = asset)
            );
        }
        let sum_name = format!("{asset}.sha256");
        github::download_asset_progress(REPO, tag, asset, &tarball, on_progress)
            .with_context(|| missing(asset))?;
        github::download_asset(REPO, tag, &sum_name, &dir.join(&sum_name))
            .with_context(|| missing(&sum_name))?;
        verify_sha256(&tarball)?;
        return Ok(tarball);
    }

    // The signature first (a few KB): a forged release costs no asset download.
    let fetch = |name: &str| -> Result<PathBuf> {
        let path = dir.join(name);
        github::download_asset(REPO, tag, name, &path).with_context(|| {
            t!(
                "cmd_misc_release_ctx_missing_signed",
                missing = missing(name)
            )
        })?;
        Ok(path)
    };
    let sums = std::fs::read(fetch("SHA256SUMS")?)?;
    let sig = std::fs::read_to_string(fetch("SHA256SUMS.sig")?)?;
    let key = policy
        .verify(&sums, &sig)
        .with_context(|| t!("cmd_misc_release_sig_invalid", tag = tag))?;
    println!(
        "\x1b[1;32m✓\x1b[0m {}",
        t!("cmd_misc_release_signed_by", key = &key.to_string())
    );
    let expected = release_verify::expected_hash(&String::from_utf8_lossy(&sums), asset)
        .with_context(|| t!("cmd_misc_release_refusing_asset", tag = tag, asset = asset))?;
    github::download_asset_progress(REPO, tag, asset, &tarball, on_progress)
        .with_context(|| missing(asset))?;
    check_sha256(&tarball, &expected)?;
    Ok(tarball)
}

/// Check `tarball` against the `<tarball>.sha256` file next to it (`shasum` format).
pub fn verify_sha256(tarball: &Path) -> Result<()> {
    let name = file_name(tarball);
    let mut sum = tarball.as_os_str().to_owned();
    sum.push(".sha256");
    let sum = PathBuf::from(sum);
    if !sum.is_file() {
        bail!(t!("cmd_misc_release_no_checksum", name = &name));
    }
    let expected = std::fs::read_to_string(&sum)?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    check_sha256(tarball, &expected)
}

/// `tarball` must hash to `expected` (lowercase hex sha256).
fn check_sha256(tarball: &Path, expected: &str) -> Result<()> {
    let actual = hex::encode(Sha256::digest(std::fs::read(tarball)?));
    if expected != actual {
        bail!(t!(
            "cmd_misc_release_checksum_mismatch",
            name = &file_name(tarball),
            expected = expected,
            actual = &actual
        ));
    }
    Ok(())
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
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
