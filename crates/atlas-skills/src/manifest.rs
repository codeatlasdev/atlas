use anyhow::Result;
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
    #[serde(with = "atlas_core::time::utc_z_serde")]
    pub installed_at: jiff::Timestamp,
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

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../testdata/manifest.json");

    // Contract (chrono → jiff migration, #540): `.atlas-skills.json` is written by one build and read by the
    // next. A manifest written by today's producer must load and save back byte for byte: `installed_at` keeps
    // `Z` and exactly 0, 3, 6 or 9 fractional digits (chrono's `AutoSi`), never trimmed or padded.
    #[test]
    fn a_manifest_written_today_saves_back_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.json");
        std::fs::write(&path, FIXTURE).unwrap();
        let m = SkillsManifest::load_from(&path).unwrap();
        assert_eq!(m.skills.len(), 4);
        m.save_to(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), FIXTURE.trim_end());
    }

    // The same instant spelled the other RFC 3339 ways still loads, and is saved as the one `Z` form.
    #[test]
    fn other_rfc3339_spellings_load_and_are_saved_in_the_z_form() {
        let entry = |ts: &str| {
            format!(
                r#"{{"skills":[{{"name":"a","version":"1","description":"d","installed_at":"{ts}","file":"a/SKILL.md"}}]}}"#
            )
        };
        let saved = |ts: &str| {
            let m: SkillsManifest = serde_json::from_str(&entry(ts)).unwrap();
            serde_json::to_value(&m).unwrap()["skills"][0]["installed_at"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(saved("2026-11-14T09:05:07+00:00"), "2026-11-14T09:05:07Z");
        assert_eq!(saved("2026-11-14T06:05:07-03:00"), "2026-11-14T09:05:07Z");
        assert_eq!(
            saved("2026-11-14T12:05:07.5+03:00"),
            "2026-11-14T09:05:07.500Z"
        );
        assert_eq!(
            saved("2026-11-14T09:05:07.123456+00:00"),
            "2026-11-14T09:05:07.123456Z"
        );
        // `files` is optional in old manifests.
        let m: SkillsManifest = serde_json::from_str(&entry("2026-11-14T09:05:07Z")).unwrap();
        assert!(m.skills[0].files.is_empty());
    }

    #[test]
    fn a_timestamp_that_is_not_rfc3339_is_an_error_not_a_default() {
        for bad in [
            "",
            "yesterday",
            "2026-11-14",
            "2026-11-14T09:05:07",
            "1700000000",
        ] {
            let json = format!(
                r#"{{"skills":[{{"name":"a","version":"1","description":"d","installed_at":"{bad}","file":"f"}}]}}"#
            );
            assert!(
                serde_json::from_str::<SkillsManifest>(&json).is_err(),
                "{bad:?}"
            );
        }
    }
}
