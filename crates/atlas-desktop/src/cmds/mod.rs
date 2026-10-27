//! `#[tauri::command]`s. Every error is a human-readable string (shown in a toast).
//!
//! By concern: `projects` (registry list/add/remove/pin/identity/discover), `stack`
//! (start/stop, services, actions, the config overview), `setup` (dependency install),
//! `services` (what a service is and how to open it), `config_edit` (atlas.yaml read /
//! validate / write) and `system` (open in editor / browser / Finder). This file holds the
//! registry lookups they share.

mod config_edit;
mod discover;
mod projects;
mod services;
mod setup;
mod stack;
mod system;
mod theme_pack;
mod ui_sounds;

use std::path::PathBuf;

use atlas_tui::config;
use tauri::{AppHandle, Emitter};

use crate::registry::{self, Entry};

pub use config_edit::*;
pub use projects::*;
pub use setup::*;
pub use stack::*;
pub use system::*;
pub use theme_pack::*;
pub use ui_sounds::*;

pub(crate) use stack::atlas_bin;

fn find(id: &str) -> Result<Entry, String> {
    registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown project {id}"))
}

/// Root of a checkout on this machine. SSH checkouts are refused here: commands
/// that read local files or talk to a local runner socket don't apply to them.
fn root_of(id: &str) -> Result<PathBuf, String> {
    let e = find(id)?;
    if e.is_remote() {
        return Err(REMOTE_UNSUPPORTED.into());
    }
    Ok(PathBuf::from(e.root))
}

const REMOTE_UNSUPPORTED: &str =
    "not available for SSH checkouts yet (use a terminal in the worktree)";

/// Config of an entry (its `atlas.yaml`, or the one detected in memory when the folder has none), ports shifted by its
/// worktree offset.
fn load_for(id: &str) -> Result<(PathBuf, config::ProjectConfig), String> {
    let e = find(id)?;
    if e.is_remote() {
        return Err(REMOTE_UNSUPPORTED.into());
    }
    let root = PathBuf::from(&e.root);
    let (mut cfg, _) = config::load_or_detect(&root).map_err(|e| format!("{e:#}"))?;
    if e.port_offset > 0 {
        config::apply_port_offset(&mut cfg, e.port_offset);
    }
    Ok((root, cfg))
}

/// Tell the frontend the project list (or a project's state) changed.
fn notify_changed(app: &AppHandle) {
    let _ = app.emit("projects://changed", ());
}

/// Load the registry, apply `f` to the entry `id`, save, and notify the frontend.
fn update_entry(
    app: &AppHandle,
    id: &str,
    f: impl FnOnce(&mut Entry) -> Result<(), String>,
) -> Result<(), String> {
    let mut entries = registry::load()?;
    let e = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(format!("unknown project {id}"))?;
    f(e)?;
    registry::save(&entries)?;
    notify_changed(app);
    Ok(())
}
