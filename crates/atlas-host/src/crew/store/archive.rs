//! The crew export/import archive (issue #202): one versioned `.tar.gz` of crewmate definitions, memory,
//! task history, automations, usage and `crew.json`, so a crew server's data can be backed up, moved,
//! or kept through an uninstall.
//!
//! Layout (the manifest is always the first entry; there is no other kind of entry):
//!
//! ```text
//! manifest.json                 [`Manifest`]: format version, atlas version, crewmates, created_at
//! crew.json                     crew-wide settings (never `paused`)
//! crew/<id>/crewmate.json       definition + automations (a pre-rename `bot.json` is written under this name)
//! crew/<id>/memory.md  tasks.jsonl  usage.jsonl  usage-<date>.jsonl.gz
//! crew/<id>/secrets.json        only with `secrets: true`
//! ```
//!
//! Never exported: the `work/` checkout, `session.json` (an agent session id means nothing on another
//! machine), `<trigger>_seen.jsonl` (per-machine GitHub dedupe) and agent sign-in files (they live outside
//! the crew dir). Import validates everything (format, ids, entry paths, collisions) before it writes a
//! byte, and never replaces an existing crewmate or the settings of a crew that has some.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Component;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use super::{CrewSettings, CrewStore, Crewmate, TriggerKind, now_s};
use crate::paths;

/// Bumped when the layout changes in a way an older import cannot read.
pub const FORMAT_VERSION: u32 = 1;
/// The first entry of every archive.
pub const MANIFEST: &str = "manifest.json";
const SETTINGS: &str = "crew.json";
const DEFINITION: &str = "crewmate.json";
/// How many fresh ids `plan_ids` draws before giving up (random 128-bit ids never collide in practice).
const FRESH_TRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestCrewmate {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    /// The atlas version that wrote the archive.
    pub atlas_version: String,
    /// Unix seconds.
    pub created_at: u64,
    /// Whether `secrets.json` files and webhook tokens are inside.
    pub secrets: bool,
    pub crewmates: Vec<ManifestCrewmate>,
}

#[derive(Debug)]
pub enum ImportError {
    /// Not a gzip'd tar, or truncated.
    Unreadable(String),
    /// A tar without `manifest.json` first: not one of ours.
    NoManifest,
    /// A `format` newer (or older, unknown) than [`FORMAT_VERSION`].
    UnsupportedFormat(u32),
    /// An entry or id that could write outside its crewmate's directory, or is not in the layout.
    BadEntry(String),
    /// These ids already exist and `--rename` was not given.
    Collision(Vec<String>),
    Io(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "not a crew archive ({e})"),
            Self::NoManifest => write!(f, "not a crew archive (no {MANIFEST} first)"),
            Self::UnsupportedFormat(v) => write!(
                f,
                "archive format {v} is not supported by this atlas (it reads format {FORMAT_VERSION}); update atlas"
            ),
            Self::BadEntry(e) => write!(f, "refused unsafe archive entry: {e}"),
            Self::Collision(ids) => write!(
                f,
                "crewmate id(s) already on this server: {}; pass --rename to import them under new ids",
                ids.join(", ")
            ),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub old_id: String,
    pub new_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    pub imported: Vec<Imported>,
    /// `crew.json` was restored (the target had none).
    pub settings_restored: bool,
}

/// Whether a file directly inside a crewmate's directory goes into the archive. By file name alone,
/// and an allow-list: anything not named here (the checkout, sessions, sign-in files, a future cache)
/// stays out.
pub fn is_exported(name: &str, with_secrets: bool) -> bool {
    match name {
        DEFINITION | super::LEGACY_FILE | "memory.md" | "tasks.jsonl" | "usage.jsonl" => true,
        "secrets.json" => with_secrets,
        n => super::usage::is_rotated_name(n),
    }
}

/// Old id → id to store it under. Free ids stay. A taken id is an error, or with `rename` a fresh
/// id from `fresh` that is neither taken nor another incoming id.
pub fn plan_ids(
    incoming: &[String],
    taken: impl Fn(&str) -> bool,
    rename: bool,
    mut fresh: impl FnMut() -> String,
) -> Result<HashMap<String, String>, ImportError> {
    let colliding: Vec<String> = incoming.iter().filter(|i| taken(i)).cloned().collect();
    if !colliding.is_empty() && !rename {
        return Err(ImportError::Collision(colliding));
    }
    let mut used: HashSet<String> = incoming.iter().cloned().collect();
    let mut map: HashMap<String, String> =
        incoming.iter().map(|i| (i.clone(), i.clone())).collect();
    for old in &colliding {
        let new = (0..FRESH_TRIES)
            .map(|_| fresh())
            .find(|n| !taken(n) && !used.contains(n))
            .ok_or_else(|| ImportError::Io("could not draw a free crewmate id".into()))?;
        used.insert(new.clone());
        map.insert(old.clone(), new);
    }
    Ok(map)
}

