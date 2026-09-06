#![allow(dead_code)]

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::env;

const MANIFEST_URL: &str = "https://releases.atlas.codeatlas.com.br/manifest.json";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub channel: String,
    pub date: String,
    pub assets: std::collections::HashMap<String, PlatformAssets>,
    pub changelog: Option<String>,
    pub required_update: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PlatformAssets {
    pub cli: AssetInfo,
}

#[derive(Debug, Deserialize)]
pub struct AssetInfo {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstallMethod {
    Homebrew,
    ShellInstaller,
    Manual,
}

impl InstallMethod {
    pub fn detect() -> Self {
        let exe = env::current_exe().unwrap_or_default();
        let path = exe.to_string_lossy();

        if path.contains("/opt/homebrew")
            || path.contains("/usr/local/Cellar")
            || path.contains("/home/linuxbrew")
        {
            Self::Homebrew
        } else if path.contains(".atlas/bin") {
            Self::ShellInstaller
        } else {
            Self::Manual
        }
    }

    pub fn update_instruction(&self) -> String {
        match self {
            Self::Homebrew => "brew upgrade atlas".to_string(),
            Self::ShellInstaller | Self::Manual => "atlas self-update".to_string(),
        }
    }
}

pub fn current_platform() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };

    if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else {
        format!("{arch}-unknown-linux-gnu")
    }
}

pub async fn handle(check_only: bool, channel: Option<String>) -> Result<()> {
    let method = InstallMethod::detect();
    if method == InstallMethod::Homebrew {
        println!("{}", method.update_instruction());
        return Ok(());
    }

    println!("\x1b[1;34m\u{25cf}\x1b[0m Checking for updates...");
    println!("  Current: v{CURRENT_VERSION}");
    println!("  Channel: {}", channel.as_deref().unwrap_or("stable"));
    println!();

    let manifest = fetch_manifest(channel.as_deref().unwrap_or("stable")).await?;

    let current =
        semver::Version::parse(CURRENT_VERSION).context("Failed to parse current version")?;
    let latest =
        semver::Version::parse(&manifest.version).context("Failed to parse latest version")?;

    if latest <= current {
        println!("\x1b[1;32m\u{2713}\x1b[0m Already up to date (v{CURRENT_VERSION})");
        return Ok(());
    }

    println!("  Latest:  v{}", manifest.version);
    if let Some(ref url) = manifest.changelog {
        println!("  Changes: {url}");
    }
    println!();

    if check_only {
        println!(
            "\x1b[1;33m\u{25cf}\x1b[0m Update available: v{} \u{2192} v{}",
            CURRENT_VERSION, manifest.version
        );
        return Ok(());
    }

    let platform = current_platform();
    let assets = manifest
        .assets
        .get(&platform)
        .ok_or_else(|| anyhow::anyhow!("No release for platform: {platform}"))?;

    println!(
        "\x1b[1;34m\u{25cf}\x1b[0m Downloading v{}...",
        manifest.version
    );
    let binary = download_and_verify(&assets.cli).await?;

    println!("\x1b[1;34m\u{25cf}\x1b[0m Installing...");
    let tmp_path = std::env::temp_dir().join("atlas-update-tmp");
    std::fs::write(&tmp_path, &binary)?;
    self_replace::self_replace(&tmp_path)
        .context("Failed to replace binary. Try running with sudo.")?;
    let _ = std::fs::remove_file(&tmp_path);

    println!();
    println!("\x1b[1;32m\u{2713}\x1b[0m Updated to v{}", manifest.version);
    Ok(())
}

async fn fetch_manifest(channel: &str) -> Result<UpdateManifest> {
    let url = if channel == "stable" {
        MANIFEST_URL.to_string()
    } else {
        format!("https://releases.atlas.codeatlas.com.br/{channel}/manifest.json")
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => Ok(resp.json().await?),
        _ => fetch_manifest_from_github(channel).await,
    }
}

async fn fetch_manifest_from_github(channel: &str) -> Result<UpdateManifest> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("atlas-cli")
        .build()?;

    let resp: serde_json::Value = client
        .get("https://api.github.com/repos/codeatlasdev/atlas/releases/latest")
        .send()
        .await?
        .json()
        .await?;

    let tag = resp["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No tag_name in release"))?;
    let version = tag.trim_start_matches('v').to_string();

    let platform = current_platform();
    let tarball_name = format!("atlas-{version}-{platform}.tar.gz");

    let assets_arr = resp["assets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No assets in release"))?;

    let asset = assets_arr
        .iter()
        .find(|a| a["name"].as_str() == Some(&tarball_name))
        .ok_or_else(|| anyhow::anyhow!("No asset for platform: {platform}"))?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No download URL"))?
        .to_string();

    let mut assets_map = std::collections::HashMap::new();
    assets_map.insert(
        platform,
        PlatformAssets {
            cli: AssetInfo {
                url: download_url,
                sha256: String::new(),
                size: asset["size"].as_u64().unwrap_or(0),
            },
        },
    );

    Ok(UpdateManifest {
        version,
        channel: channel.to_string(),
        date: resp["published_at"].as_str().unwrap_or("").to_string(),
        assets: assets_map,
        changelog: resp["html_url"].as_str().map(|s| s.to_string()),
        required_update: None,
    })
}

async fn download_and_verify(asset: &AssetInfo) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let bytes = client
        .get(&asset.url)
        .header("Accept", "application/octet-stream")
        .send()
        .await?
        .error_for_status()
        .context("Download failed")?
        .bytes()
        .await?
        .to_vec();

    if !asset.sha256.is_empty() {
        use sha2::{Digest, Sha256};
        let hash = hex::encode(Sha256::digest(&bytes));
        if hash != asset.sha256 {
            bail!(
                "SHA256 mismatch!\n  Expected: {}\n  Got: {}",
                asset.sha256,
                hash
            );
        }
    }

    if asset.url.ends_with(".tar.gz") {
        extract_tarball(&bytes)
    } else {
        Ok(bytes)
    }
}

fn extract_tarball(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let gz = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path
            .file_name()
            .map(|n| n.to_string_lossy() == "atlas")
            .unwrap_or(false)
        {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }

    bail!("Could not find atlas binary in tarball")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform() {
        let platform = current_platform();
        assert!(platform.contains("apple-darwin") || platform.contains("linux"));
        assert!(platform.starts_with("aarch64") || platform.starts_with("x86_64"));
    }

    #[test]
    fn test_install_method_detect() {
        let method = InstallMethod::detect();
        assert!(matches!(
            method,
            InstallMethod::Manual | InstallMethod::ShellInstaller | InstallMethod::Homebrew
        ));
    }

    #[test]
    fn test_version_comparison() {
        let current = semver::Version::parse("0.1.0").unwrap();
        let latest = semver::Version::parse("0.2.0").unwrap();
        assert!(latest > current);
    }

    #[test]
    fn test_platform_format() {
        let p = current_platform();
        let parts: Vec<&str> = p.split('-').collect();
        assert!(parts.len() >= 3);
    }
}
