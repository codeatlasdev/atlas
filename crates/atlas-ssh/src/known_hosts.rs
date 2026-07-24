use std::path::{Path, PathBuf};

use atlas_core::{AtlasError, Result};

pub struct KnownHosts {
    path: PathBuf,
}

impl KnownHosts {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> PathBuf {
        dirs_home().join(".ssh").join("known_hosts")
    }

    #[allow(dead_code)]
    pub fn verify_host(&self, _host: &str, _port: u16, _key: &[u8]) -> Result<HostVerification> {
        // TODO: parse known_hosts file and verify key
        if !self.path.exists() {
            return Err(AtlasError::Ssh(format!(
                "known_hosts file not found: {}",
                self.path.display()
            )));
        }
        Ok(HostVerification::Unknown)
    }

    #[allow(dead_code)]
    pub fn add_host(&self, _host: &str, _port: u16, _key: &[u8]) -> Result<()> {
        // TODO: append host key to known_hosts
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostVerification {
    #[allow(dead_code)]
    Trusted,
    #[allow(dead_code)]
    Changed,
    Unknown,
}

fn dirs_home() -> &'static Path {
    static HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    })
}
