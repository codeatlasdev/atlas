//! `~/.atlas/desktop/workspace.json`: worktrees, tabs, pane layouts, unread,
//! diff comments, agent feed. The frontend owns the schema (`version` field);
//! Rust only stores the document atomically, and never lets a bad file keep the app from opening: a file that is
//! not a JSON object is set aside beside the original (`workspace.json.corrupt-<unix_ms>`, the one convention of
//! `atlas_host::paths::set_aside_corrupt`) and the app starts from an empty workspace. The frontend says what happened.

use std::path::{Path, PathBuf};

use atlas_host::paths::{Loaded, read_json_or_aside, set_aside_corrupt};
use serde::Serialize;
use serde_json::Value;

pub fn path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".atlas/desktop/workspace.json"))
        .ok_or_else(|| "cannot locate home directory".to_string())
}

/// What the page gets from `workspace_load`: the saved document (`null`: start empty) and, when the file on disk
/// could not be used, what became of it (`lib/stores/workspacePersist.svelte.ts` reads these keys).
#[derive(Debug, Serialize, PartialEq)]
pub struct WorkspaceLoad {
    pub doc: Value,
    pub recovered: Option<Recovered>,
}

/// A workspace file that could not be used. `kept` is where its bytes now are; `None` means it could not be moved
/// and is still at `path` (an I/O error, a locked file): the page then starts empty and must not save over it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Recovered {
    pub path: String,
    pub kept: Option<String>,
    pub error: Option<String>,
}

impl Recovered {
    fn kept(path: &Path, aside: &Path) -> Self {
        Self {
            path: path.display().to_string(),
            kept: Some(aside.display().to_string()),
            error: None,
        }
    }

    fn stuck(path: &Path, error: impl std::fmt::Display) -> Self {
        Self {
            path: path.display().to_string(),
            kept: None,
            error: Some(error.to_string()),
        }
    }
}

/// Missing file or JSON `null`: no document, nothing to report. A JSON object: the document. Anything else (not
/// JSON, empty, an array, a number) is not a workspace: moved aside, reported, never an error.
fn load_at(p: &Path) -> WorkspaceLoad {
    let aside = |why: &dyn std::fmt::Display| match set_aside_corrupt(p) {
        Ok(kept) => Some(Recovered::kept(p, &kept)),
        Err(e) => Some(Recovered::stuck(
            p,
            format!("{why}; cannot move it aside: {e}"),
        )),
    };
    match read_json_or_aside::<Value>(p) {
        Ok(Loaded::Missing) => none(),
        Ok(Loaded::Ok(Value::Null)) => none(),
        Ok(Loaded::Ok(doc @ Value::Object(_))) => WorkspaceLoad {
            doc,
            recovered: None,
        },
        Ok(Loaded::Ok(other)) => WorkspaceLoad {
            doc: Value::Null,
            recovered: aside(&format!("expected a JSON object, found {}", kind(&other))),
        },
        Ok(Loaded::Corrupt(kept)) => WorkspaceLoad {
            doc: Value::Null,
            recovered: Some(Recovered::kept(p, &kept)),
        },
        Err(e) => WorkspaceLoad {
            doc: Value::Null,
            recovered: Some(Recovered::stuck(p, e)),
        },
    }
}

