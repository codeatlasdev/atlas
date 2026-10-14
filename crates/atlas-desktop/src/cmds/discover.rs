//! Project discovery: walk likely roots (merged with Spotlight), inspect what was found and match
//! moved copies of projects that disappeared. The engine is synchronous and event-based so the
//! command streams it to the UI and the tests drive it with a plain closure.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use atlas_tui::config;
use serde::Serialize;

/// Directories never worth descending into (dependencies, build output, OS noise, VCS internals).
const SKIP: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    "out",
    "Pods",
    "Library",
    "Applications",
    "Movies",
    "Music",
    "Pictures",
    "Public",
    "venv",
    "__pycache__",
];

/// Folders under `$HOME` that usually hold checkouts.
const DEFAULT_ROOTS: &[&str] = &[
    "Projetos",
    "Projects",
    "Developer",
    "code",
    "src",
    "dev",
    "work",
    "repos",
    "GitHub",
    "Documents/GitHub",
];

/// A nested `atlas.yaml` inside a project (monorepo packages) is looked for this deep below it.
const NESTED_DEPTH: u32 = 3;
pub const DEFAULT_DEPTH: u32 = 4;
/// Marker files of a project that could use Atlas.
const CANDIDATE_MARKERS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

#[derive(Debug, Clone)]
pub struct ScanOpts {
    pub roots: Vec<PathBuf>,
    pub max_depth: u32,
    /// Also report git roots without `atlas.yaml` that have a project marker.
    pub candidates: bool,
}

/// What discovery learned about one folder.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub path: String,
    /// The scan root it was found under (UI grouping); empty for Spotlight-only hits.
    pub root: String,
    /// `name` of atlas.yaml, else the folder name.
    pub name: String,
    pub services: usize,
    /// Parse/validation problems of atlas.yaml (`None` = valid).
    pub error: Option<String>,
    pub branch: Option<String>,
    pub remote: Option<String>,
    /// Seconds since the epoch of the last change to atlas.yaml (or the folder).
    pub modified: Option<u64>,
    /// `atlas.yaml` exists here (`false` = a candidate that could use `atlas init`).
    pub has_config: bool,
    /// Lives inside another project's tree (monorepo package).
    pub nested: bool,
    /// Detected stack of a candidate (`node`, `rust`, `go`, `python`, `compose`).
    pub stack: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Event {
    Found(Found),
    Progress {
        dirs: u64,
        found: u64,
        elapsed_ms: u64,
        current: String,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub dirs: u64,
    /// Directories that could not be read (permissions, vanished).
    pub unreadable: u64,
}

/// Roots to scan: the user's defaults that exist, plus parents of registered projects.
pub fn default_roots(home: &Path, registered: &[String]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = DEFAULT_ROOTS.iter().map(|d| home.join(d)).collect();
    for r in registered {
        if let Some(parent) = Path::new(r).parent() {
            // A parent that is $HOME itself or `/` would make the scan the whole disk.
            if parent != home && parent.parent().is_some() {
                roots.push(parent.to_path_buf());
            }
        }
    }
    let mut seen = HashSet::new();
    roots.retain(|r| r.is_dir() && seen.insert(r.canonicalize().unwrap_or_else(|_| r.clone())));
    // A root inside another one is already covered by the walk.
    let all = roots.clone();
    roots.retain(|r| !all.iter().any(|o| o != r && r.starts_with(o)));
    roots
}

/// Resolve symlinks (`/tmp` → `/private/tmp`), drop duplicates and roots nested in another root,
/// so every result is attributed to exactly one canonical root and groups never split.
pub fn canonical_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        let Ok(r) = r.canonicalize() else { continue };
        if r.is_dir() && !out.contains(&r) {
            out.push(r);
        }
    }
    let all = out.clone();
    out.retain(|r| !all.iter().any(|o| o != r && r.starts_with(o)));
    out
}

fn skipped(name: &str) -> bool {
    name.starts_with('.') || SKIP.contains(&name)
}

/// Folders a repo keeps its own demo and test stacks in. Such a folder under a git checkout belongs to that checkout.
const FIXTURE_DIRS: &[&str] = &["demo", "examples", "fixtures", "testdata", "samples"];

