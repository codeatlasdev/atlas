#![allow(unused)]

use std::fs;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use crate::manifest::PluginManifest;

pub struct PluginLoader;

impl PluginLoader {
    pub fn scan_plugins(plugins_dir: &Path) -> Vec<PluginManifest> {
        let mut manifests = Vec::new();

        let entries = match fs::read_dir(plugins_dir) {
            Ok(entries) => entries,
            Err(e) => {
                debug!(?plugins_dir, %e, "could not read plugins directory");
                return manifests;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("manifest.toml");
                if manifest_path.exists() {
                    match Self::parse_manifest(&manifest_path) {
                        Ok(manifest) => {
                            debug!(name = %manifest.name, "loaded plugin manifest");
                            manifests.push(manifest);
                        }
                        Err(e) => {
                            warn!(?manifest_path, %e, "failed to parse plugin manifest");
                        }
                    }
                }
            }
        }

        manifests
    }

    pub fn load_all() -> Vec<PluginManifest> {
        let home = match std::env::var("HOME") {
            Ok(h) => PathBuf::from(h),
            Err(_) => return Vec::new(),
        };
        let plugins_dir = home.join(".atlas").join("plugins");
        Self::scan_plugins(&plugins_dir)
    }

    fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        toml::from_str(&content).map_err(|e| e.to_string())
    }
}
