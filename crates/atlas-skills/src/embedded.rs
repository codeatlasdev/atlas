//! Skills compiled into the binary (see `build.rs`), plus install/status logic
//! that writes them into an agent skills directory.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::path::{Path, PathBuf};

use crate::manifest::{InstalledSkill, SkillsManifest};

include!(concat!(env!("OUT_DIR"), "/embedded_skills.rs"));

/// One embedded file of a skill package (`SKILL.md` or a sibling `.md`).
#[derive(Debug, Clone, Copy)]
pub struct SkillFile {
    /// Path relative to the skill directory, `/`-separated.
    pub path: &'static str,
    pub content: &'static str,
}

/// A skill package compiled into the binary.
#[derive(Debug, Clone)]
pub struct EmbeddedSkill {
    pub name: String,
    pub version: String,
    pub description: String,
    /// `SKILL.md` first, then siblings sorted by path.
    pub files: Vec<SkillFile>,
}

/// How an installed skill compares with the embedded copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillState {
    /// Not on disk (or a file is missing).
    Missing,
    /// All files present and byte-identical.
    UpToDate,
    /// Present but at least one file differs.
    Outdated,
}

/// All embedded skills, sorted by name.
pub fn all() -> Vec<EmbeddedSkill> {
    let mut out: Vec<EmbeddedSkill> = Vec::new();
    for (skill, rel, content) in EMBEDDED_FILES {
        let file = SkillFile { path: rel, content };
        match out.iter_mut().find(|s| s.name == *skill) {
            Some(s) => s.files.push(file),
            None => out.push(EmbeddedSkill {
                name: (*skill).to_string(),
                version: String::new(),
                description: String::new(),
                files: vec![file],
            }),
        }
    }
    for s in &mut out {
        s.files
            .sort_by(|a, b| (a.path != "SKILL.md", a.path).cmp(&(b.path != "SKILL.md", b.path)));
        if let Some(md) = s.files.iter().find(|f| f.path == "SKILL.md")
            && let Ok(fm) = parse_frontmatter(md.content)
            && fm.name == s.name
        {
            s.version = fm.version;
            s.description = fm.description;
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Look up one embedded skill by name.
pub fn get(name: &str) -> Option<EmbeddedSkill> {
    all().into_iter().find(|s| s.name == name)
}

impl EmbeddedSkill {
    /// Compare this skill with what is on disk under `dir/<name>/`.
    pub fn state_in(&self, dir: &Path) -> SkillState {
        let base = dir.join(&self.name);
        let mut any_diff = false;
        for f in &self.files {
            match std::fs::read_to_string(base.join(f.path)) {
                Ok(on_disk) if on_disk == f.content => {}
                Ok(_) => any_diff = true,
                Err(_) => return SkillState::Missing,
            }
        }
        if any_diff {
            SkillState::Outdated
        } else {
            SkillState::UpToDate
        }
    }

    /// Write every file of this skill under `dir/<name>/` (atomic per file).
    /// Removes a legacy flat `<name>.md` left by pre-package atlas versions.
    pub fn write_to(&self, dir: &Path) -> Result<()> {
        let legacy = dir.join(format!("{}.md", self.name));
        if legacy.is_file() {
            let _ = std::fs::remove_file(&legacy);
        }
        let base = dir.join(&self.name);
        for f in &self.files {
            write_atomic(&base.join(f.path), f.content)?;
        }
        Ok(())
    }

    fn to_manifest_entry(&self) -> InstalledSkill {
        InstalledSkill {
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            installed_at: Utc::now(),
            file: format!("{}/SKILL.md", self.name),
            files: self.files.iter().map(|f| f.path.to_string()).collect(),
        }
    }
}

/// Result of installing one skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed,
    Updated,
    UpToDate,
}

/// Install (or refresh) embedded skills into `dir`, recording them in the
/// manifest at `manifest_path`. `names` empty = all. Only files whose content
/// differs are rewritten; the manifest is only saved when something changed.
pub fn install(
    dir: &Path,
    manifest_path: &Path,
    names: &[String],
) -> Result<Vec<(String, InstallOutcome)>> {
    let available = all();
    let selected: Vec<&EmbeddedSkill> = if names.is_empty() {
        available.iter().collect()
    } else {
        let mut v = Vec::new();
        for n in names {
            match available.iter().find(|s| &s.name == n) {
                Some(s) => v.push(s),
                None => bail!(
                    "unknown skill '{n}' (available: {})",
                    available
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        v
    };

    let mut manifest = SkillsManifest::load_from(manifest_path)?;
    let mut results = Vec::new();
    let mut dirty = false;
    for s in selected {
        let state = s.state_in(dir);
        let tracked = manifest.get(&s.name).is_some();
        let outcome = match state {
            SkillState::UpToDate => InstallOutcome::UpToDate,
            SkillState::Missing => InstallOutcome::Installed,
            SkillState::Outdated => InstallOutcome::Updated,
        };
        if outcome != InstallOutcome::UpToDate {
            s.write_to(dir)?;
        }
        if outcome != InstallOutcome::UpToDate || !tracked {
            manifest.upsert(s.to_manifest_entry());
            dirty = true;
        }
        results.push((s.name.clone(), outcome));
    }
    if dirty {
        manifest.save_to(manifest_path)?;
    }
    Ok(results)
}

/// Remove skills recorded in the manifest that atlas embeds, from `dir`.
/// Returns the removed skill names. Skills not in the manifest are never touched.
pub fn uninstall(dir: &Path, manifest_path: &Path) -> Result<Vec<String>> {
    let mut manifest = SkillsManifest::load_from(manifest_path)?;
    let embedded = all();
    let mut removed = Vec::new();
    for s in &embedded {
        if manifest.get(&s.name).is_some() {
            let base = dir.join(&s.name);
            if base.is_dir() {
                for f in &s.files {
                    let _ = std::fs::remove_file(base.join(f.path));
                }
                // Leaves user-added files in place; rmdir only succeeds when empty.
                let _ = std::fs::remove_dir(&base);
            }
            removed.push(s.name.clone());
        }
    }
    manifest.skills.retain(|m| !removed.contains(&m.name));
    if !removed.is_empty() {
        manifest.save_to(manifest_path)?;
    }
    Ok(removed)
}

/// Atomic text write: tmp file in the same directory + rename.
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().context("path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let tmp: PathBuf = parent.join(format!(
        ".{}.atlas-tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

#[derive(Debug)]
struct Frontmatter {
    name: String,
    /// `metadata.version`, falling back to the atlas crate version.
    version: String,
    description: String,
    /// Top-level keys in order of appearance (used by the spec test).
    #[cfg_attr(not(test), allow(dead_code))]
    keys: Vec<String>,
}

/// Minimal line-based parser for the agentskills.io frontmatter subset atlas uses:
/// single-line top-level `key: value` scalars plus an optional `metadata:` map with
/// indented `version: "x"`. Unknown keys are ignored.
fn parse_frontmatter(content: &str) -> Result<Frontmatter> {
    let content = content.trim_start();
    let Some(rest) = content.strip_prefix("---") else {
        bail!("skill file has no YAML frontmatter");
    };
    let end = rest.find("\n---").context("unclosed frontmatter block")?;
    let unq = |v: &str| v.trim().trim_matches('"').to_string();
    let (mut name, mut version, mut description) = (String::new(), String::new(), String::new());
    let mut keys = Vec::new();
    let mut in_metadata = false;
    for line in rest[..end].lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indented = line.starts_with(char::is_whitespace);
        if !indented {
            let key = line.split(':').next().unwrap_or("").trim().to_string();
            in_metadata = key == "metadata";
            keys.push(key);
            if let Some(v) = line.strip_prefix("name:") {
                name = unq(v);
            } else if let Some(v) = line.strip_prefix("description:") {
                description = unq(v);
            }
        } else if in_metadata && let Some(v) = line.trim_start().strip_prefix("version:") {
            version = unq(v);
        }
    }
    if name.is_empty() {
        bail!("frontmatter missing 'name' field");
    }
    if version.is_empty() {
        version = env!("CARGO_PKG_VERSION").to_string();
    }
    Ok(Frontmatter {
        name,
        version,
        description,
        keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skills_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills")
    }

    fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                md_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }

    #[test]
    fn every_file_under_skills_is_embedded() {
        let root = skills_root();
        let mut on_disk = Vec::new();
        md_files(&root, &mut on_disk);
        assert!(!on_disk.is_empty(), "no skills found under {root:?}");
        for p in &on_disk {
            let rel = p.strip_prefix(&root).unwrap();
            let mut comps = rel.components();
            let skill = comps.next().unwrap().as_os_str().to_string_lossy();
            let inner: Vec<String> = comps
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            assert!(
                !inner.is_empty(),
                "{rel:?}: flat skills/*.md are unsupported; use skills/<name>/SKILL.md"
            );
            let inner = inner.join("/");
            let hit = EMBEDDED_FILES
                .iter()
                .find(|(s, f, _)| *s == skill && *f == inner)
                .unwrap_or_else(|| panic!("{rel:?} is not embedded"));
            assert_eq!(
                hit.2,
                std::fs::read_to_string(p).unwrap(),
                "{rel:?} embedded content differs"
            );
        }
        assert_eq!(
            EMBEDDED_FILES.len(),
            on_disk.len(),
            "embedded table has entries not on disk"
        );
    }

    #[test]
    fn every_skill_has_valid_frontmatter() {
        const ALLOWED: &[&str] = &[
            "name",
            "description",
            "license",
            "compatibility",
            "metadata",
            "allowed-tools",
        ];
        let skills = all();
        assert!(!skills.is_empty());
        for s in &skills {
            let md = s
                .files
                .iter()
                .find(|f| f.path == "SKILL.md")
                .unwrap_or_else(|| panic!("{} has no SKILL.md", s.name));
            let fm = parse_frontmatter(md.content).unwrap_or_else(|e| panic!("{}: {e}", s.name));
            assert_eq!(fm.name, s.name, "frontmatter name must equal directory");
            let valid_name = fm.name.len() <= 64
                && fm.name.split('-').all(|p| {
                    !p.is_empty()
                        && p.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                });
            assert!(valid_name, "{}: invalid skill name", s.name);
            assert!(!fm.description.is_empty(), "{} lacks description", s.name);
            assert!(
                fm.description.chars().count() <= 1024,
                "{} description > 1024 chars",
                s.name
            );
            assert!(
                !["", ">", "|", ">-", "|-"].contains(&fm.description.as_str()),
                "{} description must be on one line",
                s.name
            );
            for k in &fm.keys {
                assert!(
                    ALLOWED.contains(&k.as_str()),
                    "{}: frontmatter key '{k}' is not in the agentskills.io spec",
                    s.name
                );
            }
            assert!(!s.version.is_empty());

            // Relative .md links in SKILL.md must resolve to embedded files.
            let mut rest = md.content;
            while let Some(i) = rest.find("](") {
                rest = &rest[i + 2..];
                let Some(j) = rest.find(')') else { break };
                let target = rest[..j].split('#').next().unwrap_or("");
                if target.ends_with(".md") && !target.contains("://") && !target.starts_with('/') {
                    let target = target.trim_start_matches("./");
                    assert!(
                        s.files.iter().any(|f| f.path == target),
                        "{}: SKILL.md links to missing file {target}",
                        s.name
                    );
                }
            }
        }
    }

    #[test]
    fn install_is_idempotent_and_detects_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("skills");
        let mpath = tmp.path().join("m.json");
        let first = install(&dir, &mpath, &[]).unwrap();
        assert!(first.iter().all(|(_, o)| *o == InstallOutcome::Installed));
        let second = install(&dir, &mpath, &[]).unwrap();
        assert!(second.iter().all(|(_, o)| *o == InstallOutcome::UpToDate));

        let victim = all().remove(0);
        std::fs::write(dir.join(&victim.name).join("SKILL.md"), "stale").unwrap();
        assert_eq!(victim.state_in(&dir), SkillState::Outdated);
        let third = install(&dir, &mpath, std::slice::from_ref(&victim.name)).unwrap();
        assert_eq!(third[0].1, InstallOutcome::Updated);
        assert_eq!(victim.state_in(&dir), SkillState::UpToDate);
    }

    #[test]
    fn install_unknown_name_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let err = install(
            &tmp.path().join("s"),
            &tmp.path().join("m.json"),
            &["nope".to_string()],
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown skill"));
    }

    #[test]
    fn uninstall_only_removes_tracked_atlas_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("skills");
        let mpath = tmp.path().join("m.json");
        install(&dir, &mpath, &[]).unwrap();
        // A foreign skill and a user file inside an atlas skill dir.
        std::fs::create_dir_all(dir.join("foreign")).unwrap();
        std::fs::write(dir.join("foreign/SKILL.md"), "mine").unwrap();
        let first = all().remove(0);
        std::fs::write(dir.join(&first.name).join("notes.txt"), "keep").unwrap();

        let removed = uninstall(&dir, &mpath).unwrap();
        assert_eq!(removed.len(), all().len());
        assert!(dir.join("foreign/SKILL.md").exists());
        assert!(dir.join(&first.name).join("notes.txt").exists());
        assert!(!dir.join(&first.name).join("SKILL.md").exists());
        assert!(SkillsManifest::load_from(&mpath).unwrap().skills.is_empty());
    }
}