/// `dir` is (or sits under) a fixture folder of a git checkout: a stack the repo ships for itself (`atlas/demo`, named
/// `orbit-shop`), not a project of the user's. The checkout is the project; a `demo/` with no checkout above it is not
/// covered, it is somebody's project.
fn in_checkout_fixture(dir: &Path) -> bool {
    dir.ancestors().skip(1).any(|up| {
        up.file_name()
            .is_some_and(|n| FIXTURE_DIRS.iter().any(|f| n == *f))
            && up
                .parent()
                .is_some_and(|p| p.ancestors().any(|a| a.join(".git").exists()))
    }) || dir
        .file_name()
        .is_some_and(|n| FIXTURE_DIRS.iter().any(|f| n == *f))
        && dir
            .parent()
            .is_some_and(|p| p.ancestors().any(|a| a.join(".git").exists()))
}

fn mtime(p: &Path) -> Option<u64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    m.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Branch from `.git/HEAD` (a worktree's `.git` is a file pointing at its gitdir).
fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot = root.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let rel = text.trim().strip_prefix("gitdir:")?.trim();
    let p = Path::new(rel);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    })
}

pub fn git_branch(root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir(root)?.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(b) => Some(b.to_string()),
        None => (head.len() >= 7).then(|| format!("{} (detached)", &head[..7])),
    }
}

/// `url` of `[remote "origin"]` from the git config (worktrees share the main repo's config).
pub fn git_remote(root: &Path) -> Option<String> {
    let gd = git_dir(root)?;
    let common = std::fs::read_to_string(gd.join("commondir"))
        .ok()
        .map(|c| gd.join(c.trim()))
        .unwrap_or(gd);
    let text = std::fs::read_to_string(common.join("config")).ok()?;
    let mut in_origin = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l.replace(' ', "") == r#"[remote"origin"]"#;
        } else if in_origin && let Some(v) = l.strip_prefix("url") {
            let v = v.trim_start().strip_prefix('=')?.trim();
            return (!v.is_empty()).then(|| v.to_string());
        }
    }
    None
}

/// `git@github.com:org/Repo.git`, `https://github.com/org/repo/` → `github.com/org/repo`.
pub fn normalize_remote(url: &str) -> String {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let u = u.split_once("://").map_or(u, |(_, rest)| rest);
    let u = u.rsplit_once('@').map_or(u, |(_, rest)| rest);
    u.replacen(':', "/", 1).to_lowercase()
}

fn stack_of(dir: &Path) -> Option<&'static str> {
    let has = |f: &str| dir.join(f).is_file();
    if has("package.json") {
        Some("node")
    } else if has("Cargo.toml") {
        Some("rust")
    } else if has("go.mod") {
        Some("go")
    } else if has("pyproject.toml") {
        Some("python")
    } else if CANDIDATE_MARKERS.iter().any(|m| has(m)) {
        Some("compose")
    } else {
        None
    }
}

/// Read a folder with an `atlas.yaml` (or, for a candidate, just its identity).
pub fn inspect(dir: &Path, root: &Path, nested: bool) -> Found {
    let folder = dir.file_name().map_or_else(
        || dir.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let has_config = dir.join("atlas.yaml").is_file();
    let (name, services, error) = if has_config {
        match config::load_checked(dir) {
            Ok((cfg, errs)) => (
                cfg.name.clone(),
                cfg.services.len(),
                (!errs.is_empty()).then(|| config::render_errors(&errs, "atlas.yaml")),
            ),
            Err(e) => (folder.clone(), 0, Some(format!("{e:#}"))),
        }
    } else {
        (folder.clone(), 0, None)
    };
    Found {
        path: dir.to_string_lossy().into_owned(),
        root: root.to_string_lossy().into_owned(),
        name,
        services,
        error,
        branch: git_branch(dir),
        remote: git_remote(dir),
        modified: mtime(&dir.join(if has_config { "atlas.yaml" } else { "." })),
        has_config,
        nested,
        stack: if has_config {
            None
        } else {
            stack_of(dir).map(str::to_string)
        },
    }
}

struct Walk<'a> {
    opts: &'a ScanOpts,
    cancel: &'a AtomicBool,
    emit: &'a mut dyn FnMut(Event),
    stats: Stats,
    found: u64,
    seen: HashSet<PathBuf>,
    start: Instant,
    last_progress: Instant,
}

