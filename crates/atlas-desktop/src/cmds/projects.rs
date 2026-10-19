//! The project registry commands: list, add, remove, pin, identity, relink and discovery.

use atlas_core::sys::CommandExt as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use atlas_tui::{config, status};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::discover::{self, Event, Found, ScanOpts};
use super::{notify_changed, root_of, update_entry};
use crate::live::LiveHub;
use crate::registry::{self, Entry};

#[derive(Serialize)]
pub struct ProjectView {
    id: String,
    root: String,
    name: String,
    pinned: bool,
    running: bool,
    mode: Option<String>,
    services_total: usize,
    services_healthy: usize,
    error: Option<String>,
    profiles: Vec<String>,
    parent: Option<String>,
    port_offset: u16,
    branch: Option<String>,
    /// `ssh:<alias>` for SSH checkouts.
    host: Option<String>,
    /// User-chosen icon and color (`None` = automatic).
    identity: Option<registry::Identity>,
    /// The checkout is gone from this machine (`None` = present, or a remote project).
    missing: Option<Missing>,
    /// `origin` URL last seen while the project existed (only reported when it is missing).
    last_remote: Option<String>,
}

/// Why a local project cannot be opened.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Missing {
    /// The folder itself no longer exists (deleted, moved, unmounted drive).
    Root,
    /// The folder is there but its `atlas.yaml` is not.
    Config,
}

/// Cheap stat check; never reads the config or touches a remote path.
fn missing_of(e: &Entry) -> Option<Missing> {
    if e.is_remote() {
        return None;
    }
    let root = Path::new(&e.root);
    if !root.is_dir() {
        Some(Missing::Root)
    } else if !root.join("atlas.yaml").is_file() {
        Some(Missing::Config)
    } else {
        None
    }
}

/// The part of a `ProjectView` that comes from reading the checkout (config + live runner).
#[derive(Default)]
struct Detail {
    name: Option<String>,
    error: Option<String>,
    profiles: Vec<String>,
    mode: Option<String>,
    running: bool,
    services_total: usize,
    services_healthy: usize,
}

fn read_local(root: &Path) -> Detail {
    let (name, error, total, profiles) = match config::load_checked(root) {
        Ok((cfg, errs)) => (
            Some(cfg.name.clone()),
            (!errs.is_empty()).then(|| config::render_errors(&errs, "atlas.yaml")),
            cfg.services.len(),
            cfg.profiles.keys().cloned().collect(),
        ),
        Err(err) => (None, Some(format!("{err:#}")), 0, Vec::new()),
    };
    let snap = status::live_snapshot(root).ok();
    Detail {
        name,
        error,
        profiles,
        mode: snap.as_ref().map(|s| s.mode.clone()),
        running: snap.is_some(),
        services_total: snap.as_ref().map_or(total, |s| s.services.len()),
        services_healthy: snap.as_ref().map_or(0, |s| {
            s.services.iter().filter(|r| r.state == "running").count()
        }),
    }
}

fn view(e: &Entry, known: Option<&registry::Known>) -> ProjectView {
    let root = Path::new(&e.root);
    let missing = missing_of(e);
    // Never read a remote path on this machine (details come via hostd) nor a missing one.
    let d = if e.is_remote() || missing.is_some() {
        Detail::default()
    } else {
        read_local(root)
    };
    // Without a checkout to read, the last known name beats the folder's.
    let known_name = known.filter(|_| missing.is_some()).map(|k| k.name.clone());
    let name = d.name.or(known_name).unwrap_or_else(|| {
        root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| e.root.clone())
    });
    ProjectView {
        id: e.id.clone(),
        root: e.root.clone(),
        name,
        pinned: e.pinned,
        running: d.running,
        mode: d.mode,
        services_total: d.services_total,
        services_healthy: d.services_healthy,
        error: d.error,
        profiles: d.profiles,
        parent: e.parent.clone(),
        port_offset: e.port_offset,
        branch: e.branch.clone(),
        host: e.host.clone(),
        identity: e.identity.clone(),
        last_remote: known
            .filter(|_| missing.is_some())
            .and_then(|k| k.remote.clone()),
        missing,
    }
}

