//! The `gh` CLI on a crew server: install it without root (checksummed release tarball), set git up
//! to use it, read the login out of `gh auth status`, and read the device code of `gh auth login`.

/// Returns the tarball asset name for this version and arch suffix (e.g. `"linux_amd64"`).
fn gh_asset_name(version: &str, arch_suffix: &str) -> String {
    format!("gh_{version}_{arch_suffix}.tar.gz")
}

/// Maps `std::env::consts::ARCH` values to the suffixes used in gh release asset names.
/// Returns `None` for unsupported architectures.
fn gh_arch_suffix(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("linux_amd64"),
        "aarch64" => Some("linux_arm64"),
        _ => None,
    }
}

/// Verify the SHA-256 of `data` against `checksums` (GitHub's `checksums.txt` format:
/// `<hex>  <filename>` lines, two spaces).  Rejects a mismatch and a missing entry.
fn verify_gh_checksum(data: &[u8], checksums: &str, filename: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let expected = checksums
        .lines()
        .find_map(|line| {
            let (hash, name) = line.split_once("  ")?;
            if name.trim() == filename {
                Some(hash.trim().to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!("checksum for {filename} not found in checksums file"))?;
    let actual = hex::encode(Sha256::digest(data));
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {filename}: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

/// Configure git to use `gh` as the HTTPS credential helper for github.com.
pub(super) async fn gh_auth_setup_git() -> Result<(), String> {
    let out = tokio::process::Command::new("gh")
        .args(["auth", "setup-git"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("gh auth setup-git: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("gh auth setup-git failed: {stderr}"));
    }
    Ok(())
}

/// Ensure the `gh` CLI is available.  If not found on PATH, downloads the official release
/// binary for the current arch into `~/.local/bin` without requiring root.
/// Verifies the SHA-256 checksum before extracting.
pub(super) async fn ensure_gh_installed(home: &std::path::Path) -> Result<(), String> {
    // Fast path: already on PATH.
    if tokio::process::Command::new("gh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let arch = std::env::consts::ARCH;
    let arch_suffix = gh_arch_suffix(arch)
        .ok_or_else(|| format!("unsupported architecture for gh auto-install: {arch}"))?;
    let local_bin = home.join(".local/bin");
    std::fs::create_dir_all(&local_bin).map_err(|e| format!("cannot create ~/.local/bin: {e}"))?;
    // Use ring crypto provider (may already be installed; ignore the error).
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent("atlas-host/1.0")
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    // Fetch latest release metadata from GitHub API.
    let release: serde_json::Value = client
        .get("https://api.github.com/repos/cli/cli/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("fetch gh release info: {e}"))?
        .json()
        .await
        .map_err(|e| format!("parse gh release metadata: {e}"))?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or("gh release has no tag_name")?;
    let version = tag.trim_start_matches('v');
    let asset_name = gh_asset_name(version, arch_suffix);
    let checksums_name = format!("gh_{version}_checksums.txt");
    let assets = release["assets"]
        .as_array()
        .ok_or("gh release has no assets array")?;
    let asset_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&asset_name))
        .and_then(|a| a["browser_download_url"].as_str().map(str::to_string))
        .ok_or_else(|| format!("gh release asset {asset_name} not found"))?;
    let checksums_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&checksums_name))
        .and_then(|a| a["browser_download_url"].as_str().map(str::to_string))
        .ok_or_else(|| format!("gh release checksums file {checksums_name} not found"))?;
    let checksums = client
        .get(&checksums_url)
        .send()
        .await
        .map_err(|e| format!("download gh checksums: {e}"))?
        .text()
        .await
        .map_err(|e| format!("read gh checksums: {e}"))?;
    let tarball_bytes = client
        .get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("download gh tarball: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("read gh tarball: {e}"))?;
    verify_gh_checksum(&tarball_bytes, &checksums, &asset_name)?;
    // Extract `bin/gh` from the tarball.
    let dir_name = asset_name.trim_end_matches(".tar.gz");
    let expected_inner = format!("{dir_name}/bin/gh");
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(&tarball_bytes[..]));
    let mut archive = tar::Archive::new(gz);
    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|e| format!("read gh tarball entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("gh tarball entry error: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("gh tarball path: {e}"))?
            .to_path_buf();
        if path.to_str() == Some(&expected_inner) {
            let dest = local_bin.join("gh");
            entry
                .unpack(&dest)
                .map_err(|e| format!("extract gh binary: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("chmod gh: {e}"))?;
            }
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!(
            "gh binary not found at {expected_inner} inside tarball {asset_name}"
        ));
    }
    Ok(())
}

/// Parse `Logged in to github.com account <login>` from `gh auth status` output.
pub(super) fn parse_gh_login(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.split("account ").nth(1) {
            let login = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
            if !login.is_empty() {
                return Some(login.to_string());
            }
        }
    }
    None
}

/// Read lines from stdout + stderr of a `gh auth login` process until the device
/// user_code and verification_uri appear, or `timeout` elapses.
pub(super) async fn wait_for_gh_code(
    stdout: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    stderr: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    timeout: std::time::Duration,
) -> Option<(String, String)> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::mpsc;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let mut r = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx.send(line);
        }
    });
    tokio::spawn(async move {
        let mut r = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx2.send(line);
        }
    });
    let mut user_code: Option<String> = None;
    let verification_uri = "https://github.com/login/device".to_string();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(line)) => {
                if user_code.is_none()
                    && let Some(code) = extract_device_code(&line)
                {
                    user_code = Some(code);
                }
                if let Some(code) = user_code.take() {
                    return Some((code, verification_uri));
                }
            }
            Ok(None) | Err(_) => return None,
        }
    }
}

