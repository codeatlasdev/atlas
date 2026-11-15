//! The on-disk store: blobs, entries, the lock, LRU eviction under a byte cap.
//!
//! Reads (`lookup`, `restore`, `replay`) take no lock: blobs never change once written and an
//! entry is replaced by an atomic rename, so a reader sees the old entry or the new one. Every
//! mutation (record, eviction, stats, clear, gc) runs under `lock`, an OS file lock that a dying
//! process releases by dying (the same primitive as the heavy queue).
//!
//! An entry file's modification time is its last use: a hit touches it, eviction removes the
//! oldest first. Blobs are shared between entries, so one is deleted only when no entry names it.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::key::{Key, Spec};
use super::log::{self, Chunk};
use super::run::{Recorded, Restored, Run};
use super::stats::{self, LastResult, Recent, Stats, Why};
use crate::{hygiene, sys};

/// Store directory override (tests, sandboxes).
pub const ENV_DIR: &str = "ATLAS_CACHE_DIR";
/// Byte cap override (a positive integer). Also `cache.max_bytes` in `~/.atlas/config.json`.
pub const ENV_MAX_BYTES: &str = "ATLAS_CACHE_MAX_BYTES";
/// Default cap: 2 GiB, a few hundred test and build outputs.
pub const DEFAULT_MAX_BYTES: u64 = 2 << 30;
/// How long a mutation waits for the lock before giving up (the run itself is never failed by it).
const LOCK_WAIT: Duration = Duration::from_secs(30);

/// One file a run produced, as the entry names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutFile {
    /// Relative to the project root, `/`-separated.
    pub path: String,
    /// sha256 of the content (the blob's name).
    pub blob: String,
    pub size: u64,
    /// Unix permission bits (0 where the OS has none).
    pub mode: u32,
}

/// A stored result (`entries/<key>.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub project: String,
    pub label: String,
    pub command: String,
    pub exit: i32,
    /// How long the command took when it ran for real.
    pub duration_ms: u64,
    /// Unix milliseconds.
    pub created_at: u64,
    /// The declared outputs (files or directories): a directory is emptied before a restore.
    pub output_roots: Vec<String>,
    pub outputs: Vec<OutFile>,
    /// sha256 of the log blob.
    pub log_blob: String,
}

/// `atlas cache status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub dir: String,
    pub entries: usize,
    /// Bytes on disk: blobs, entries and counters.
    pub bytes: u64,
    pub max_bytes: u64,
    pub stats: Stats,
}

/// What `gc`, `clear` and eviction gave back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freed {
    pub entries: usize,
    pub blobs: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
    max_bytes: u64,
    keep_failures: bool,
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The cap: a positive `env` value wins, then a positive `config` one, else [`DEFAULT_MAX_BYTES`].
pub(super) fn resolve_max(env: Option<&str>, config: Option<u64>) -> u64 {
    env.and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .or(config.filter(|n| *n > 0))
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// `cache.max_bytes` of the user config file.
pub(super) fn config_max_bytes(path: &Path) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    v.get("cache")?.get("max_bytes")?.as_u64()
}