#[tauri::command]
pub fn projects_list(app: AppHandle, hub: State<'_, LiveHub>) -> Result<Vec<ProjectView>, String> {
    let entries = registry::load()?;
    for e in entries
        .iter()
        .filter(|e| !e.is_remote() && missing_of(e).is_none())
    {
        hub.ensure(&app, &e.id, Path::new(&e.root));
    }
    let known = registry::load_known();
    let views: Vec<ProjectView> = entries.iter().map(|e| view(e, known.get(&e.id))).collect();
    remember(&entries, &views);
    Ok(views)
}

/// Update the last-known name/remote of every present local project (best effort).
fn remember(entries: &[Entry], views: &[ProjectView]) {
    let mut known = registry::load_known();
    let before = known.clone();
    for (e, v) in entries.iter().zip(views) {
        if e.is_remote() || v.missing.is_some() || e.parent.is_some() {
            continue;
        }
        known.insert(
            e.id.clone(),
            registry::Known {
                name: v.name.clone(),
                remote: discover::git_remote(Path::new(&e.root)),
            },
        );
    }
    known.retain(|id, _| entries.iter().any(|e| &e.id == id));
    if known != before {
        let _ = registry::save_known(&known);
    }
}

/// Restart a project's subscription so the frontend gets a fresh `snapshot`
/// (after a webview reload, the long-lived task would otherwise only send deltas).
#[tauri::command]
pub fn live_resync(app: AppHandle, hub: State<'_, LiveHub>, id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    hub.remove(&id);
    hub.ensure(&app, &id, &root);
    Ok(())
}

#[tauri::command]
pub fn projects_add(
    app: AppHandle,
    hub: State<'_, LiveHub>,
    root: String,
) -> Result<ProjectView, String> {
    let mut entries = registry::load()?;
    let (entry, fresh) = register(&mut entries, Path::new(&root))?;
    if fresh {
        registry::save(&entries)?;
        hub.ensure(&app, &entry.id, Path::new(&entry.root));
        notify_changed(&app);
    }
    Ok(view(&entry, None))
}

/// Registry edit behind `projects_add`: the project whose `atlas.yaml` is at or above `picked`, added once.
/// Returns its entry and whether it is new. A folder without a config is refused (`probe` tells the UI first).
fn register(entries: &mut Vec<Entry>, picked: &Path) -> Result<(Entry, bool), String> {
    let root = config::find_root(picked).ok_or(format!("no atlas.yaml in {}", picked.display()))?;
    let root = atlas_core::paths::canonical(&root).unwrap_or(root);
    let id = registry::id_for(&root);
    if let Some(e) = existing(entries, &id, &root) {
        return Ok((e.clone(), false));
    }
    let entry = Entry {
        id,
        root: root.to_string_lossy().into_owned(),
        pinned: false,
        added: chrono::Utc::now().to_rfc3339(),
        parent: None,
        port_offset: 0,
        branch: None,
        host: None,
        identity: None,
    };
    entries.push(entry.clone());
    Ok((entry, true))
}

/// What the Add flow learns about a picked folder before touching anything.
#[derive(Serialize, Debug)]
pub struct Probe {
    /// The folder that was picked (canonical).
    picked: String,
    /// The project root: where `atlas.yaml` is (the picked folder or one above it), else the picked folder.
    root: String,
    has_config: bool,
    /// Project name from the config, else the folder name.
    name: String,
    /// Id of the registered project for this folder, when there is one.
    registered: Option<String>,
}

fn probe(entries: &[Entry], picked: &Path) -> Result<Probe, String> {
    if !picked.is_dir() {
        return Err(format!("{} is not a folder", picked.display()));
    }
    let picked = atlas_core::paths::canonical(picked).unwrap_or_else(|_| picked.to_path_buf());
    let found = config::find_root(&picked).map(|r| atlas_core::paths::canonical(&r).unwrap_or(r));
    let root = found.clone().unwrap_or_else(|| picked.clone());
    let name = found
        .as_deref()
        .and_then(|r| config::load(r).ok())
        .map(|c| c.name)
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let registered = found
        .as_deref()
        .and_then(|r| existing(entries, &registry::id_for(r), r))
        .map(|e| e.id.clone());
    Ok(Probe {
        picked: picked.to_string_lossy().into_owned(),
        root: root.to_string_lossy().into_owned(),
        has_config: found.is_some(),
        name,
        registered,
    })
}