impl Walk<'_> {
    fn progress(&mut self, dir: &Path, force: bool) {
        if !force && self.last_progress.elapsed() < Duration::from_millis(120) {
            return;
        }
        self.last_progress = Instant::now();
        (self.emit)(Event::Progress {
            dirs: self.stats.dirs,
            found: self.found,
            elapsed_ms: self.start.elapsed().as_millis() as u64,
            current: dir.to_string_lossy().into_owned(),
        });
    }

    fn report(&mut self, f: Found) {
        // The same folder can come from two overlapping roots.
        let key = PathBuf::from(&f.path);
        if !self.seen.insert(key) {
            return;
        }
        if f.has_config {
            self.found += 1;
        }
        (self.emit)(Event::Found(f));
    }

    fn walk(&mut self, dir: &Path, root: &Path, depth: u32) {
        if self.cancel.load(Ordering::Relaxed) {
            return;
        }
        self.stats.dirs += 1;
        self.progress(dir, false);
        if in_checkout_fixture(dir) {
            return;
        }
        if dir.join("atlas.yaml").is_file() {
            self.report(inspect(dir, root, false));
            // Stop here: the project is the unit. Nested configs (monorepo packages) are
            // still reported, from a shallow bounded look below it.
            self.nested(dir, root, 0);
            return;
        }
        // A git root with a project marker could use Atlas; keep walking, since a package
        // below it may have its own atlas.yaml.
        if self.opts.candidates && dir.join(".git").exists() && stack_of(dir).is_some() {
            self.report(inspect(dir, root, false));
        }
        if depth >= self.opts.max_depth {
            return;
        }
        for sub in self.subdirs(dir) {
            self.walk(&sub, root, depth + 1);
        }
    }

    fn nested(&mut self, dir: &Path, root: &Path, depth: u32) {
        if depth >= NESTED_DEPTH || self.cancel.load(Ordering::Relaxed) {
            return;
        }
        for sub in self.subdirs(dir) {
            self.stats.dirs += 1;
            if sub.join("atlas.yaml").is_file() {
                self.report(inspect(&sub, root, true));
            } else {
                self.nested(&sub, root, depth + 1);
            }
        }
    }

    /// Real subdirectories, sorted, never following symlinks (so a symlink loop cannot trap the
    /// walk) and never the skip list. Unreadable directories are counted, not fatal.
    fn subdirs(&mut self, dir: &Path) -> Vec<PathBuf> {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => {
                self.stats.unreadable += 1;
                return Vec::new();
            }
        };
        let mut out: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| !skipped(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        out.sort();
        out
    }
}

/// `scan_skipping` with nothing to skip.
#[cfg(test)]
pub fn scan(opts: &ScanOpts, cancel: &AtomicBool, emit: &mut dyn FnMut(Event)) -> Stats {
    scan_skipping(opts, cancel, &HashSet::new(), emit)
}

/// Walk `opts.roots`, calling `emit` as projects are found, without re-reporting folders in
/// `already` (e.g. found by Spotlight first). Returns what the walk covered.
pub fn scan_skipping(
    opts: &ScanOpts,
    cancel: &AtomicBool,
    already: &HashSet<PathBuf>,
    emit: &mut dyn FnMut(Event),
) -> Stats {
    let now = Instant::now();
    let mut w = Walk {
        opts,
        cancel,
        emit,
        stats: Stats::default(),
        found: 0,
        seen: already.clone(),
        start: now,
        last_progress: now - Duration::from_secs(1),
    };
    for root in &opts.roots {
        let root = root.canonicalize().unwrap_or_else(|_| root.clone());
        w.walk(&root, &root, 0);
    }
    let last = opts.roots.last().cloned().unwrap_or_default();
    w.progress(&last, true);
    w.stats
}

/// Folders with an `atlas.yaml` that Spotlight knows about. Empty when Spotlight is off or slow.
pub fn spotlight(home: &Path, timeout: Duration) -> Vec<PathBuf> {
    let Ok(mut child) = std::process::Command::new("mdfind")
        .args(["-onlyin"])
        .arg(home)
        .args(["-name", "atlas.yaml"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let Some(mut out) = child.stdout.take() else {
        return Vec::new();
    };
    // Read on a thread so the timeout is real even if mdfind hangs.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut out, &mut s);
        let _ = tx.send(s);
    });
    let text = match rx.recv_timeout(timeout) {
        Ok(t) => t,
        Err(_) => {
            let _ = child.kill();
            return Vec::new();
        }
    };
    let _ = child.wait();
    parse_mdfind(&text)
        .into_iter()
        .map(|d| d.canonicalize().unwrap_or(d))
        .collect()
}

