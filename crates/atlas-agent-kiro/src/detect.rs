#![allow(unused)]

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use atlas_agent::{ActivityDetector, ActivityState};

pub struct KiroActivityDetector {
    sessions_dir: PathBuf,
}

impl KiroActivityDetector {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            sessions_dir: PathBuf::from(home)
                .join(".kiro")
                .join("sessions")
                .join("cli"),
        }
    }

    fn check_file_mtime(&self) -> Option<ActivityState> {
        let entries = std::fs::read_dir(&self.sessions_dir).ok()?;
        let now = SystemTime::now();
        let threshold = Duration::from_secs(5);

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") {
                if let Ok(metadata) = path.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if now.duration_since(modified).unwrap_or(Duration::MAX) < threshold {
                            return Some(ActivityState::Active);
                        }
                    }
                }
            }
        }

        None
    }
}

impl Default for KiroActivityDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityDetector for KiroActivityDetector {
    fn detect(&self, output: &[u8]) -> Option<ActivityState> {
        let text = String::from_utf8_lossy(output);

        if text.contains("ask a question") || text.contains("What would you like") {
            return Some(ActivityState::Idle);
        }

        if text.contains("Kiro is working") || text.contains("Running") {
            return Some(ActivityState::Active);
        }

        if text.contains("waiting for approval") {
            return Some(ActivityState::WaitingInput);
        }

        // Fall back to file mtime detection
        self.check_file_mtime()
    }
}