/// Look at a folder the user picked or dropped: has it a config, which project root is that, is it registered.
#[tauri::command]
pub fn project_probe(dir: String) -> Result<Probe, String> {
    probe(&registry::load()?, Path::new(&dir))
}

/// What `atlas init` would write in `dir` (nothing is created), for the preview before "Create and add".
#[derive(Serialize, Debug)]
pub struct InitPreview {
    yaml: String,
    /// `language (framework)` or `None` when nothing was recognised.
    detected: Option<String>,
    services: Vec<String>,
}

#[tauri::command]
pub fn project_init_preview(dir: String) -> Result<InitPreview, String> {
    let dir = Path::new(&dir);
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    if dir.join("atlas.yaml").exists() {
        return Err(format!("atlas.yaml already exists in {}", dir.display()));
    }
    let (yaml, d) = atlas_core::domain::project::initial_config(dir);
    Ok(InitPreview {
        yaml,
        detected: (d.language != "unknown").then(|| match &d.framework {
            Some(fw) => format!("{} ({fw})", d.language),
            None => d.language.clone(),
        }),
        services: d.services.into_iter().map(|s| s.name).collect(),
    })
}

/// The entry already showing `root`: same id, or (after a relink, which keeps the old id) the
/// same local folder under another id.
fn existing<'a>(entries: &'a [Entry], id: &str, root: &Path) -> Option<&'a Entry> {
    entries
        .iter()
        .find(|e| e.id == id || (!e.is_remote() && Path::new(&e.root) == root))
}

/// Take a project out of Atlas: its running stack is stopped first, then it and its linked worktrees are
/// unregistered. Never touches a file on disk.
#[tauri::command]
pub async fn projects_remove(
    app: AppHandle,
    hub: State<'_, LiveHub>,
    id: String,
) -> Result<(), String> {
    let mut entries = registry::load()?;
    // The main checkout's worktrees go with it, so stop every stack that is about to lose its row.
    let doomed: Vec<Entry> = entries
        .iter()
        .filter(|e| e.id == id || e.parent.as_deref() == Some(id.as_str()))
        .cloned()
        .collect();
    for e in doomed.iter().filter(|e| !e.is_remote()) {
        if status::live_snapshot(Path::new(&e.root)).is_ok() {
            super::stack_stop(app.clone(), e.id.clone()).await?;
        }
    }
    let gone = unregister(&mut entries, &id);
    registry::save(&entries)?;
    for g in &gone {
        hub.remove(g);
    }
    notify_changed(&app);
    Ok(())
}

/// Registry edit behind `projects_remove`: `id` and, when it is a main checkout, its linked worktrees (a dangling
/// `parent` would render them as orphans). Returns the ids removed, in registry order.
fn unregister(entries: &mut Vec<Entry>, id: &str) -> Vec<String> {
    let gone: Vec<String> = entries
        .iter()
        .filter(|e| e.id == id || e.parent.as_deref() == Some(id))
        .map(|e| e.id.clone())
        .collect();
    entries.retain(|e| !gone.contains(&e.id));
    gone
}

#[tauri::command]
pub fn projects_pin(app: AppHandle, id: String, pinned: bool) -> Result<(), String> {
    update_entry(&app, &id, |e| {
        e.pinned = pinned;
        Ok(())
    })
}

/// Set (or, with `None`/an empty identity, clear) the icon and color of a project.
#[tauri::command]
pub fn projects_set_identity(
    app: AppHandle,
    id: String,
    identity: Option<registry::Identity>,
) -> Result<(), String> {
    let identity = identity
        .map(registry::Identity::validated)
        .transpose()?
        .flatten();
    update_entry(&app, &id, |e| {
        e.identity = identity;
        Ok(())
    })
}

/// The running discovery's cancel flag (one scan at a time; a new one cancels the old).
#[derive(Default)]
pub struct Discovery(parking_lot::Mutex<Option<std::sync::Arc<AtomicBool>>>);

