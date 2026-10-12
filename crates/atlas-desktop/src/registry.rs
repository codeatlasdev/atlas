//! `~/.atlas/desktop/projects.json`: the projects shown in the sidebar.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub root: String,
    #[serde(default)]
    pub pinned: bool,
    pub added: String,
    /// Linked git worktree: id of the main checkout's entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// `ATLAS_PORT_OFFSET` for this checkout's stack (0 = main ports).
    #[serde(default)]
    pub port_offset: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// `ssh:<alias>` for checkouts on an SSH host; `None` = this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// User-chosen icon and color of a project (repos only); `None` = automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
}

/// How a project looks in the app. Both parts are optional: no icon = detect the repo's logo,
/// else a monogram; no hue = the deterministic per-id one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    /// Accent hue, 0–359.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Icon {
    Emoji(String),
    /// Lucide icon name (`rocket`).
    Lucide(String),
    /// Image path relative to the project root.
    Image(String),
}

impl Identity {
    /// Reject values the UI could not render; an all-empty identity means "automatic".
    pub fn validated(mut self) -> Result<Option<Identity>, String> {
        if let Some(h) = self.hue
            && h > 359
        {
            return Err(format!("hue {h} out of range 0-359"));
        }
        match &self.icon {
            Some(Icon::Emoji(v)) if v.trim().is_empty() || v.chars().count() > 16 => {
                return Err("emoji must be 1-16 characters".into());
            }
            Some(Icon::Lucide(v))
                if v.is_empty()
                    || !v
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') =>
            {
                return Err(format!("invalid icon name {v:?}"));
            }
            Some(Icon::Image(v))
                if v.is_empty() || v.starts_with('/') || v.split('/').any(|seg| seg == "..") =>
            {
                return Err(format!("image must be a path inside the project: {v:?}"));
            }
            _ => {}
        }
        if self.icon.is_none() && self.hue.is_none() {
            return Ok(None);
        }
        if let Some(Icon::Emoji(v)) = &mut self.icon {
            *v = v.trim().to_string();
        }
        Ok(Some(self))
    }
}

impl Entry {
    pub fn host(&self) -> &str {
        self.host.as_deref().unwrap_or(crate::host::LOCAL)
    }
    pub fn is_remote(&self) -> bool {
        self.host.is_some()
    }
}

/// Id of a checkout on `host` (`None` = local; local ids are unchanged).
pub fn id_for_host(host: Option<&str>, root: &Path) -> String {
    match host {
        None => id_for(root),
        Some(h) => id_for(&Path::new(h).join(root.strip_prefix("/").unwrap_or(root))),
    }
}

pub fn id_for(root: &Path) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(root.to_string_lossy().as_bytes());
    h.digest().to_string()[..12].to_string()
}

fn path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".atlas/desktop/projects.json"))
        .ok_or_else(|| "cannot locate home directory".to_string())
}

/// Missing file = empty registry; a corrupt one is an error (never overwrite it silently).
pub fn load() -> Result<Vec<Entry>, String> {
    let p = path()?;
    match std::fs::read_to_string(&p) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

pub fn save(entries: &[Entry]) -> Result<(), String> {
    let p = path()?;
    let text = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    write_atomic(&p, &text)
}

/// tmp + rename, so a crash never leaves a half-written file.
pub fn write_atomic(p: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = p.with_extension("tmp~");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, p).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(identity: Option<Identity>) -> Entry {
        Entry {
            id: "abc".into(),
            root: "/r".into(),
            pinned: false,
            added: "now".into(),
            parent: None,
            port_offset: 0,
            branch: None,
            host: None,
            identity,
        }
    }

    #[test]
    fn identity_round_trips_through_json() {
        let id = Identity {
            icon: Some(Icon::Lucide("rocket".into())),
            hue: Some(200),
        };
        let text = serde_json::to_string(&[entry(Some(id.clone()))]).unwrap();
        assert!(
            text.contains(r#""identity":{"icon":{"kind":"lucide","value":"rocket"},"hue":200}"#)
        );
        let back: Vec<Entry> = serde_json::from_str(&text).unwrap();
        assert_eq!(back[0].identity, Some(id));
    }

    #[test]
    fn old_registries_without_identity_still_load() {
        let text = r#"[{"id":"a","root":"/r","pinned":true,"added":"x"}]"#;
        let back: Vec<Entry> = serde_json::from_str(text).unwrap();
        assert_eq!(back[0].identity, None);
        assert!(!serde_json::to_string(&back).unwrap().contains("identity"));
    }

    #[test]
    fn validation_accepts_each_icon_kind_and_clears_empty() {
        let ok = |icon, hue| Identity { icon, hue }.validated();
        assert_eq!(ok(None, None), Ok(None));
        assert!(
            ok(Some(Icon::Emoji(" 🚀 ".into())), None)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            ok(Some(Icon::Emoji(" 🚀 ".into())), None)
                .unwrap()
                .unwrap()
                .icon,
            Some(Icon::Emoji("🚀".into()))
        );
        assert!(
            ok(Some(Icon::Lucide("git-branch".into())), Some(359))
                .unwrap()
                .is_some()
        );
        assert!(
            ok(Some(Icon::Image("public/logo.svg".into())), None)
                .unwrap()
                .is_some()
        );
        assert!(ok(None, Some(10)).unwrap().is_some());
    }

    #[test]
    fn validation_rejects_unrenderable_values() {
        let bad = |icon, hue| Identity { icon, hue }.validated().is_err();
        assert!(bad(None, Some(360)));
        assert!(bad(Some(Icon::Emoji("  ".into())), None));
        assert!(bad(Some(Icon::Emoji("x".repeat(17))), None));
        assert!(bad(Some(Icon::Lucide("Rocket".into())), None));
        assert!(bad(Some(Icon::Lucide(String::new())), None));
        assert!(bad(Some(Icon::Image("/etc/passwd".into())), None));
        assert!(bad(Some(Icon::Image("../x.png".into())), None));
        assert!(bad(Some(Icon::Image("a/../../x.png".into())), None));
    }
}
