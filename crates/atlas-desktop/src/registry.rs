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
    load_at(&path()?)
}

pub fn load_at(p: &Path) -> Result<Vec<Entry>, String> {
    match std::fs::read_to_string(p) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// One registry edit at a time: load, `f`, save, under one lock. Two edits in flight (the deck adding folders while
/// a pin or `projects_list` runs) used to start from the same file and the later save dropped the earlier entry.
/// An error from `f` (or an unreadable registry) writes nothing. Returns whatever `f` returns.
pub fn edit<T>(f: impl FnOnce(&mut Vec<Entry>) -> Result<T, String>) -> Result<T, String> {
    edit_at(&path()?, f)
}

static EDIT_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn edit_at<T>(
    p: &Path,
    f: impl FnOnce(&mut Vec<Entry>) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = EDIT_LOCK.lock();
    let mut entries = load_at(p)?;
    let before = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
    let out = f(&mut entries)?;
    let text = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
    if text == before && p.exists() {
        return Ok(out);
    }
    write_atomic(p, &text)?;
    Ok(out)
}

/// tmp + rename, so a crash never leaves a half-written file. The temp name is unique per call: writers sharing
/// one name interleaved their bytes or lost the rename to each other. A failed write removes its temp file.
pub fn write_atomic(p: &Path, text: &str) -> Result<(), String> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut name = p.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.{seq}.tmp~", std::process::id()));
    let tmp = p.with_file_name(name);
    let result = std::fs::write(&tmp, text)
        .map_err(|e| format!("{}: {e}", tmp.display()))
        .and_then(|()| std::fs::rename(&tmp, p).map_err(|e| format!("{}: {e}", p.display())));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// What Atlas last saw of a checkout while it still existed. A deleted or moved project has no
/// folder left to read, so this is what lets discovery recognize its new location.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Known {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

fn known_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".atlas/desktop/known.json"))
        .ok_or_else(|| "cannot locate home directory".to_string())
}

/// Missing or unreadable file = nothing known (it is only a hint, never authoritative).
pub fn load_known() -> std::collections::HashMap<String, Known> {
    known_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_known(map: &std::collections::HashMap<String, Known>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    write_atomic(&known_path()?, &text)
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

    fn plain(id: &str) -> Entry {
        Entry {
            id: id.into(),
            ..entry(None)
        }
    }

    /// Incident class: every writer used the same `projects.json.tmp~`, so two saves in flight wrote into one temp
    /// file and renamed an interleaved or already-moved file (ENOENT on the second rename, or mixed bytes).
    #[test]
    fn concurrent_atomic_writes_leave_one_complete_file_and_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let p = std::sync::Arc::new(dir.path().join("f.json"));
        let texts: Vec<String> = (0..16).map(|i| format!("{i}").repeat(200_000)).collect();
        let threads: Vec<_> = texts
            .iter()
            .cloned()
            .map(|t| {
                let p = p.clone();
                std::thread::spawn(move || write_atomic(&p, &t))
            })
            .collect();
        for t in threads {
            t.join().unwrap().unwrap();
        }
        let got = std::fs::read_to_string(&*p).unwrap();
        assert!(texts.contains(&got), "file is not any single writer's text");
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["f.json"], "temp files left behind");
    }

    /// A failed write (target is a directory) must not leave its temp file behind either.
    #[test]
    fn a_failed_atomic_write_removes_its_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("target");
        std::fs::create_dir(&p).unwrap();
        assert!(write_atomic(&p, "x").is_err());
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["target"]);
    }

    /// Incident class: `projects_add`, pin, identity, worktree create/remove all did load → change → save with no
    /// lock; two in flight (the first-run deck adds several folders while `projects_list` and a pin run) lost one
    /// entry. `edit_at` holds the lock across the whole read-modify-write.
    #[test]
    fn concurrent_edits_never_lose_an_entry() {
        let dir = tempfile::tempdir().unwrap();
        let p = std::sync::Arc::new(dir.path().join("desktop/projects.json"));
        let threads: Vec<_> = (0..16)
            .map(|i| {
                let p = p.clone();
                std::thread::spawn(move || {
                    edit_at(&p, |entries| {
                        entries.push(plain(&format!("id{i}")));
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let mut ids: Vec<_> = load_at(&p).unwrap().into_iter().map(|e| e.id).collect();
        ids.sort();
        let mut want: Vec<_> = (0..16).map(|i| format!("id{i}")).collect();
        want.sort();
        assert_eq!(ids, want);
    }

    /// A closure error aborts the edit: nothing is written, the file is as it was.
    #[test]
    fn a_failed_edit_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("projects.json");
        edit_at(&p, |e| {
            e.push(plain("a"));
            Ok(())
        })
        .unwrap();
        let before = std::fs::read_to_string(&p).unwrap();
        let r: Result<(), String> = edit_at(&p, |e| {
            e.clear();
            Err("nope".into())
        });
        assert_eq!(r.unwrap_err(), "nope");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), before);
    }

    /// A corrupt registry stays an error for readers AND editors (the existing "never overwrite it silently"
    /// rule), and the file is untouched.
    #[test]
    fn a_corrupt_registry_is_an_error_and_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("projects.json");
        std::fs::write(&p, r#"[{"id":"a","root""#).unwrap();
        assert!(load_at(&p).unwrap_err().contains("projects.json"));
        assert!(
            edit_at(&p, |e| {
                e.push(plain("b"));
                Ok(())
            })
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&p).unwrap(), r#"[{"id":"a","root""#);
    }

    /// An edit that changes nothing (adding a folder already registered, removing no missing project) must not
    /// rewrite the file: every needless rewrite is another chance for a crash to catch it mid-write.
    #[test]
    fn an_edit_that_changes_nothing_does_not_rewrite_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("projects.json");
        edit_at(&p, |e| {
            e.push(plain("a"));
            Ok(())
        })
        .unwrap();
        // a hand-formatted (compact) file stays as the user left it when nothing changes
        std::fs::write(&p, serde_json::to_string(&load_at(&p).unwrap()).unwrap()).unwrap();
        let compact = std::fs::read_to_string(&p).unwrap();
        edit_at(&p, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), compact);
    }

    /// First write creates the file (and its folder) even when the closure adds nothing: `p.exists()` is part of
    /// the no-op rule, so a missing registry is not "unchanged".
    #[test]
    fn the_first_edit_creates_a_missing_registry() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("fresh/desktop/projects.json");
        edit_at(&p, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "[]");
    }
}