impl Discovery {
    /// Cancel the running scan (if any) and hand out the flag for the next one.
    fn begin(&self) -> std::sync::Arc<AtomicBool> {
        let mut slot = self.0.lock();
        if let Some(old) = slot.take() {
            old.store(true, Ordering::Relaxed);
        }
        let flag = std::sync::Arc::new(AtomicBool::new(false));
        *slot = Some(flag.clone());
        flag
    }

    fn cancel(&self) {
        if let Some(f) = self.0.lock().take() {
            f.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub struct DiscoverArgs {
    /// Extra folders to scan besides the defaults (absolute or `~/…`).
    roots: Vec<String>,
    /// Scan only `roots` instead of adding the defaults.
    only_roots: bool,
    /// Also report git roots without `atlas.yaml` (`Could use Atlas`).
    candidates: bool,
    depth: Option<u32>,
}

#[derive(Serialize)]
pub struct DiscoverDone {
    roots: Vec<String>,
    dirs: u64,
    unreadable: u64,
    elapsed_ms: u64,
    cancelled: bool,
}

fn expand(home: &Path, p: &str) -> std::path::PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if p == "~" => home.to_path_buf(),
        None => p.into(),
    }
}

/// Scan this machine for atlas projects, streaming `discover://event` as they turn up:
/// `{ t: "found", ...Found }` and `{ t: "progress", dirs, found, elapsed_ms, current }`.
/// Resolves when the scan finished or was cancelled.
#[tauri::command]
pub async fn projects_discover(
    app: AppHandle,
    disc: State<'_, Discovery>,
    args: Option<DiscoverArgs>,
) -> Result<DiscoverDone, String> {
    let args = args.unwrap_or_default();
    let cancel = disc.begin();
    tauri::async_runtime::spawn_blocking(move || {
        let start = std::time::Instant::now();
        let home = dirs::home_dir().ok_or("cannot locate home directory")?;
        let registered: Vec<String> = registry::load()?.into_iter().map(|e| e.root).collect();
        let extra: Vec<_> = args.roots.iter().map(|r| expand(&home, r)).collect();
        let roots = discover::canonical_roots(if args.only_roots {
            extra
        } else {
            let mut r = discover::default_roots(&home, &registered);
            r.extend(extra.into_iter().filter(|p| p.is_dir()));
            r
        });
        let mut emit = |ev: Event| {
            let _ = app.emit("discover://event", ev);
        };

        // Spotlight first: it already knows most projects, so the list fills in at once.
        let mut seen = std::collections::HashSet::new();
        for dir in discover::spotlight(&home, Duration::from_secs(4)) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let root = roots
                .iter()
                .find(|r| dir.starts_with(r))
                .cloned()
                .unwrap_or_default();
            if seen.insert(dir.clone()) && dir.join("atlas.yaml").is_file() {
                emit(Event::Found(discover::inspect(&dir, &root, false)));
            }
        }
        let opts = ScanOpts {
            roots: roots.clone(),
            max_depth: args.depth.unwrap_or(discover::DEFAULT_DEPTH),
            candidates: args.candidates,
        };
        let stats = discover::scan_skipping(&opts, &cancel, &seen, &mut emit);
        Ok(DiscoverDone {
            roots: roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect(),
            dirs: stats.dirs,
            unreadable: stats.unreadable,
            elapsed_ms: start.elapsed().as_millis() as u64,
            cancelled: cancel.load(Ordering::Relaxed),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn projects_discover_cancel(disc: State<'_, Discovery>) {
    disc.cancel();
}

/// Point a project at a new folder, keeping its id so tabs, layouts and its icon stay attached.
#[tauri::command]
pub fn projects_relink(
    app: AppHandle,
    hub: State<'_, LiveHub>,
    id: String,
    root: String,
) -> Result<ProjectView, String> {
    let mut entries = registry::load()?;
    let new = relink(&mut entries, &id, Path::new(&root))?;
    registry::save(&entries)?;
    hub.remove(&id);
    hub.ensure(&app, &id, Path::new(&new));
    notify_changed(&app);
    entries
        .iter()
        .find(|e| e.id == id)
        .map(|e| view(e, None))
        .ok_or_else(|| format!("unknown project {id}"))
}

/// Registry edit behind `projects_relink`; returns the root now stored.
fn relink(entries: &mut [Entry], id: &str, picked: &Path) -> Result<String, String> {
    let root = config::find_root(picked)
        .ok_or_else(|| format!("no atlas.yaml in {}", picked.display()))?;
    let root = atlas_core::paths::canonical(&root).unwrap_or(root);
    let root_s = root.to_string_lossy().into_owned();
    if entries
        .iter()
        .any(|e| e.id != id && !e.is_remote() && e.root == root_s)
    {
        return Err(format!("{root_s} is already another project in Atlas"));
    }
    let e = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown project {id}"))?;
    if e.is_remote() {
        return Err(super::REMOTE_UNSUPPORTED.into());
    }
    e.root = root_s.clone();
    Ok(root_s)
}

/// Drop every local project whose folder or atlas.yaml is gone. Returns how many were removed.
#[tauri::command]
pub fn projects_remove_missing(app: AppHandle, hub: State<'_, LiveHub>) -> Result<usize, String> {
    let mut entries = registry::load()?;
    let gone = take_missing(&mut entries);
    if gone.is_empty() {
        return Ok(0);
    }
    registry::save(&entries)?;
    for id in &gone {
        hub.remove(id);
    }
    notify_changed(&app);
    Ok(gone.len())
}

/// Remove missing local entries; a linked worktree whose main checkout goes too goes with it.
fn take_missing(entries: &mut Vec<Entry>) -> Vec<String> {
    let mut gone: Vec<String> = entries
        .iter()
        .filter(|e| missing_of(e).is_some())
        .map(|e| e.id.clone())
        .collect();
    let orphans: Vec<String> = entries
        .iter()
        .filter(|e| e.parent.as_ref().is_some_and(|p| gone.contains(p)) && !gone.contains(&e.id))
        .map(|e| e.id.clone())
        .collect();
    gone.extend(orphans);
    entries.retain(|e| !gone.contains(&e.id));
    gone
}

/// Where a missing project went: looks for an atlas.yaml with the same git remote (or, failing
/// that, the same name) among the default roots and Spotlight. `None` when nothing fits.
#[tauri::command]
pub async fn projects_find_moved(id: String) -> Result<Option<Found>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = dirs::home_dir().ok_or("cannot locate home directory")?;
        let entries = registry::load()?;
        let e = entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("unknown project {id}"))?;
        let known = registry::load_known().remove(&id);
        let name = known.as_ref().map_or_else(
            || {
                Path::new(&e.root)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            },
            |k| k.name.clone(),
        );
        let registered: Vec<String> = entries.iter().map(|e| e.root.clone()).collect();
        let opts = ScanOpts {
            roots: discover::default_roots(&home, &registered),
            max_depth: discover::DEFAULT_DEPTH,
            candidates: false,
        };
        let mut all = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for dir in discover::spotlight(&home, Duration::from_secs(4)) {
            if dir.join("atlas.yaml").is_file() && seen.insert(dir.clone()) {
                all.push(discover::inspect(&dir, &dir, false));
            }
        }
        discover::scan_skipping(&opts, &AtomicBool::new(false), &seen, &mut |ev| {
            if let Event::Found(f) = ev {
                all.push(f);
            }
        });
        // Folders other registered projects already use are not "the moved copy".
        all.retain(|f| !registered.contains(&f.path));
        let remote = known.and_then(|k| k.remote);
        Ok(discover::match_moved(&name, remote.as_deref(), &all).cloned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Create an `atlas.yaml` in `dir` with `atlas init` (stack detection lives in the CLI) and
/// register it. For the "Could use Atlas" results of discovery.
#[tauri::command]
pub async fn project_init(
    app: AppHandle,
    hub: State<'_, LiveHub>,
    dir: String,
) -> Result<ProjectView, String> {
    let out = tokio::process::Command::new(super::atlas_bin()?)
        .no_window()
        .args(["init", "--dir", &dir])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("cannot run atlas: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("atlas init failed ({})", out.status)
        } else {
            err
        });
    }
    projects_add(app, hub, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, root: &Path) -> Entry {
        Entry {
            id: id.into(),
            root: root.to_string_lossy().into_owned(),
            pinned: false,
            added: "now".into(),
            parent: None,
            port_offset: 0,
            branch: None,
            host: None,
            identity: None,
        }
    }

    fn project(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("atlas.yaml"), "name: x\nservices: {}\n").unwrap();
    }

    #[test]
    fn missing_distinguishes_present_gone_folder_gone_config_and_remote() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("ok"));
        std::fs::create_dir_all(t.path().join("noconf")).unwrap();
        assert_eq!(missing_of(&entry("a", &t.path().join("ok"))), None);
        assert_eq!(
            missing_of(&entry("b", &t.path().join("gone"))),
            Some(Missing::Root)
        );
        assert_eq!(
            missing_of(&entry("c", &t.path().join("noconf"))),
            Some(Missing::Config)
        );
        // A path that only exists on the SSH host is never stat'ed here.
        let mut remote = entry("d", &t.path().join("gone"));
        remote.host = Some("ssh:box".into());
        assert_eq!(missing_of(&remote), None);
    }