impl Cache {
    /// A store at `root` (created on first write) holding at most `max_bytes`.
    pub fn at(root: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_bytes,
            keep_failures: false,
        }
    }

    /// `$ATLAS_CACHE_DIR`, else `~/.atlas/cache`; the cap from `$ATLAS_CACHE_MAX_BYTES`, else
    /// `cache.max_bytes` in `~/.atlas/config.json`, else [`DEFAULT_MAX_BYTES`].
    pub fn default_cache() -> Self {
        let root = std::env::var_os(ENV_DIR)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| crate::paths::atlas_home().join("cache"));
        let max = resolve_max(
            std::env::var(ENV_MAX_BYTES).ok().as_deref(),
            config_max_bytes(&crate::paths::atlas_home().join("config.json")),
        );
        Self::at(root, max)
    }

    /// Store failed runs too (a hit then replays the failure's exit code).
    #[must_use]
    pub fn keep_failures(mut self) -> Self {
        self.keep_failures = true;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn blobs(&self) -> PathBuf {
        self.root.join("blobs")
    }
    fn entries(&self) -> PathBuf {
        self.root.join("entries")
    }
    fn blob_path(&self, sha: &str) -> PathBuf {
        self.blobs().join(&sha[..2]).join(sha)
    }
    fn entry_path(&self, key: &str) -> PathBuf {
        self.entries().join(format!("{key}.json"))
    }

    fn init(&self) -> io::Result<()> {
        fs::create_dir_all(self.blobs())?;
        fs::create_dir_all(self.entries())?;
        // Spotlight must not index a store of build outputs.
        hygiene::mark_never_index(&self.root);
        Ok(())
    }

    /// The store lock, waiting up to [`LOCK_WAIT`] for another process's mutation.
    fn lock(&self) -> io::Result<File> {
        self.init()?;
        let mut o = OpenOptions::new();
        o.create(true).read(true).write(true).truncate(false);
        let f = sys::owner_only(&mut o).open(self.root.join("lock"))?;
        let start = Instant::now();
        while !sys::try_lock(&f)? {
            if start.elapsed() > LOCK_WAIT {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the task cache is locked by another atlas",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(f)
    }

    // ---- blobs ----

    /// Store `bytes` (content-addressed); returns the sha256. Already present: nothing written.
    fn put_blob(&self, bytes: &[u8]) -> io::Result<String> {
        let sha = hex::encode(Sha256::digest(bytes));
        let path = self.blob_path(&sha);
        if path.is_file() {
            return Ok(sha);
        }
        fs::create_dir_all(path.parent().expect("blob has a parent"))?;
        crate::fs::write_atomic(&path, bytes, false)?;
        Ok(sha)
    }

    /// Store the file at `src` as a blob: streamed through the hash, then moved into place.
    fn put_blob_file(&self, src: &Path) -> io::Result<(String, u64)> {
        let bytes = fs::read(src)?;
        let size = bytes.len() as u64;
        Ok((self.put_blob(&bytes)?, size))
    }

    // ---- reads ----

    /// The entry for `key`, when it is stored and every blob it names is still there (a hit
    /// must never be a half-restorable lie). A hit marks the entry as used.
    pub fn lookup(&self, key: &Key) -> Option<Entry> {
        let entry = self.peek(key)?;
        if let Ok(f) = File::options()
            .write(true)
            .open(self.entry_path(key.as_str()))
        {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(entry)
    }

    /// [`lookup`](Self::lookup) as a question: the same entry, but it is not marked as used (so
    /// asking whether a run would hit never keeps an entry alive against eviction).
    pub fn peek(&self, key: &Key) -> Option<Entry> {
        let entry: Entry =
            serde_json::from_slice(&fs::read(self.entry_path(key.as_str())).ok()?).ok()?;
        if entry.key != key.as_str() {
            return None;
        }
        let blobs_ok = entry
            .outputs
            .iter()
            .map(|o| o.blob.as_str())
            .chain([entry.log_blob.as_str()])
            .all(|b| b.len() == 64 && self.blob_path(b).is_file());
        blobs_ok.then_some(entry)
    }

    /// Put the entry's outputs into `spec.cwd`: a declared directory is emptied first (the tree
    /// is what the command left, not a merge), each file is a copy-on-write clone of its blob
    /// where the file system can, else a copy. All or an error; nothing is half-written in place.
    pub fn restore(&self, entry: &Entry, spec: &Spec) -> io::Result<Restored> {
        for root in &entry.output_roots {
            let p = spec.cwd.join(root);
            match fs::symlink_metadata(&p) {
                Ok(m) if m.is_dir() => fs::remove_dir_all(&p)?,
                Ok(_) => fs::remove_file(&p)?,
                Err(_) => {}
            }
        }
        let mut bytes = 0;
        for o in &entry.outputs {
            let dst = spec.cwd.join(&o.path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            let src = self.blob_path(&o.blob);
            let tmp = dst.with_extension(format!("atlas-restore-{}", std::process::id()));
            let _ = fs::remove_file(&tmp);
            match sys::clone_file(&src, &tmp) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::Unsupported => {
                    fs::copy(&src, &tmp)?;
                }
                Err(e) => return Err(e),
            }
            #[cfg(unix)]
            if o.mode != 0 {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&tmp, fs::Permissions::from_mode(o.mode))?;
            }
            fs::rename(&tmp, &dst)?;
            bytes += o.size;
        }
        Ok(Restored {
            files: entry.outputs.len(),
            bytes,
        })
    }

    fn read_log(&self, entry: &Entry) -> io::Result<Vec<Chunk>> {
        let bytes = fs::read(self.blob_path(&entry.log_blob))?;
        log::decode(&bytes)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "damaged log blob"))
    }

    /// Write the entry's captured output to `out` and `err`, each chunk to its own stream.
    pub fn replay(
        &self,
        entry: &Entry,
        out: &mut impl Write,
        err: &mut impl Write,
    ) -> io::Result<()> {
        log::replay(&self.read_log(entry)?, out, err)
    }

    // ---- record ----

    /// Store a finished run under `key`. Exit 0 only, unless [`keep_failures`](Self::keep_failures);
    /// a run that does not fit the cap alone is not stored. Whatever the outcome, the run goes to
    /// `recent.json` (the tool `last_result` reads it) and, on a miss, to the counters.
    pub fn record(&self, key: &Key, spec: &Spec, run: &Run) -> io::Result<Recorded> {
        let stored = if run.exit != 0 && !self.keep_failures {
            Recorded::NotStored(Why::Failed)
        } else {
            self.store(key, spec, run)?
        };
        self.note_miss(key, run);
        Ok(stored)
    }

    fn store(&self, key: &Key, spec: &Spec, run: &Run) -> io::Result<Recorded> {
        // Gather outside the lock: reading outputs can take a while and blocks nobody.
        let mut files = Vec::new();
        for root in &spec.outputs {
            collect_files(&spec.cwd, &spec.cwd.join(root), &mut files)?;
        }
        let mut chunks = run.log.clone();
        log::clamp(&mut chunks);
        let log_bytes = log::encode(&chunks);
        let incoming: u64 = files.iter().map(|f| f.2).sum::<u64>() + log_bytes.len() as u64;
        if incoming >= self.max_bytes {
            return Ok(Recorded::NotStored(Why::TooBig));
        }
        let _lock = self.lock()?;
        let mut outputs = Vec::with_capacity(files.len());
        for (rel, abs, _, mode) in &files {
            let (blob, size) = self.put_blob_file(abs)?;
            outputs.push(OutFile {
                path: rel.clone(),
                blob,
                size,
                mode: *mode,
            });
        }
        let entry = Entry {
            key: key.to_string(),
            project: run.project.clone(),
            label: run.label.clone(),
            command: run.command.clone(),
            exit: run.exit,
            duration_ms: run.duration_ms,
            created_at: now_ms(),
            output_roots: spec.outputs.clone(),
            outputs,
            log_blob: self.put_blob(&log_bytes)?,
        };
        crate::fs::write_atomic(
            &self.entry_path(key.as_str()),
            &serde_json::to_vec(&entry)?,
            false,
        )?;
        // The cap holds after every insert; the new entry is the newest so it goes last.
        self.evict_locked();
        Ok(if self.entry_path(key.as_str()).is_file() {
            Recorded::Stored
        } else {
            Recorded::NotStored(Why::TooBig)
        })
    }

    // ---- size, eviction, maintenance ----

    /// Bytes the store occupies: blobs, entry files and counters.
    fn disk_bytes(&self) -> u64 {
        hygiene::dir_size(&self.blobs())
            + hygiene::dir_size(&self.entries())
            + ["stats.json", "recent.json"]
                .iter()
                .map(|f| fs::metadata(self.root.join(f)).map_or(0, |m| m.len()))
                .sum::<u64>()
    }

    /// Entries with their last use, oldest first. Unreadable entry files are dropped on the way.
    fn entries_by_use(&self) -> Vec<(SystemTime, PathBuf, Entry)> {
        let mut v: Vec<_> = fs::read_dir(self.entries())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension().is_none_or(|x| x != "json") {
                    return None;
                }
                let used = e.metadata().ok()?.modified().ok()?;
                match fs::read(&p)
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                {
                    Some(entry) => Some((used, p, entry)),
                    None => {
                        let _ = fs::remove_file(&p);
                        None
                    }
                }
            })
            .collect();
        v.sort_by_key(|e| e.0);
        v
    }

    /// Drop least recently used entries (and the blobs only they used) until the store fits its
    /// cap. Caller holds the lock.
    fn evict_locked(&self) -> Freed {
        let mut freed = Freed::default();
        if self.disk_bytes() <= self.max_bytes {
            return freed;
        }
        let mut dropped_keys = Vec::new();
        for (_, path, entry) in self.entries_by_use() {
            if self.disk_bytes() <= self.max_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                freed.entries += 1;
                dropped_keys.push(entry.key);
                let g = self.sweep_blobs_locked();
                freed.blobs += g.blobs;
                freed.bytes += g.bytes;
            }
        }
        if !dropped_keys.is_empty() {
            self.forget_keys_locked(&dropped_keys);
        }
        freed
    }

    /// Delete every blob no entry names (and stray temp files of crashed writers).
    fn sweep_blobs_locked(&self) -> Freed {
        let mut used: HashSet<String> = HashSet::new();
        for e in fs::read_dir(self.entries()).into_iter().flatten().flatten() {
            if let Some(entry) = fs::read(e.path())
                .ok()
                .and_then(|b| serde_json::from_slice::<Entry>(&b).ok())
            {
                used.extend(entry.outputs.into_iter().map(|o| o.blob));
                used.insert(entry.log_blob);
            }
        }
        let mut freed = Freed::default();
        for shard in fs::read_dir(self.blobs()).into_iter().flatten().flatten() {
            for b in fs::read_dir(shard.path()).into_iter().flatten().flatten() {
                let name = b.file_name().to_string_lossy().into_owned();
                if used.contains(&name) {
                    continue;
                }
                let len = b.metadata().map_or(0, |m| m.len());
                if fs::remove_file(b.path()).is_ok() {
                    freed.blobs += 1;
                    freed.bytes += len;
                }
            }
        }
        freed
    }

    fn forget_keys_locked(&self, keys: &[String]) {
        let mut st = self.read_stats();
        for k in keys {
            st.keys.remove(k);
        }
        let _ = self.write_stats(&st);
    }

    /// Remove blobs no entry uses, then trim to the cap.
    pub fn gc(&self) -> Freed {
        let Ok(_lock) = self.lock() else {
            return Freed::default();
        };
        let mut freed = self.sweep_blobs_locked();
        let e = self.evict_locked();
        freed.entries += e.entries;
        freed.blobs += e.blobs;
        freed.bytes += e.bytes;
        freed
    }

    /// Empty the store (counters included).
    pub fn clear(&self) -> Freed {
        let Ok(_lock) = self.lock() else {
            return Freed::default();
        };
        let bytes = self.disk_bytes();
        let entries = fs::read_dir(self.entries()).map_or(0, Iterator::count);
        for d in [self.blobs(), self.entries()] {
            let _ = fs::remove_dir_all(&d);
        }
        for f in ["stats.json", "recent.json"] {
            let _ = fs::remove_file(self.root.join(f));
        }
        let _ = self.init();
        Freed {
            entries,
            blobs: 0,
            bytes,
        }
    }

    pub fn status(&self) -> Status {
        Status {
            dir: self.root.to_string_lossy().into_owned(),
            entries: fs::read_dir(self.entries()).map_or(0, |d| {
                d.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .count()
            }),
            bytes: self.disk_bytes(),
            max_bytes: self.max_bytes,
            stats: self.read_stats(),
        }
    }

    // ---- counters and recent runs ----

    pub fn stats(&self) -> Stats {
        self.read_stats()
    }

    fn read_stats(&self) -> Stats {
        fs::read(self.root.join("stats.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write_stats(&self, st: &Stats) -> io::Result<()> {
        crate::fs::write_atomic(
            &self.root.join("stats.json"),
            &serde_json::to_vec_pretty(st)?,
            false,
        )
    }

    fn read_recent(&self) -> Vec<Recent> {
        fs::read(self.root.join("recent.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Count a hit and remember it. Best effort: a cache that cannot count still served the hit.
    pub fn note_hit(&self, entry: &Entry, key: &Key) {
        let tail = self
            .read_log(entry)
            .map(|c| log::tail(&c))
            .unwrap_or_default();
        self.note(
            key,
            &entry.label,
            &entry.command,
            &entry.project,
            true,
            entry.exit,
            entry.duration_ms,
            tail,
        );
    }

    /// Count a miss (the command ran for real) and remember it.
    fn note_miss(&self, key: &Key, run: &Run) {
        self.note(
            key,
            &run.label,
            &run.command,
            &run.project,
            false,
            run.exit,
            run.duration_ms,
            log::tail(&run.log),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn note(
        &self,
        key: &Key,
        label: &str,
        command: &str,
        project: &str,
        hit: bool,
        exit: i32,
        duration_ms: u64,
        log_tail: String,
    ) {
        let Ok(_lock) = self.lock() else { return };
        let now = now_ms();
        let mut st = self.read_stats();
        {
            let ks = st.touch_key(key.as_str(), label, project, now);
            if hit {
                ks.hits += 1;
                ks.saved_ms += duration_ms;
            } else {
                ks.misses += 1;
            }
        }
        let proj = st.projects.entry(project.to_string()).or_default();
        if hit {
            proj.add_hit(duration_ms);
            st.total.add_hit(duration_ms);
        } else {
            proj.misses += 1;
            st.total.misses += 1;
        }
        if hit {
            st.add_hit_at(now, duration_ms);
        }
        let _ = self.write_stats(&st);
        let mut recent = self.read_recent();
        recent.push(Recent {
            label: label.to_string(),
            command: command.to_string(),
            project: project.to_string(),
            key: key.to_string(),
            hit,
            exit,
            duration_ms,
            at: now,
            log_tail,
        });
        let excess = recent.len().saturating_sub(stats::RECENT_KEEP);
        recent.drain(..excess);
        if let Ok(b) = serde_json::to_vec(&recent) {
            let _ = crate::fs::write_atomic(&self.root.join("recent.json"), &b, false);
        }
    }

    /// The most recent run (hit or miss) whose label or command line is `name`, optionally of one
    /// project.
    pub fn last_result(&self, name: &str, project: Option<&str>) -> Option<LastResult> {
        self.read_recent()
            .into_iter()
            .rev()
            .find(|r| r.matches(name, project))
            .map(|r| r.into_last(name))
    }

    /// Milliseconds of the runs of `name` (a label or a command line, optionally of one project)
    /// that really ran, oldest first: the misses. A hit only replays the original duration, so
    /// counting it would weigh one run once per hit.
    pub fn durations_ms(&self, name: &str, project: Option<&str>) -> Vec<u64> {
        self.read_recent()
            .into_iter()
            .filter(|r| !r.hit && r.matches(name, project))
            .map(|r| r.duration_ms)
            .collect()
    }
}

/// Every regular file under `path` (a file, or a directory walked whole: build outputs are
/// usually gitignored), as `(relative path, absolute path, size, mode)`. A declared output that
/// does not exist is simply not part of this run's result.
fn collect_files(
    cwd: &Path,
    path: &Path,
    out: &mut Vec<(String, PathBuf, u64, u32)>,
) -> io::Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if meta.is_dir() {
        let mut names: Vec<_> = fs::read_dir(path)?.collect::<io::Result<_>>()?;
        names.sort_by_key(fs::DirEntry::file_name);
        for e in names {
            collect_files(cwd, &e.path(), out)?;
        }
    } else if meta.is_file() {
        let rel = path.strip_prefix(cwd).unwrap_or(path);
        #[cfg(unix)]
        let mode = std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o7777;
        #[cfg(not(unix))]
        let mode = 0;
        out.push((
            rel.to_string_lossy().replace('\\', "/"),
            path.to_path_buf(),
            meta.len(),
            mode,
        ));
    }
    // Symlinks, sockets and devices are not outputs worth sharing: left out.
    Ok(())
}
