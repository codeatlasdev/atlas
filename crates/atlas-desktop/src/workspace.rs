//! `~/.atlas/desktop/workspace.json`: worktrees, tabs, pane layouts, unread,
//! diff comments, agent feed. The frontend owns the schema (`version` field);
//! Rust only stores the document atomically.

use std::path::PathBuf;

use serde_json::Value;

pub fn path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join(".atlas/desktop/workspace.json"))
        .ok_or_else(|| "cannot locate home directory".to_string())
}

pub fn load_doc() -> Result<Value, String> {
    let p = path()?;
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

#[tauri::command]
pub fn workspace_load() -> Result<Value, String> {
    load_doc()
}

#[tauri::command]
pub fn workspace_save(doc: Value) -> Result<(), String> {
    let text = serde_json::to_string(&doc).map_err(|e| e.to_string())?;
    crate::registry::write_atomic(&path()?, &text)
}