    #[test]
    fn a_missing_project_keeps_its_last_known_name_and_remote() {
        let t = tempfile::tempdir().unwrap();
        let e = entry("a", &t.path().join("gone"));
        let known = registry::Known {
            name: "Billing".into(),
            remote: Some("git@h:o/r.git".into()),
        };
        let v = view(&e, Some(&known));
        assert_eq!(v.missing, Some(Missing::Root));
        assert_eq!(v.name, "Billing");
        assert_eq!(v.last_remote.as_deref(), Some("git@h:o/r.git"));
        // Nothing known: the folder name.
        assert_eq!(view(&e, None).name, "gone");
        // A present project ignores stale knowledge.
        project(&t.path().join("here"));
        let v = view(&entry("b", &t.path().join("here")), Some(&known));
        assert_eq!(v.name, "x");
        assert_eq!(v.last_remote, None);
    }

    #[test]
    fn adding_a_folder_that_a_relinked_project_uses_finds_that_project() {
        let t = tempfile::tempdir().unwrap();
        let moved = t.path().join("moved");
        let mut remote = entry("r", &moved);
        remote.host = Some("ssh:box".into());
        let entries = vec![remote, entry("old-id", &moved)];
        // Same folder, different id: the relinked entry, never a duplicate.
        let hit = existing(&entries, &registry::id_for(&moved), &moved).unwrap();
        assert_eq!(hit.id, "old-id");
        assert!(existing(&entries, "x", &t.path().join("other")).is_none());
    }