/// Rewrite a crewmate for its new place: its own id, and every after-crewmate trigger that follows a
/// crewmate whose id changed. `reset_fired`: a copy has not fired anything (`last_fired` cleared).
pub fn remap_crewmate(c: &mut Crewmate, map: &HashMap<String, String>, reset_fired: bool) {
    if let Some(new) = map.get(&c.id) {
        c.id = new.clone();
    }
    for t in &mut c.triggers {
        if let TriggerKind::Crewmate { from_crewmate, .. } = &mut t.kind
            && let Some(new) = map.get(from_crewmate.as_str())
        {
            *from_crewmate = new.clone();
        }
        if reset_fired {
            t.last_fired = None;
        }
    }
}

/// A webhook token is a credential kept in `crewmate.json`: without secrets it is dropped and the
/// automation arrives disabled (an empty token never matches a request).
fn strip_webhook_secrets(c: &mut Crewmate) {
    for t in &mut c.triggers {
        if let TriggerKind::Webhook { token, .. } = &mut t.kind {
            token.clear();
            t.enabled = false;
        }
    }
}

/// An id read from an archive is a directory name: one plain path component.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn append(
    b: &mut tar::Builder<impl Write>,
    path: &str,
    data: &[u8],
    private: bool,
) -> std::io::Result<()> {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(if private { 0o600 } else { 0o644 });
    h.set_mtime(now_s());
    b.append_data(&mut h, path, data)
}

/// Write the archive of `ids` (every crewmate when empty) to `out`. An id that does not exist is an
/// error, never a quietly smaller archive.
pub fn export(
    store: &CrewStore,
    ids: &[String],
    with_secrets: bool,
    out: &mut impl Write,
) -> anyhow::Result<Manifest> {
    let mut crewmates = store.list_crewmates();
    crewmates.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
    if !ids.is_empty() {
        if let Some(missing) = ids.iter().find(|i| !crewmates.iter().any(|c| &c.id == *i)) {
            bail!("no crewmate with id {missing} on this server");
        }
        crewmates.retain(|c| ids.contains(&c.id));
    }
    let manifest = Manifest {
        format: FORMAT_VERSION,
        atlas_version: env!("CARGO_PKG_VERSION").into(),
        created_at: now_s(),
        secrets: with_secrets,
        crewmates: crewmates
            .iter()
            .map(|c| ManifestCrewmate {
                id: c.id.clone(),
                name: c.name.clone(),
            })
            .collect(),
    };
    let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
        out,
        flate2::Compression::default(),
    ));
    append(
        &mut b,
        MANIFEST,
        &serde_json::to_vec_pretty(&manifest)?,
        false,
    )?;
    // `paused` is this server's live kill switch, not a setting to carry to another.
    let settings = CrewSettings {
        paused: false,
        ..store.load_crew_settings()
    };
    append(
        &mut b,
        SETTINGS,
        &serde_json::to_vec_pretty(&settings)?,
        false,
    )?;
    for mut c in crewmates {
        let id = c.id.clone();
        let dir = store.crewmate_dir(&id);
        if !with_secrets {
            strip_webhook_secrets(&mut c);
        }
        // The definition is re-serialized: a pre-rename `bot.json` leaves in the current shape.
        append(
            &mut b,
            &format!("crew/{id}/{DEFINITION}"),
            &serde_json::to_vec_pretty(&c)?,
            true,
        )?;
        let files = ["memory.md", "tasks.jsonl", "usage.jsonl", "secrets.json"]
            .map(String::from)
            .into_iter()
            .chain(super::usage::rotated_names(&dir));
        for file in files {
            if !is_exported(&file, with_secrets) {
                continue;
            }
            // Memory follows the crewmate: under isolation it is in its own directory.
            let path = if file == "memory.md" {
                store.memory_path(&id)
            } else {
                dir.join(&file)
            };
            match std::fs::read(path) {
                Ok(data) => append(
                    &mut b,
                    &format!("crew/{id}/{file}"),
                    &data,
                    file == "secrets.json",
                )?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).with_context(|| format!("reading {id}/{file}")),
            }
        }
    }
    b.into_inner()?.finish()?;
    Ok(manifest)
}

/// What an archive holds once read and checked, before anything is written.
struct Parsed {
    manifest: Manifest,
    settings: Option<Vec<u8>>,
    /// `(crewmate id, file name) → bytes`
    files: BTreeMap<(String, String), Vec<u8>>,
}

/// An entry path must be `manifest.json`, `crew.json` or `crew/<plain id>/<allowed file>`: no `..`,
/// no absolute path, no nesting.
fn classify(path: &str) -> Result<Entry<'_>, ImportError> {
    let bad = || ImportError::BadEntry(path.to_string());
    let p = std::path::Path::new(path);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(bad());
    }
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [MANIFEST] => Ok(Entry::Manifest),
        [SETTINGS] => Ok(Entry::Settings),
        ["crew", id, file] if valid_id(id) && is_exported(file, true) => Ok(Entry::File(id, file)),
        _ => Err(bad()),
    }
}