/// Extract the device user_code from a gh output line.
/// Matches tokens shaped XXXX-XXXX where X is A-Z or 0-9.
fn extract_device_code(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        let parts: Vec<&str> = t.split('-').collect();
        if parts.len() == 2 {
            let a = parts[0];
            let b = parts[1];
            if a.len() == 4
                && b.len() == 4
                && a.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && b.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                return Some(t.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gh_asset_name_for_known_arches() {
        assert_eq!(
            gh_asset_name("2.60.1", "linux_amd64"),
            "gh_2.60.1_linux_amd64.tar.gz"
        );
        assert_eq!(
            gh_asset_name("2.60.1", "linux_arm64"),
            "gh_2.60.1_linux_arm64.tar.gz"
        );
    }

    #[test]
    fn gh_arch_suffix_maps_rust_consts_to_asset_suffixes() {
        assert_eq!(gh_arch_suffix("x86_64"), Some("linux_amd64"));
        assert_eq!(gh_arch_suffix("aarch64"), Some("linux_arm64"));
        assert_eq!(gh_arch_suffix("arm"), None);
    }

    #[test]
    fn verify_gh_checksum_accepts_correct_hash() {
        use sha2::{Digest, Sha256};
        let data = b"hello gh";
        let hash = hex::encode(Sha256::digest(data));
        let checksums = format!("{hash}  gh_2.60.1_linux_amd64.tar.gz\n");
        verify_gh_checksum(data, &checksums, "gh_2.60.1_linux_amd64.tar.gz").unwrap();
    }

    #[test]
    fn verify_gh_checksum_rejects_wrong_hash() {
        let data = b"hello gh";
        let wrong = "0".repeat(64);
        let checksums = format!("{wrong}  gh_2.60.1_linux_amd64.tar.gz\n");
        let err = verify_gh_checksum(data, &checksums, "gh_2.60.1_linux_amd64.tar.gz").unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
    }

    #[test]
    fn verify_gh_checksum_rejects_missing_entry() {
        let data = b"hello gh";
        let checksums = "abc123  some_other_file.tar.gz\n";
        let err = verify_gh_checksum(data, checksums, "gh_2.60.1_linux_amd64.tar.gz").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn parse_gh_login_extracts_username() {
        let text = "github.com\n  x Logged in to github.com account alice (keyring)\n";
        assert_eq!(parse_gh_login(text), Some("alice".to_string()));
        assert_eq!(parse_gh_login("not logged in"), None);
    }

    #[test]
    fn extract_device_code_finds_code_in_gh_output() {
        assert_eq!(
            extract_device_code("! First copy your one-time code: ABCD-1234"),
            Some("ABCD-1234".to_string())
        );
        assert_eq!(extract_device_code("nothing here"), None);
        assert_eq!(
            extract_device_code("  code: WXYZ-9876  extra"),
            Some("WXYZ-9876".to_string())
        );
    }
}