    #[test]
    fn a_file_where_the_folder_was_counts_as_missing() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("f"), "x").unwrap();
        assert_eq!(
            missing_of(&entry("a", &t.path().join("f"))),
            Some(Missing::Root)
        );
    }

    #[test]
    fn take_missing_removes_only_gone_projects_and_their_orphaned_worktrees() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("ok"));
        project(&t.path().join("wt-of-gone"));
        let mut entries = vec![
            entry("ok", &t.path().join("ok")),
            entry("gone", &t.path().join("gone")),
            entry("wt", &t.path().join("wt-of-gone")),
        ];
        entries[2].parent = Some("gone".into());
        let removed = take_missing(&mut entries);
        assert_eq!(removed, ["gone", "wt"]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "ok");
        assert!(take_missing(&mut entries).is_empty());
    }

    #[test]
    fn relink_keeps_the_id_and_validates_the_target() {
        let t = tempfile::tempdir().unwrap();
        let new = t.path().join("moved");
        project(&new);
        let mut entries = vec![
            entry("keep", &t.path().join("old")),
            entry("other", &t.path().join("o")),
        ];
        let stored = relink(&mut entries, "keep", &new).unwrap();
        assert_eq!(entries[0].id, "keep");
        assert_eq!(entries[0].root, stored);
        // The stored form: canonical, and on Windows with forward slashes and no verbatim prefix.
        assert_eq!(
            Path::new(&stored),
            atlas_core::paths::canonical(&new).unwrap()
        );
        assert_eq!(missing_of(&entries[0]), None);

        // No atlas.yaml in (or above) the folder: refused, entry untouched.
        let empty = tempfile::tempdir().unwrap();
        let before = entries[0].root.clone();
        assert!(relink(&mut entries, "keep", empty.path()).is_err());
        assert_eq!(entries[0].root, before);
        assert!(relink(&mut entries, "nope", &new).is_err());
    }

    #[test]
    fn relink_refuses_a_folder_another_project_already_uses() {
        let t = tempfile::tempdir().unwrap();
        let used = t.path().join("used");
        project(&used);
        let canon = atlas_core::paths::canonical(&used).unwrap();
        let mut entries = vec![entry("a", &t.path().join("old")), entry("b", &canon)];
        let err = relink(&mut entries, "a", &used).unwrap_err();
        assert!(err.contains("already another project"), "{err}");
    }

    #[test]
    fn discovery_cancel_flags_the_previous_scan() {
        let d = Discovery::default();
        let first = d.begin();
        let second = d.begin();
        assert!(first.load(Ordering::Relaxed));
        assert!(!second.load(Ordering::Relaxed));
        d.cancel();
        assert!(second.load(Ordering::Relaxed));
        d.cancel();
    }

    #[test]
    fn tilde_roots_expand_against_home() {
        let home = Path::new("/h");
        assert_eq!(expand(home, "~/code"), Path::new("/h/code"));
        assert_eq!(expand(home, "~"), Path::new("/h"));
        assert_eq!(expand(home, "/abs"), Path::new("/abs"));
    }

    // Incident (#65): adding a folder without atlas.yaml failed with "no atlas.yaml in <dir>" and the UI had no way
    // forward. `probe` is what the Add flow asks first, so it can offer `atlas init` instead of that error.
    #[test]
    fn probe_tells_a_configless_folder_from_a_configured_one_and_a_subfolder_of_one() {
        let t = tempfile::tempdir().unwrap();
        let bare = t.path().join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let p = probe(&[], &bare).unwrap();
        assert!(!p.has_config);
        assert_eq!(p.registered, None);
        assert_eq!(p.name, "bare");

        let proj = t.path().join("proj");
        project(&proj);
        let p = probe(&[], &proj).unwrap();
        assert!(p.has_config);
        assert_eq!(p.name, "x");
        assert_eq!(
            Path::new(&p.root),
            atlas_core::paths::canonical(&proj).unwrap()
        );

        // `find_root` walks up: a subfolder of a configured project would silently add the PARENT, so the probe
        // reports the folder the config really lives in and the UI says so.
        let sub = proj.join("apps/web");
        std::fs::create_dir_all(&sub).unwrap();
        let p = probe(&[], &sub).unwrap();
        assert!(p.has_config);
        assert_eq!(
            Path::new(&p.root),
            atlas_core::paths::canonical(&proj).unwrap()
        );
        assert_eq!(
            Path::new(&p.picked),
            atlas_core::paths::canonical(&sub).unwrap()
        );
    }

    #[test]
    fn probe_reports_the_project_already_registered_for_that_folder() {
        let t = tempfile::tempdir().unwrap();
        let proj = t.path().join("proj");
        project(&proj);
        let canon = atlas_core::paths::canonical(&proj).unwrap();
        let entries = vec![entry("abc", &canon)];
        assert_eq!(
            probe(&entries, &proj).unwrap().registered.as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn probe_refuses_what_is_not_a_folder() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("f");
        std::fs::write(&file, "x").unwrap();
        assert!(probe(&[], &file).is_err());
        assert!(probe(&[], &t.path().join("gone")).is_err());
    }

    #[test]
    fn register_adds_once_and_never_duplicates_a_folder() {
        let t = tempfile::tempdir().unwrap();
        let proj = t.path().join("proj");
        project(&proj);
        let mut entries = Vec::new();
        let (first, fresh) = register(&mut entries, &proj).unwrap();
        assert!(fresh);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            Path::new(&first.root),
            atlas_core::paths::canonical(&proj).unwrap()
        );
        assert_eq!(
            first.id,
            registry::id_for(&atlas_core::paths::canonical(&proj).unwrap())
        );
        let (again, fresh) = register(&mut entries, &proj).unwrap();
        assert!(!fresh);
        assert_eq!(again.id, first.id);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn register_without_a_config_is_the_old_error_so_callers_must_init_first() {
        let t = tempfile::tempdir().unwrap();
        let bare = t.path().join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let mut entries = Vec::new();
        let err = register(&mut entries, &bare).unwrap_err();
        assert!(err.contains("no atlas.yaml"), "{err}");
        assert!(entries.is_empty());
    }

    // Incident (#65): a main project removed from the app left its linked worktrees registered with a dangling
    // `parent`, rendered as orphans. Unregistering takes them along; files are never touched.
    #[test]
    fn unregister_takes_the_project_and_its_worktrees_and_leaves_files_alone() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("main"));
        project(&t.path().join("wt"));
        project(&t.path().join("other"));
        let mut entries = vec![
            entry("main", &t.path().join("main")),
            entry("wt", &t.path().join("wt")),
            entry("other", &t.path().join("other")),
        ];
        entries[1].parent = Some("main".into());
        let gone = unregister(&mut entries, "main");
        assert_eq!(gone, ["main", "wt"]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "other");
        for d in ["main", "wt", "other"] {
            assert!(t.path().join(d).join("atlas.yaml").is_file(), "{d} touched");
        }
    }

    #[test]
    fn unregister_of_a_worktree_or_unknown_id_removes_only_that_one() {
        let t = tempfile::tempdir().unwrap();
        let mut entries = vec![
            entry("main", &t.path().join("main")),
            entry("wt", &t.path().join("wt")),
        ];
        entries[1].parent = Some("main".into());
        assert!(unregister(&mut entries, "nope").is_empty());
        assert_eq!(entries.len(), 2);
        assert_eq!(unregister(&mut entries, "wt"), ["wt"]);
        assert_eq!(entries[0].id, "main");
    }

    // Acceptance (#65): a folder without atlas.yaml becomes a project with a config and no terminal. The steps the UI
    // runs, on a real folder: probe says "no config", the preview names what will be written, `atlas init`'s file
    // (`initial_config`) is written, and the folder then registers once and loads as a project of that name.
    #[test]
    fn a_configless_folder_becomes_a_loadable_registered_project() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("shop");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"name":"shop","scripts":{"dev":"vite --port 5173"}}"#,
        )
        .unwrap();
        let mut entries = Vec::new();
        assert!(register(&mut entries, &dir).is_err());
        assert!(!probe(&entries, &dir).unwrap().has_config);

        let preview = project_init_preview(dir.to_string_lossy().into_owned()).unwrap();
        assert!(preview.yaml.starts_with("name: shop\n"), "{}", preview.yaml);
        assert!(!dir.join("atlas.yaml").exists(), "a preview must not write");

        std::fs::write(dir.join("atlas.yaml"), &preview.yaml).unwrap();
        let (entry, fresh) = register(&mut entries, &dir).unwrap();
        assert!(fresh);
        let v = view(&entry, None);
        assert_eq!(v.name, "shop");
        assert_eq!(v.error, None, "the generated config must load cleanly");
        assert_eq!(v.missing, None);
        assert_eq!(v.services_total, preview.services.len());
        assert!(probe(&entries, &dir).unwrap().registered.is_some());
    }

    #[test]
    fn the_init_preview_refuses_an_existing_config_and_a_non_folder() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("p"));
        let p = t.path().join("p").to_string_lossy().into_owned();
        assert!(
            project_init_preview(p)
                .unwrap_err()
                .contains("already exists")
        );
        let f = t.path().join("f");
        std::fs::write(&f, "x").unwrap();
        assert!(project_init_preview(f.to_string_lossy().into_owned()).is_err());
    }

    // Contract (AGENTS.md): the frontend reads the REAL serialization of what `project_probe` returns.
    // `UPDATE_FIXTURES=1` regenerates `__fixtures__/project-probe.json`.
    #[test]
    fn probe_fixture_is_the_real_serialization() {
        let t = tempfile::tempdir().unwrap();
        let bare = t.path().join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let mut real = serde_json::to_value(probe(&[], &bare).unwrap()).unwrap();
        // The temp path differs per run; the contract is the shape, so it is pinned to a stable value.
        for k in ["root", "picked"] {
            real[k] = "/projects/bare".into();
        }
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/project-probe.json");
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&real, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
        )
        .unwrap();
        assert_eq!(
            committed, real,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
