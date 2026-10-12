use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Tracks installed skills for a given directory.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SkillsManifest {
    pub skills: Vec<InstalledSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed_at: DateTime<Utc>,
    /// Path relative to the manifest file (kept for backward compat with flat skills)
    pub file: String,
    /// All installed filenames relative to the skill's install directory.
    /// Flat skill: `["skill-name.md"]`. Package: `["SKILL.md", "other.md", …]`.
    #[serde(default)]
    pub files: Vec<String>,
}

impl SkillsManifest {
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_from(&Self::path(dir))
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        atlas_core::util::load_json_or_default(path)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        self.save_to(&Self::path(dir))
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn upsert(&mut self, skill: InstalledSkill) {
        if let Some(existing) = self.skills.iter_mut().find(|s| s.name == skill.name) {
            *existing = skill;
        } else {
            self.skills.push(skill);
        }
    }

    pub fn get(&self, name: &str) -> Option<&InstalledSkill> {
        self.skills.iter().find(|s| s.name == name)
    }

    pub fn path(dir: &Path) -> PathBuf {
        dir.join(".atlas-skills.json")
    }
}
