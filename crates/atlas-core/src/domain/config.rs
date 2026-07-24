use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub socket_path: PathBuf,
    pub db_path: PathBuf,
    pub log_level: String,
    pub ssh_key_path: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs_path();
        Self {
            socket_path: home.join("atlas.sock"),
            db_path: home.join("atlas.db"),
            log_level: "info".to_string(),
            ssh_key_path: None,
        }
    }
}

fn dirs_path() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join(".atlas")
}