/// `mdfind` paths of `atlas.yaml` → their project folders, without noise folders.
pub fn parse_mdfind(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(Path::new)
        .filter(|p| p.file_name().is_some_and(|n| n == "atlas.yaml"))
        .filter_map(Path::parent)
        .filter(|d| !in_checkout_fixture(d))
        .filter(|d| {
            !d.components().any(|c| {
                let c = c.as_os_str().to_string_lossy();
                c != "/" && skipped(&c)
            })
        })
        .map(Path::to_path_buf)
        .collect()
}

/// Which of the `found` projects is the moved copy of a missing one: same normalized `origin`
/// URL first, then the same name. `None` when nothing (or more than one candidate) fits.
pub fn match_moved<'a>(
    missing_name: &str,
    missing_remote: Option<&str>,
    found: &'a [Found],
) -> Option<&'a Found> {
    let usable = || found.iter().filter(|f| f.has_config);
    let unique = |mut it: Box<dyn Iterator<Item = &'a Found> + '_>| {
        let first = it.next()?;
        it.next().is_none().then_some(first)
    };
    if let Some(r) = missing_remote.map(normalize_remote) {
        let by_remote =
            unique(Box::new(usable().filter(|f| {
                f.remote.as_deref().map(normalize_remote).as_deref() == Some(&r)
            })));
        if by_remote.is_some() {
            return by_remote;
        }
    }
    unique(Box::new(usable().filter(|f| f.name == missing_name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(dir: &Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("atlas.yaml"),
            format!("name: {name}\nservices:\n  web:\n    command: echo hi\n    port: 3000\n"),
        )
        .unwrap();
    }

    fn run(opts: &ScanOpts) -> (Vec<Found>, Stats) {
        let mut out = Vec::new();
        let stats = scan(opts, &AtomicBool::new(false), &mut |e| {
            if let Event::Found(f) = e {
                out.push(f);
            }
        });
        (out, stats)
    }

    fn opts(root: &Path) -> ScanOpts {
        ScanOpts {
            roots: vec![root.to_path_buf()],
            max_depth: DEFAULT_DEPTH,
            candidates: false,
        }
    }

    fn names(f: &[Found]) -> Vec<&str> {
        let mut n: Vec<&str> = f.iter().map(|f| f.name.as_str()).collect();
        n.sort();
        n
    }

    #[test]
    fn finds_projects_and_reads_their_config() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("a"), "alpha");
        project(&t.path().join("group/b"), "beta");
        let (found, stats) = run(&opts(t.path()));
        assert_eq!(names(&found), ["alpha", "beta"]);
        assert_eq!(found[0].services, 1);
        assert!(
            found
                .iter()
                .all(|f| f.error.is_none() && f.has_config && !f.nested)
        );
        assert!(stats.dirs >= 3);
    }

    #[test]
    fn skip_list_and_hidden_dirs_are_not_entered() {
        let t = tempfile::tempdir().unwrap();
        for d in [
            "node_modules/x",
            "target/x",
            "vendor/x",
            ".git/x",
            "dist/x",
            "Library/x",
            ".hidden/x",
        ] {
            project(&t.path().join(d), "ghost");
        }
        project(&t.path().join("real"), "real");
        assert_eq!(names(&run(&opts(t.path())).0), ["real"]);
    }

    #[test]
    fn depth_limit_is_respected() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("a/b"), "shallow");
        project(&t.path().join("x/y/z/w/v"), "deep");
        let mut o = opts(t.path());
        o.max_depth = 2;
        assert_eq!(names(&run(&o).0), ["shallow"]);
        o.max_depth = 5;
        assert_eq!(names(&run(&o).0), ["deep", "shallow"]);
    }

    #[test]
    fn stops_at_a_project_but_reports_monorepo_packages_as_nested() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("mono"), "mono");
        project(&t.path().join("mono/packages/api"), "api");
        // Deeper than the nested look: not reported.
        project(&t.path().join("mono/a/b/c/d"), "too-deep");
        // A node_modules copy inside the project is noise.
        project(&t.path().join("mono/node_modules/pkg"), "pkg");
        let (found, _) = run(&opts(t.path()));
        assert_eq!(names(&found), ["api", "mono"]);
        let api = found.iter().find(|f| f.name == "api").unwrap();
        assert!(api.nested);
        assert!(!found.iter().find(|f| f.name == "mono").unwrap().nested);
    }

    // Incident (owner's tray, v1.5.0-alpha.7): "orbit-shop" sat in the real project list. It is `atlas/demo`, the
    // repo's own demo stack (`name: orbit-shop`), found by "Find projects" and added with the rest. A fixture folder
    // of a git checkout is part of that repo, not a project of its own; the checkout is the project.
    fn checkout(dir: &Path) {
        fs::create_dir_all(dir.join(".git")).unwrap();
    }

    #[test]
    fn a_fixture_folder_inside_a_git_checkout_is_not_offered_as_a_project() {
        for fixture in ["demo", "examples", "fixtures", "testdata", "samples"] {
            let t = tempfile::tempdir().unwrap();
            checkout(&t.path().join("atlas"));
            project(
                &t.path().join("atlas").join(fixture).join("shop"),
                "orbit-shop",
            );
            project(&t.path().join("atlas").join(fixture), "orbit-shop-direct");
            project(&t.path().join("real"), "real");
            assert_eq!(names(&run(&opts(t.path())).0), ["real"], "{fixture}");
        }
    }

    #[test]
    fn the_checkout_itself_and_a_package_named_like_a_fixture_outside_a_checkout_are_still_found() {
        let t = tempfile::tempdir().unwrap();
        // the checkout has its own atlas.yaml: it is the project, its demo/ is nested, flagged, and not a project root
        checkout(&t.path().join("shop"));
        project(&t.path().join("shop"), "shop");
        // a folder called demo that is NOT inside a checkout is somebody's project
        project(&t.path().join("demo"), "standalone-demo");
        let (found, _) = run(&opts(t.path()));
        assert_eq!(names(&found), ["shop", "standalone-demo"]);
    }

    #[test]
    fn spotlight_hits_inside_a_checkout_fixture_folder_are_dropped_too() {
        // not `.tmpXXXX`: a hidden component would be dropped by the noise filter and hide what this test is about
        let t = tempfile::Builder::new().prefix("scan").tempdir().unwrap();
        checkout(&t.path().join("atlas"));
        fs::create_dir_all(t.path().join("atlas/demo")).unwrap();
        fs::write(t.path().join("atlas/demo/atlas.yaml"), "name: orbit-shop\n").unwrap();
        project(&t.path().join("real"), "real");
        let out = format!(
            "{}/atlas/demo/atlas.yaml\n{}/real/atlas.yaml\n",
            t.path().display(),
            t.path().display()
        );
        assert_eq!(parse_mdfind(&out), [t.path().join("real")]);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loops_and_symlinked_dirs_do_not_trap_or_duplicate() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("a"), "alpha");
        fs::create_dir_all(t.path().join("loop")).unwrap();
        std::os::unix::fs::symlink(t.path(), t.path().join("loop/back")).unwrap();
        std::os::unix::fs::symlink(t.path().join("a"), t.path().join("alias")).unwrap();
        let (found, _) = run(&opts(t.path()));
        assert_eq!(names(&found), ["alpha"]);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_directories_are_counted_not_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("ok"), "ok");
        let locked = t.path().join("locked");
        fs::create_dir_all(locked.join("inner")).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let (found, stats) = run(&opts(t.path()));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(names(&found), ["ok"]);
        // Running as root can read it anyway; otherwise it is reported as unreadable.
        assert!(stats.unreadable == 1 || fs::read_dir(&locked).is_ok());
    }

    #[test]
    fn invalid_config_is_reported_not_dropped() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("bad")).unwrap();
        fs::write(t.path().join("bad/atlas.yaml"), "name: [unclosed\n").unwrap();
        let (found, _) = run(&opts(t.path()));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "bad");
        assert!(found[0].error.is_some());
        assert_eq!(found[0].services, 0);
    }

    #[test]
    fn cancel_stops_the_walk() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..5 {
            project(&t.path().join(format!("p{i}")), "p");
        }
        let cancel = AtomicBool::new(true);
        let mut n = 0;
        scan(&opts(t.path()), &cancel, &mut |e| {
            n += matches!(e, Event::Found(_)) as u32
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn progress_is_emitted_at_the_end() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("a"), "alpha");
        let mut last = None;
        scan(&opts(t.path()), &AtomicBool::new(false), &mut |e| {
            if let Event::Progress { dirs, found, .. } = e {
                last = Some((dirs, found));
            }
        });
        let (dirs, found) = last.unwrap();
        assert!(dirs >= 1);
        assert_eq!(found, 1);
    }

    #[test]
    fn candidates_are_git_roots_with_a_marker_and_no_config() {
        let t = tempfile::tempdir().unwrap();
        let web = t.path().join("web");
        fs::create_dir_all(web.join(".git")).unwrap();
        fs::write(web.join("package.json"), "{}").unwrap();
        // A git root with nothing recognizable, and a plain folder with a marker.
        fs::create_dir_all(t.path().join("notes/.git")).unwrap();
        fs::create_dir_all(t.path().join("loose")).unwrap();
        fs::write(t.path().join("loose/package.json"), "{}").unwrap();
        project(&t.path().join("real"), "real");

        let (without, _) = run(&opts(t.path()));
        assert_eq!(names(&without), ["real"]);

        let mut o = opts(t.path());
        o.candidates = true;
        let (with, _) = run(&o);
        assert_eq!(names(&with), ["real", "web"]);
        let c = with.iter().find(|f| f.name == "web").unwrap();
        assert!(!c.has_config);
        assert_eq!(c.stack.as_deref(), Some("node"));
    }

    #[test]
    fn overlapping_roots_report_a_project_once() {
        let t = tempfile::tempdir().unwrap();
        project(&t.path().join("a/b"), "alpha");
        let o = ScanOpts {
            roots: vec![t.path().to_path_buf(), t.path().join("a")],
            max_depth: DEFAULT_DEPTH,
            candidates: false,
        };
        assert_eq!(run(&o).0.len(), 1);
    }

    #[test]
    fn canonical_roots_resolve_symlinks_and_drop_duplicates_and_nested() {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("real");
        fs::create_dir_all(real.join("inner")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, t.path().join("link")).unwrap();
        let roots = canonical_roots(vec![
            t.path().join("link"),
            real.clone(),
            real.join("inner"),
            t.path().join("missing"),
        ]);
        assert_eq!(roots, vec![real.canonicalize().unwrap()]);
    }

    #[test]
    fn default_roots_keep_existing_dirs_add_parents_and_dedupe() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path();
        fs::create_dir_all(home.join("Projetos")).unwrap();
        fs::create_dir_all(home.join("Documents/GitHub")).unwrap();
        fs::create_dir_all(home.join("elsewhere/deep")).unwrap();
        let reg = vec![
            home.join("Projetos/a").to_string_lossy().into_owned(),
            home.join("elsewhere/deep/b").to_string_lossy().into_owned(),
            // Parent is $HOME: must not become a root.
            home.join("toplevel").to_string_lossy().into_owned(),
        ];
        let roots = default_roots(home, &reg);
        assert!(roots.contains(&home.join("Projetos")));
        assert!(roots.contains(&home.join("Documents/GitHub")));
        assert!(roots.contains(&home.join("elsewhere/deep")));
        assert!(!roots.contains(&home.to_path_buf()));
        assert_eq!(
            roots
                .iter()
                .filter(|r| **r == home.join("Projetos"))
                .count(),
            1
        );
        // Nonexistent defaults are dropped.
        assert!(!roots.contains(&home.join("Developer")));
    }

    #[test]
    fn git_branch_and_remote_come_from_the_files() {
        let t = tempfile::tempdir().unwrap();
        let g = t.path().join(".git");
        fs::create_dir_all(&g).unwrap();
        fs::write(g.join("HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        fs::write(
            g.join("config"),
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:other/x.git\n[remote \"origin\"]\n\turl = git@github.com:Org/Repo.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n",
        )
        .unwrap();
        assert_eq!(git_branch(t.path()).as_deref(), Some("feat/x"));
        assert_eq!(
            git_remote(t.path()).as_deref(),
            Some("git@github.com:Org/Repo.git")
        );
        fs::write(g.join("HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(git_branch(t.path()).as_deref(), Some("0123456 (detached)"));
        assert_eq!(git_remote(&t.path().join("nope")), None);
    }

    #[test]
    fn worktree_gitdir_files_resolve_branch_and_shared_remote() {
        let t = tempfile::tempdir().unwrap();
        let main = t.path().join("main/.git");
        let wt_git = main.join("worktrees/w");
        fs::create_dir_all(&wt_git).unwrap();
        fs::write(
            main.join("config"),
            "[remote \"origin\"]\n\turl = https://h/o/r\n",
        )
        .unwrap();
        fs::write(wt_git.join("HEAD"), "ref: refs/heads/w\n").unwrap();
        fs::write(wt_git.join("commondir"), "../..\n").unwrap();
        let wt = t.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        assert_eq!(git_branch(&wt).as_deref(), Some("w"));
        assert_eq!(git_remote(&wt).as_deref(), Some("https://h/o/r"));
    }

    #[test]
    fn remotes_normalize_across_ssh_https_and_case() {
        let n = normalize_remote;
        assert_eq!(n("git@github.com:Org/Repo.git"), "github.com/org/repo");
        assert_eq!(n("https://github.com/org/repo/"), "github.com/org/repo");
        assert_eq!(
            n("https://user:tok@github.com/org/repo.git"),
            "github.com/org/repo"
        );
        assert_eq!(n("ssh://git@github.com/org/repo"), "github.com/org/repo");
    }

    fn found(name: &str, path: &str, remote: Option<&str>) -> Found {
        Found {
            path: path.into(),
            root: String::new(),
            name: name.into(),
            services: 0,
            error: None,
            branch: None,
            remote: remote.map(Into::into),
            modified: None,
            has_config: true,
            nested: false,
            stack: None,
        }
    }

    #[test]
    fn moved_copy_matches_by_remote_before_name() {
        let list = vec![
            found(
                "renamed",
                "/new/renamed",
                Some("git@github.com:org/app.git"),
            ),
            found("app", "/other/app", Some("git@github.com:someone/else.git")),
        ];
        // Same remote wins even though another folder has the same name.
        let m = match_moved("app", Some("https://github.com/org/app"), &list).unwrap();
        assert_eq!(m.path, "/new/renamed");
    }

    #[test]
    fn moved_copy_falls_back_to_unique_name_and_refuses_ambiguity() {
        let one = vec![
            found("app", "/x/app", None),
            found("other", "/x/other", None),
        ];
        assert_eq!(match_moved("app", None, &one).unwrap().path, "/x/app");
        let two = vec![found("app", "/x/app", None), found("app", "/y/app", None)];
        assert!(match_moved("app", None, &two).is_none());
        assert!(match_moved("gone", None, &one).is_none());
        // Candidates without atlas.yaml are never offered as a relink target.
        let mut cand = found("app", "/c/app", None);
        cand.has_config = false;
        assert!(match_moved("app", None, &[cand]).is_none());
    }

    #[test]
    fn mdfind_output_becomes_project_dirs_without_noise() {
        let out = "/Users/me/Projetos/a/atlas.yaml\n/Users/me/Projetos/b/node_modules/x/atlas.yaml\n\
                   /Users/me/Library/Caches/z/atlas.yaml\n/Users/me/Projetos/c/atlas.yaml.bak\n\
                   /Users/me/Projetos/d/atlas.yaml\n";
        let dirs = parse_mdfind(out);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/Users/me/Projetos/a"),
                PathBuf::from("/Users/me/Projetos/d")
            ]
        );
    }

    /// Studio records onboarding in a scratch HOME so "found on this Mac" never lists the user's real projects.
    /// A decoy "real home" with a project sits next to the scratch HOME; the walk over the scratch HOME's default
    /// roots must report only the scratch project, and Spotlight (real `mdfind`, scoped by `-onlyin`) nothing outside it.
    #[test]
    fn discovery_in_a_scratch_home_finds_its_own_projects_and_never_the_real_homes() {
        let base = tempfile::tempdir().unwrap();
        let base = base.path().canonicalize().unwrap();
        let (real, scratch) = (base.join("real-home"), base.join("scratch-home"));
        for (home, name) in [(&real, "secret"), (&scratch, "mine")] {
            let dir = home.join("Projetos").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("atlas.yaml"), format!("name: {name}\n")).unwrap();
        }

        let opts = ScanOpts {
            roots: canonical_roots(default_roots(&scratch, &[])),
            max_depth: DEFAULT_DEPTH,
            candidates: false,
        };
        let mut found = Vec::new();
        scan(&opts, &AtomicBool::new(false), &mut |ev| {
            if let Event::Found(f) = ev {
                found.push(f.name);
            }
        });
        assert_eq!(
            found,
            ["mine"],
            "the scratch HOME's own project, and only that"
        );

        for dir in spotlight(&scratch, Duration::from_secs(10)) {
            assert!(
                dir.starts_with(&scratch),
                "{dir:?} is outside the scratch HOME"
            );
        }
    }
}
