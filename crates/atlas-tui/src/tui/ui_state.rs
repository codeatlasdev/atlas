//! Per-project UI memory in `.logs/.atlas-ui.json`: what you had open last time,
//! plus recently used palette commands. Best-effort: a missing or corrupt file
//! is simply a fresh start, and write failures are ignored.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = ".atlas-ui.json";
/// Recent palette commands kept.
pub const RECENT_MAX: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    /// Active tab by name (tab indexes shift between runs).
    pub tab: Option<String>,
    pub zoom: bool,
    pub wrap: Option<bool>,
    pub timestamps: Option<bool>,
    pub json_pretty: Option<bool>,
    /// `all` | `warn` | `error`.
    pub level: Option<String>,
    /// Palette command ids, most recent first.
    pub recent: Vec<String>,
}

impl UiState {
    pub fn path(log_dir: &Path) -> PathBuf {
        log_dir.join(FILE)
    }

    pub fn load(log_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(log_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, log_dir: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::create_dir_all(log_dir);
            let _ = std::fs::write(Self::path(log_dir), json);
        }
    }

    /// Move `id` to the front of the recent list.
    pub fn touch_recent(&mut self, id: &str) {
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_string());
        self.recent.truncate(RECENT_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_garbage() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(UiState::load(dir.path()), UiState::default());
        let mut s = UiState {
            tab: Some("api".into()),
            zoom: true,
            wrap: Some(false),
            level: Some("warn".into()),
            ..Default::default()
        };
        s.touch_recent("a");
        s.touch_recent("b");
        s.touch_recent("a");
        s.save(dir.path());
        let back = UiState::load(dir.path());
        assert_eq!(back, s);
        assert_eq!(back.recent, ["a", "b"]);
        std::fs::write(UiState::path(dir.path()), "{not json").unwrap();
        assert_eq!(UiState::load(dir.path()), UiState::default());
    }

    #[test]
    fn recent_is_capped() {
        let mut s = UiState::default();
        for i in 0..20 {
            s.touch_recent(&i.to_string());
        }
        assert_eq!(s.recent.len(), RECENT_MAX);
        assert_eq!(s.recent[0], "19");
    }
}