fn none() -> WorkspaceLoad {
    WorkspaceLoad {
        doc: Value::Null,
        recovered: None,
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[tauri::command]
pub fn workspace_load() -> WorkspaceLoad {
    match path() {
        Ok(p) => load_at(&p),
        Err(e) => WorkspaceLoad {
            doc: Value::Null,
            recovered: Some(Recovered::stuck(Path::new("workspace.json"), e)),
        },
    }
}

/// The page found the file valid JSON but not a workspace (its schema is the page's): set it aside like
/// [`workspace_load`] does for the unparseable ones.
#[tauri::command]
pub fn workspace_set_aside() -> Recovered {
    match path() {
        Ok(p) => match set_aside_corrupt(&p) {
            Ok(kept) => Recovered::kept(&p, &kept),
            Err(e) => Recovered::stuck(&p, e),
        },
        Err(e) => Recovered::stuck(Path::new("workspace.json"), e),
    }
}

#[tauri::command]
pub fn workspace_save(doc: Value) -> Result<(), String> {
    save_at(&path()?, &doc)
}

/// `atlas_core::fs::write_atomic` (temp file, flush, rename): a crash or a full disk leaves the old file whole.
fn save_at(p: &Path, doc: &Value) -> Result<(), String> {
    let text = serde_json::to_string(doc).map_err(|e| e.to_string())?;
    crate::registry::write_atomic(p, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(content: Option<&[u8]>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("desktop/workspace.json");
        if let Some(c) = content {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, c).unwrap();
        }
        (dir, p)
    }

    fn names(p: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(p.parent().unwrap())
            .map(|d| {
                d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    // Incident: `workspace_load` returned an error for a file that was not valid JSON, so the page
    // never set `ws.loaded`, the ready gate never opened and the app sat on its loading state for good. A crash
    // mid-write on an old build, a full disk or a hand edit was enough.
    #[test]
    fn a_file_that_is_not_a_workspace_is_set_aside_and_the_app_starts_empty() {
        for (what, bytes) in [
            (
                "truncated JSON",
                &br#"{"version":1,"worktrees":[{"id":"#[..],
            ),
            ("an empty file", &b""[..]),
            ("only whitespace", &b" \n"[..]),
            ("an array", &b"[1,2]"[..]),
            ("a number", &b"42"[..]),
            ("text", &b"not json at all"[..]),
        ] {
            let (_dir, p) = file(Some(bytes));
            let got = load_at(&p);
            assert_eq!(got.doc, Value::Null, "{what}");
            let r = got
                .recovered
                .unwrap_or_else(|| panic!("{what}: nothing reported"));
            assert_eq!(r.error, None, "{what}");
            let kept = PathBuf::from(r.kept.unwrap_or_else(|| panic!("{what}: not kept")));
            assert!(
                kept.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("workspace.json.corrupt-"),
                "{what}: {kept:?}"
            );
            assert_eq!(
                std::fs::read(&kept).unwrap(),
                bytes,
                "{what}: the user's bytes are kept"
            );
            assert!(!p.exists(), "{what}: the path is free for the next save");
        }
    }

    #[test]
    fn a_missing_file_and_a_null_are_a_first_run_not_an_incident() {
        let (_dir, p) = file(None);
        assert_eq!(load_at(&p), none());
        assert!(
            !p.exists() && !p.parent().unwrap().exists(),
            "reading creates nothing"
        );
        let (_dir, p) = file(Some(b"null"));
        assert_eq!(load_at(&p), none());
        assert_eq!(names(&p), ["workspace.json"], "a null is left alone");
    }

    #[test]
    fn a_valid_document_is_returned_untouched() {
        let doc = json!({"version": 1, "worktrees": [], "selected": null});
        let (_dir, p) = file(Some(doc.to_string().as_bytes()));
        let got = load_at(&p);
        assert_eq!(
            got,
            WorkspaceLoad {
                doc,
                recovered: None
            }
        );
        assert_eq!(names(&p), ["workspace.json"]);
    }

    // A file that cannot be read (not "does not parse") is not the user's data being wrong: it stays where it is and
    // the page is told not to save over it.
    #[test]
    fn a_file_that_cannot_be_read_stays_put_and_is_reported_as_not_kept() {
        let (_dir, p) = file(None);
        std::fs::create_dir_all(&p).unwrap(); // a directory where the file should be
        let got = load_at(&p);
        assert_eq!(got.doc, Value::Null);
        let r = got.recovered.unwrap();
        assert_eq!(r.kept, None);
        assert!(r.error.is_some());
        assert!(p.is_dir());
    }

    // The page and `Recovered` agree on these keys (the same literal is the mock of workspacePersist.test.ts).
    #[test]
    fn the_page_reads_doc_recovered_path_kept_and_error() {
        let (_dir, p) = file(Some(b"{"));
        let v = serde_json::to_value(load_at(&p)).unwrap();
        let r = &v["recovered"];
        assert!(v["doc"].is_null());
        assert_eq!(r["path"], p.display().to_string());
        assert!(
            r["kept"]
                .as_str()
                .unwrap()
                .contains("workspace.json.corrupt-")
        );
        assert!(r["error"].is_null());
        let v = serde_json::to_value(none()).unwrap();
        assert_eq!(v, json!({"doc": null, "recovered": null}));
        let v = serde_json::to_value(Recovered::stuck(Path::new("/w"), "boom")).unwrap();
        assert_eq!(v, json!({"path": "/w", "kept": null, "error": "boom"}));
    }

    #[test]
    fn set_aside_command_moves_the_file_and_names_where() {
        let (_dir, p) = file(Some(br#"{"tabs":"x"}"#));
        let aside = set_aside_corrupt(&p).unwrap();
        let r = Recovered::kept(&p, &aside);
        assert_eq!(std::fs::read(r.kept.unwrap()).unwrap(), br#"{"tabs":"x"}"#);
        assert!(!p.exists());
    }

    // The writer side: a save is a temp file renamed over the target, so a crash mid-write cannot leave the half
    // file that used to brick the app; a failed save leaves the previous file and no temp behind.
    #[test]
    fn saving_replaces_the_file_whole_and_leaves_no_temp_file() {
        let (_dir, p) = file(Some(br#"{"old":true}"#));
        save_at(&p, &json!({"version": 1, "worktrees": []})).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&p).unwrap()).unwrap(),
            json!({"version": 1, "worktrees": []})
        );
        assert_eq!(names(&p), ["workspace.json"]);
        let (_dir, p) = file(None);
        std::fs::create_dir_all(&p).unwrap(); // the target is a directory: the rename fails
        assert!(save_at(&p, &json!({})).is_err());
        assert_eq!(names(&p), ["workspace.json"], "no temp file left behind");
    }
}