enum Entry<'a> {
    Manifest,
    Settings,
    File(&'a str, &'a str),
}

fn parse(r: impl Read) -> Result<Parsed, ImportError> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(r));
    let unreadable = |e: std::io::Error| ImportError::Unreadable(e.to_string());
    let mut manifest: Option<Manifest> = None;
    let mut settings = None;
    let mut files = BTreeMap::new();
    for (i, entry) in archive.entries().map_err(unreadable)?.enumerate() {
        let mut entry = entry.map_err(unreadable)?;
        let path = entry
            .path()
            .map_err(unreadable)?
            .to_string_lossy()
            .into_owned();
        if i == 0 && path != MANIFEST {
            return Err(ImportError::NoManifest);
        }
        let kind = classify(&path)?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(unreadable)?;
        match kind {
            Entry::Manifest => {
                let m: Manifest = serde_json::from_slice(&data)
                    .map_err(|e| ImportError::Unreadable(format!("manifest: {e}")))?;
                // Checked as soon as it is read: a future format may change what follows.
                if m.format != FORMAT_VERSION {
                    return Err(ImportError::UnsupportedFormat(m.format));
                }
                manifest = Some(m);
            }
            Entry::Settings => settings = Some(data),
            Entry::File(id, file) => {
                files.insert((id.to_string(), file.to_string()), data);
            }
        }
    }
    let manifest = manifest.ok_or(ImportError::NoManifest)?;
    for c in &manifest.crewmates {
        if !valid_id(&c.id) {
            return Err(ImportError::BadEntry(format!("crewmate id {:?}", c.id)));
        }
    }
    if let Some(((id, _), _)) = files
        .iter()
        .find(|((id, _), _)| !manifest.crewmates.iter().any(|c| &c.id == id))
    {
        return Err(ImportError::BadEntry(format!(
            "crew/{id}/… is not in the manifest"
        )));
    }
    Ok(Parsed {
        manifest,
        settings,
        files,
    })
}

/// Only the manifest, to show what an archive holds before importing it.
pub fn read_manifest(r: impl Read) -> Result<Manifest, ImportError> {
    parse(r).map(|p| p.manifest)
}

/// Import into `store`. Every check runs before the first write, so a refused archive leaves the
/// store as it was. Crewmates whose id is taken are refused, or with `rename` stored under new ids
/// (chain triggers follow). Returns what went where.
pub fn import(store: &CrewStore, r: impl Read, rename: bool) -> Result<ImportReport, ImportError> {
    let parsed = parse(r)?;
    let incoming: Vec<String> = parsed
        .manifest
        .crewmates
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let map = plan_ids(
        &incoming,
        |id| store.crewmate_dir(id).exists(),
        rename,
        paths::new_id,
    )?;
    // Decode every definition now: a corrupt one must refuse the whole import, not half of it.
    let mut definitions = Vec::new();
    for m in &parsed.manifest.crewmates {
        let data = parsed
            .files
            .get(&(m.id.clone(), DEFINITION.to_string()))
            .ok_or_else(|| {
                ImportError::BadEntry(format!("crew/{}/{DEFINITION} is missing", m.id))
            })?;
        let mut c: Crewmate = serde_json::from_slice(data)
            .map_err(|e| ImportError::Unreadable(format!("{}: {e}", m.id)))?;
        if c.id != m.id {
            return Err(ImportError::BadEntry(format!(
                "crew/{}/{DEFINITION} says id {}",
                m.id, c.id
            )));
        }
        remap_crewmate(&mut c, &map, true);
        definitions.push((m.id.clone(), c));
    }
    let settings: Option<CrewSettings> = parsed
        .settings
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|e| ImportError::Unreadable(format!("crew.json: {e}")))?;

    let mut imported = Vec::new();
    for (old_id, c) in definitions {
        let new_id = c.id.clone();
        store.save_crewmate(&c)?;
        let dir = store.crewmate_dir(&new_id);
        // Every file the archive holds for it: `classify` let in only allowed names (rotated usage included).
        let own = parsed
            .files
            .range((old_id.clone(), String::new())..)
            .take_while(|((id, _), _)| *id == old_id);
        for ((_, file), data) in own {
            if file != DEFINITION && file != super::LEGACY_FILE {
                paths::write_private(&dir.join(file), data)?;
            }
        }
        imported.push(Imported {
            old_id,
            new_id,
            name: c.name,
        });
    }
    // A running crew keeps its own settings (and its kill switch): an archive only fills in an empty one.
    let settings_restored = match settings {
        Some(s) if !store.crew_settings_path().exists() => {
            store.save_crew_settings(&CrewSettings { paused: false, ..s })?;
            true
        }
        _ => false,
    };
    Ok(ImportReport {
        imported,
        settings_restored,
    })
}
