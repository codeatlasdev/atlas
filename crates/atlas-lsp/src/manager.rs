//! Owns every language-server instance: one per (server, root), shared by all open documents
//! of that root. Lazy start on first open, refcount, idle shutdown, crash restart with backoff.
//!
//! No polling: every transition is driven by a document command, a server event, or a single
//! armed timer (idle shutdown / restart backoff).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::caps;
use crate::install::{self, Failure, Installer, Launch, Source};
use crate::registry::{self, Server};
use crate::rpc::{Peer, RpcError};
use crate::server::{self, Event, Live, Shared};
use crate::text;

/// What the host forwards to its clients.
#[derive(Debug)]
pub enum Push {
    Diagnostics {
        path: String,
        version: Option<i64>,
        diagnostics: Value,
    },
    /// Full server list, sent whenever any state/progress/message changes.
    Status(Vec<ServerStatus>),
    ApplyEdit {
        id: String,
        edit: Value,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ServerStatus {
    pub name: String,
    /// Human name (`TypeScript`, `rust-analyzer`…) for UI.
    pub label: String,
    pub root: String,
    pub language: String,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Absolute path of this server's stderr log.
    pub log: String,
    /// Last stderr lines, for `crashed`/`missing`: why it died without opening the log.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tail: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<Failure>,
}

/// Per-language launch customization from the user's settings (`lsp.<language>` in
/// `settings.json` / `.atlas/settings.json`); sent by the client with every `lsp_open`.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct Overrides {
    /// Replaces the registry's server: run this program (looked up on `PATH`) instead.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub initialization_options: Option<Value>,
    /// Answers `workspace/configuration` (merged over Atlas's defaults) and is pushed with
    /// `workspace/didChangeConfiguration`.
    #[serde(default)]
    pub settings: Option<Value>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Downloading,
    Starting,
    Ready,
    Crashed,
    Missing,
}

/// Tunables; tests shrink them.
#[derive(Debug, Clone)]
pub struct Config {
    pub idle_shutdown: Duration,
    pub init_timeout: Duration,
    /// Restarts allowed inside `restart_window` before the server is left `crashed`.
    pub max_restarts: usize,
    pub restart_window: Duration,
    pub restart_backoff: Duration,
    /// Per-request timeout; a hung server must not hold a client call forever.
    pub request_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            idle_shutdown: Duration::from_secs(300),
            init_timeout: Duration::from_secs(120),
            max_restarts: 3,
            restart_window: Duration::from_secs(60),
            restart_backoff: Duration::from_millis(500),
            request_timeout: Duration::from_secs(60),
        }
    }
}

struct Doc {
    inst: Arc<Inst>,
    language: String,
    version: i64,
    text: String,
}

/// How the server wants document changes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DocSync {
    None,
    Full,
    Incremental,
}

fn sync_kind(capabilities: &Value) -> DocSync {
    let ts = &capabilities["textDocumentSync"];
    let kind = ts
        .as_i64()
        .or_else(|| ts.get("change").and_then(Value::as_i64));
    match kind {
        Some(1) => DocSync::Full,
        Some(2) => DocSync::Incremental,
        Some(_) => DocSync::None,
        // Absent: servers that send nothing expect full text.
        None => DocSync::Full,
    }
}

fn wants_save_text(capabilities: &Value) -> bool {
    capabilities["textDocumentSync"]["save"]["includeText"] == true
}

enum Phase {
    Downloading,
    Starting,
    Ready(Arc<Live>),
    Crashed(Failure),
    Missing(Failure),
}

fn crashed(message: impl Into<String>) -> Phase {
    Phase::Crashed(Failure::new("crashed", message))
}

struct InstState {
    phase: Phase,
    /// Open documents (paths) using this instance.
    docs: Vec<String>,
    restarts: VecDeque<Instant>,
    idle_gen: u64,
    /// How the running (or last started) process was launched.
    launch: Option<(Source, Option<String>)>,
}

pub struct Inst {
    server: &'static Server,
    root: String,
    language: String,
    gen_id: u64,
    overrides: Overrides,
    log: PathBuf,
    st: Mutex<InstState>,
    shared: Arc<Shared>,
    /// Bumped on every phase change; `open` waits on it.
    phase_changed: tokio::sync::watch::Sender<u64>,
}

impl Inst {
    fn status(&self) -> ServerStatus {
        let st = self.st.lock();
        let (state, message, reason) = match &st.phase {
            Phase::Downloading => (State::Downloading, None, None),
            Phase::Starting => (State::Starting, None, None),
            Phase::Ready(_) => (State::Ready, self.shared.message.lock().clone(), None),
            Phase::Crashed(f) => (State::Crashed, Some(f.message.clone()), Some(f.clone())),
            Phase::Missing(f) => (State::Missing, Some(f.message.clone()), Some(f.clone())),
        };
        let tail = match state {
            State::Crashed | State::Missing => log_tail(&self.log, TAIL_LINES),
            _ => vec![],
        };
        let (source, version) = match &st.launch {
            Some((s, v)) => (Some(*s), v.clone()),
            None => (None, None),
        };
        let progress = match &st.phase {
            Phase::Downloading | Phase::Starting | Phase::Ready(_) => self.shared.progress_text(),
            _ => None,
        };
        ServerStatus {
            name: self.server.name.into(),
            label: registry::server_label(self.server).into(),
            root: self.root.clone(),
            language: self.language.clone(),
            state,
            progress,
            message,
            version,
            source,
            log: self.log.to_string_lossy().into_owned(),
            tail,
            reason,
        }
    }

    fn live(&self) -> Option<Arc<Live>> {
        match &self.st.lock().phase {
            Phase::Ready(l) => Some(l.clone()),
            _ => None,
        }
    }

    fn set_phase(&self, p: Phase) {
        self.st.lock().phase = p;
        self.phase_changed.send_modify(|n| *n += 1);
    }
}

/// Resolves a server to a launch command. Production: [`Installer`]; tests: a fixed binary.
pub trait Resolver: Send + Sync + 'static {
    fn resolve<'a>(
        &'a self,
        server: &'static Server,
        progress: &'a dyn install::Progress,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Launch>> + Send + 'a>>;

    /// Drop any cached resolution of `server` (after an uninstall).
    fn forget(&self, _server: &'static Server) {}
}

pub struct DefaultResolver {
    pub base: PathBuf,
    pub installer: Installer,
}

/// One server in `lsp_catalog`: what Atlas can run and what it finds on this machine.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogEntry {
    pub name: &'static str,
    pub label: &'static str,
    /// LSP language ids it serves (settings keys).
    pub languages: Vec<&'static str>,
    /// The desktop reads this as `languageLabels` (`lib/lsp/types.ts`); pinned by
    /// `catalog_json_uses_the_field_names_the_desktop_reads`.
    #[serde(rename = "languageLabels")]
    pub language_labels: Vec<&'static str>,
    /// `github` | `go` | `npm`
    pub install: &'static str,
    pub version: &'static str,
    pub detected: install::Detected,
}

impl Resolver for DefaultResolver {
    fn resolve<'a>(
        &'a self,
        server: &'static Server,
        progress: &'a dyn install::Progress,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Launch>> + Send + 'a>> {
        Box::pin(self.installer.resolve(&self.base, server, progress))
    }

    fn forget(&self, server: &'static Server) {
        self.installer.forget(server);
    }
}

pub struct Manager {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: Config,
    base: PathBuf,
    resolver: Arc<dyn Resolver>,
    push: mpsc::Sender<Push>,
    events: mpsc::Sender<(u64, Event)>,
    insts: Mutex<HashMap<(String, String), Arc<Inst>>>,
    docs: Mutex<HashMap<String, Doc>>,
    next_gen: AtomicU64,
    /// Request ids chosen by clients (`rid`) → (peer, server request id), for `lsp_cancel`.
    cancels: Mutex<HashMap<String, (Peer, i64)>>,
    edit_acks: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// Explicit installs in flight (server name → progress text), see `Manager::install`.
    installing: Mutex<HashMap<String, String>>,
    closed: std::sync::atomic::AtomicBool,
}

pub struct OpenReply {
    pub server: Option<&'static str>,
    pub status: &'static str,
    pub capabilities: Value,
    /// Why the server is unavailable (`missing`/`crashed`).
    pub reason: Option<Failure>,
    /// For `disabled` (no server for the language): the languages that do have one.
    pub supported: Vec<&'static str>,
}

impl OpenReply {
    fn none(server: Option<&'static str>, status: &'static str) -> OpenReply {
        OpenReply {
            server,
            status,
            capabilities: Value::Null,
            reason: None,
            supported: vec![],
        }
    }
}

/// Key of an instance: the registry name, or a distinct one for a user-supplied command.
fn instance_name(server: &Server, o: &Overrides) -> String {
    match &o.command {
        Some(c) => format!("{}#{c} {}", server.name, o.args.join(" ")),
        None => server.name.to_string(),
    }
}

/// Resolve the user's own server command: an absolute path, or a name looked up on `PATH`
/// (plus the usual tool dirs). Never installs anything.
fn custom_launch(cmd: &str, args: &[String]) -> std::result::Result<Launch, Failure> {
    let program = if cmd.contains('/') {
        let p = PathBuf::from(cmd);
        p.is_file().then_some(p)
    } else {
        install::which_in(cmd, &install::extra_bin_dirs())
    };
    match program {
        Some(p) => Ok(Launch::custom(p, args.to_vec())),
        None => Err(Failure::new(
            "custom_not_found",
            format!("The custom language server command `{cmd}` was not found"),
        )
        .hint("Check the command in Settings → Editor → Languages (or .atlas/settings.json); it must be an executable on PATH or an absolute path.")),
    }
}

/// Stderr lines kept in a status for a dead server.
const TAIL_LINES: usize = 12;

/// Last `n` non-empty lines of `log` (the server's stderr), reading only the file's tail.
pub fn log_tail(log: &Path, n: usize) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    const WINDOW: u64 = 16 * 1024;
    let Ok(mut f) = std::fs::File::open(log) else {
        return vec![];
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if f.seek(SeekFrom::Start(len.saturating_sub(WINDOW))).is_err() {
        return vec![];
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return vec![];
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    // A window starting mid-file may begin with a partial line.
    if len > WINDOW && !lines.is_empty() {
        lines.remove(0);
    }
    let from = lines.len().saturating_sub(n);
    lines.split_off(from)
}

impl Manager {
    /// `push` receives everything for the clients; it is bounded, so a stalled client costs
    /// dropped diagnostics rather than memory.
    pub fn new(base: &Path, cfg: Config, push: mpsc::Sender<Push>) -> Manager {
        let resolver = Arc::new(DefaultResolver {
            base: base.to_path_buf(),
            installer: Installer::default(),
        });
        Self::with_resolver(base, cfg, push, resolver)
    }

    pub fn with_resolver(
        base: &Path,
        cfg: Config,
        push: mpsc::Sender<Push>,
        resolver: Arc<dyn Resolver>,
    ) -> Manager {
        let (events, rx) = mpsc::channel(1024);
        let inner = Arc::new(Inner {
            cfg,
            base: base.to_path_buf(),
            resolver,
            push,
            events,
            insts: Mutex::default(),
            docs: Mutex::default(),
            next_gen: AtomicU64::new(1),
            cancels: Mutex::default(),
            edit_acks: Mutex::default(),
            installing: Mutex::default(),
            closed: Default::default(),
        });
        tokio::spawn(event_loop(Arc::downgrade(&inner), rx));
        Manager { inner }
    }

    pub fn status(&self) -> Vec<ServerStatus> {
        self.inner.statuses()
    }

    /// Every server Atlas knows, with what is installed (PATH or managed) on this machine.
    /// Never downloads or runs an installer.
    pub async fn catalog(&self) -> Vec<CatalogEntry> {
        let mut out = Vec::new();
        for s in registry::SERVERS {
            let mut languages: Vec<&'static str> = s.languages.to_vec();
            languages.dedup();
            // Exactly one label per language id, same order: the UI pairs them by index.
            let labels: Vec<&'static str> = languages
                .iter()
                .map(|l| registry::language_label(l))
                .collect();
            out.push(CatalogEntry {
                name: s.name,
                label: registry::server_label(s),
                languages,
                language_labels: labels,
                install: match s.install {
                    registry::Install::GithubGz => "github",
                    registry::Install::GoInstall => "go",
                    registry::Install::Npm => "npm",
                },
                version: s.version,
                detected: install::detect(&self.inner.base, s).await,
            });
        }
        out
    }

    /// Install (or reinstall) a managed server now, reporting progress through the normal
    /// status push. Unlike an `open`, it does not need a document.
    pub async fn install(&self, name: &str, reinstall: bool) -> Result<(), Failure> {
        let s = registry::by_name(name)
            .ok_or_else(|| Failure::new("install_failed", format!("unknown server {name}")))?;
        if reinstall {
            // Stops the running instances (they restart on the next open) and drops the copy.
            self.uninstall(name)
                .await
                .map_err(|e| Failure::from_install(name, &e))?;
        }
        let inner = self.inner.clone();
        inner.set_installing(name, Some("Installing".to_string()));
        let me = inner.clone();
        let n = name.to_string();
        let progress = move |m: &str| me.set_installing(&n, Some(m.to_string()));
        let res = inner
            .resolver
            .resolve(s, &progress)
            .await
            .map(|_| ())
            .map_err(|e| Failure::from_install(s.name, &e));
        inner.set_installing(name, None);
        res
    }

    /// Remove Atlas's managed copy of a server (stops its instances first). PATH binaries stay.
    pub async fn uninstall(&self, name: &str) -> Result<bool> {
        let s = registry::by_name(name).ok_or_else(|| anyhow!("unknown server {name}"))?;
        self.stop_instances(name).await;
        self.inner.resolver.forget(s);
        install::uninstall(&self.inner.base, s)
    }

    /// Stop and start again the instances of `name` (all roots, or only `root`); documents
    /// stay open and are replayed. Resets the crash budget.
    pub async fn restart(&self, name: &str, root: Option<&str>) -> bool {
        let insts: Vec<Arc<Inst>> = self
            .inner
            .insts
            .lock()
            .values()
            .filter(|i| i.server.name == name && root.is_none_or(|r| i.root == r))
            .cloned()
            .collect();
        let hit = !insts.is_empty();
        for i in insts {
            if let Some(live) = i.live() {
                server::shutdown(&live).await;
            }
            i.st.lock().restarts.clear();
            let me = self.inner.clone();
            tokio::spawn(async move { me.bring_up(i).await });
        }
        hit
    }

    async fn stop_instances(&self, name: &str) {
        let insts: Vec<Arc<Inst>> = {
            let mut m = self.inner.insts.lock();
            let keys: Vec<_> = m
                .iter()
                .filter(|(_, i)| i.server.name == name)
                .map(|(k, _)| k.clone())
                .collect();
            keys.iter().filter_map(|k| m.remove(k)).collect()
        };
        for i in insts {
            i.st.lock().idle_gen += 1;
            if let Some(live) = i.live() {
                server::shutdown(&live).await;
            }
            i.set_phase(crashed("uninstalled"));
        }
        self.inner
            .docs
            .lock()
            .retain(|_, d| d.inst.server.name != name);
        self.inner.notify_status();
    }

    /// The last `lines` of a server's stderr log and its path.
    pub fn log(&self, name: &str, lines: usize) -> (PathBuf, String) {
        let path = install::log_dir(&self.inner.base).join(format!("{name}.log"));
        let text = log_tail(&path, lines.clamp(1, 500)).join("\n");
        (path, text)
    }

    /// Open (or re-open after a client reconnect) a document. Returns once the server is
    /// resolved and started or known to be unavailable; a download/start in progress by
    /// another document is awaited, never duplicated.
    pub async fn open(
        &self,
        path: &str,
        language_id: &str,
        text: String,
        version: i64,
        fallback_root: Option<&Path>,
    ) -> OpenReply {
        self.open_with(
            path,
            language_id,
            text,
            version,
            fallback_root,
            &Overrides::default(),
        )
        .await
    }

    /// [`Manager::open`] with the user's per-language customization (custom command,
    /// initialization options, settings, disabled).
    pub async fn open_with(
        &self,
        path: &str,
        language_id: &str,
        text: String,
        version: i64,
        fallback_root: Option<&Path>,
        overrides: &Overrides,
    ) -> OpenReply {
        let reply = self
            .open_attached(path, language_id, text, version, fallback_root, overrides)
            .await;
        // A project file just got (or joined) a server: dependency documents that were served from a
        // broader root while no project server existed move to it.
        if !registry::is_dependency(Path::new(path)) {
            self.adopt_dependencies(path, overrides).await;
        }
        reply
    }

    /// Move dependency documents of the same server that sit on an instance rooted *above* the
    /// instance of `path` (the worktree-root fallback they got at startup) onto it; an instance left
    /// without documents idles out.
    async fn adopt_dependencies(&self, path: &str, overrides: &Overrides) {
        let inner = &self.inner;
        let Some(target) = inner.docs.lock().get(path).map(|d| d.inst.clone()) else {
            return;
        };
        let name = instance_name(target.server, overrides);
        let movers: Vec<(String, String, i64, String, Arc<Inst>)> = {
            let docs = inner.docs.lock();
            docs.iter()
                .filter(|(p, d)| {
                    !Arc::ptr_eq(&d.inst, &target)
                        && instance_name(d.inst.server, &d.inst.overrides) == name
                        && registry::is_dependency(Path::new(p))
                        && Path::new(&target.root).starts_with(&d.inst.root)
                })
                .map(|(p, d)| {
                    (
                        p.clone(),
                        d.language.clone(),
                        d.version,
                        d.text.clone(),
                        d.inst.clone(),
                    )
                })
                .collect()
        };
        for (p, language, version, text, from) in movers {
            self.close_inner(&p, false).await;
            let root = PathBuf::from(&target.root);
            self.open_attached(&p, &language, text, version, Some(&root), overrides)
                .await;
            // The old instance may now be empty: let it idle out like any server whose last file closed.
            if from.st.lock().docs.is_empty() {
                inner.arm_idle(from);
            }
        }
    }

    async fn open_attached(
        &self,
        path: &str,
        language_id: &str,
        text: String,
        version: i64,
        fallback_root: Option<&Path>,
        overrides: &Overrides,
    ) -> OpenReply {
        let inner = &self.inner;
        let Some(server) = registry::for_language(language_id) else {
            return OpenReply {
                supported: registry::supported_languages(),
                ..OpenReply::none(None, "disabled")
            };
        };
        if overrides.disabled {
            return OpenReply::none(Some(server.name), "disabled");
        }
        let name = instance_name(server, overrides);
        // Third-party sources (a dependency, the std library) are browsed through the project that led
        // there: borrow its running server, else root one at the worktree that opened the file (so the
        // dependency never wins a startup race and roots a server of its own). Only with neither
        // (no worktree known, nothing running) does it get a server rooted where its markers say.
        let dependency = registry::is_dependency(Path::new(path));
        let root = dependency
            .then(|| inner.running_root(&name))
            .flatten()
            .or_else(|| {
                dependency
                    .then_some(fallback_root)
                    .flatten()
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| {
                registry::find_root(server, Path::new(path), fallback_root)
                    .to_string_lossy()
                    .into_owned()
            });
        // A custom command is a different process than the registry's: never share an instance.
        let key = (name, root.clone());
        // Re-open of an already tracked path: close the old attachment first.
        if inner.docs.lock().contains_key(path) {
            self.close_inner(path, false).await;
        }
        let (inst, fresh) = {
            let mut insts = inner.insts.lock();
            match insts.get(&key) {
                Some(i) => (i.clone(), false),
                None => {
                    let gen_id = inner.next_gen.fetch_add(1, Ordering::Relaxed);
                    let shared = Arc::new(Shared {
                        name: server.name,
                        root: root.clone(),
                        progress: Mutex::default(),
                        message: Mutex::default(),
                        settings: overrides.settings.clone(),
                        events: inner.events.clone(),
                        gen_id,
                        next_edit: AtomicU64::new(1),
                    });
                    let inst = Arc::new(Inst {
                        server,
                        root: root.clone(),
                        language: language_id.to_string(),
                        gen_id,
                        overrides: overrides.clone(),
                        log: install::log_dir(&inner.base).join(format!("{}.log", server.name)),
                        st: Mutex::new(InstState {
                            phase: Phase::Downloading,
                            docs: vec![],
                            restarts: VecDeque::new(),
                            idle_gen: 0,
                            launch: None,
                        }),
                        shared,
                        phase_changed: tokio::sync::watch::channel(0).0,
                    });
                    insts.insert(key.clone(), inst.clone());
                    (inst, true)
                }
            }
        };
        {
            let mut st = inst.st.lock();
            st.docs.push(path.to_string());
            st.idle_gen += 1; // cancels a pending idle shutdown
        }
        inner.docs.lock().insert(
            path.to_string(),
            Doc {
                inst: inst.clone(),
                language: language_id.to_string(),
                version,
                text: text.clone(),
            },
        );
        if fresh {
            inner.notify_status();
            let me = inner.clone();
            let i = inst.clone();
            tokio::spawn(async move { me.bring_up(i).await });
        }
        // Wait for the instance to settle (ready or unavailable). The guard never lives
        // across an await: the phase is read into a plain enum first.
        let mut rx = inst.phase_changed.subscribe();
        loop {
            enum Settled {
                Ready(Arc<Live>),
                Missing(Failure),
                Crashed(Failure),
                Pending,
            }
            let settled = match &inst.st.lock().phase {
                Phase::Ready(live) => Settled::Ready(live.clone()),
                Phase::Missing(f) => Settled::Missing(f.clone()),
                Phase::Crashed(f) => Settled::Crashed(f.clone()),
                Phase::Downloading | Phase::Starting => Settled::Pending,
            };
            match settled {
                Settled::Ready(live) => {
                    // `didOpen` goes out once per connection: bring-up replays documents
                    // registered before ready; this covers a doc opened on a ready server.
                    inner.did_open_if_needed(&inst, path, &live).await;
                    return OpenReply {
                        capabilities: live.capabilities.clone(),
                        ..OpenReply::none(Some(server.name), "ready")
                    };
                }
                Settled::Missing(f) => {
                    return OpenReply {
                        reason: Some(f),
                        ..OpenReply::none(Some(server.name), "missing")
                    };
                }
                Settled::Crashed(f) => {
                    return OpenReply {
                        reason: Some(f),
                        ..OpenReply::none(Some(server.name), "crashed")
                    };
                }
                Settled::Pending => {}
            }
            if rx.changed().await.is_err() {
                return OpenReply::none(Some(server.name), "crashed");
            }
        }
    }

    pub async fn change(&self, path: &str, version: i64, changes: Vec<Value>) -> Result<()> {
        let inner = &self.inner;
        let (inst, params, full_text) = {
            let mut docs = inner.docs.lock();
            let doc = docs
                .get_mut(path)
                .ok_or_else(|| anyhow!("document not open: {path}"))?;
            for c in &changes {
                if !text::apply(&mut doc.text, c) {
                    bail!("bad content change");
                }
            }
            doc.version = version;
            (doc.inst.clone(), changes, doc.text.clone())
        };
        let Some(live) = inst.live() else {
            return Ok(()); // the shadow copy is current; didOpen replays it when ready
        };
        let uri = caps::uri_from_path(path);
        let content_changes = match sync_kind(&live.capabilities) {
            DocSync::None => return Ok(()),
            DocSync::Incremental => params,
            DocSync::Full => vec![json!({"text": full_text})],
        };
        live.peer
            .notify(
                "textDocument/didChange",
                json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": content_changes}),
            )
            .await;
        Ok(())
    }

    pub async fn save(&self, path: &str, text: Option<String>) -> Result<()> {
        let inst = {
            let mut docs = self.inner.docs.lock();
            let doc = docs
                .get_mut(path)
                .ok_or_else(|| anyhow!("document not open: {path}"))?;
            if let Some(t) = &text {
                doc.text = t.clone();
            }
            doc.inst.clone()
        };
        let Some(live) = inst.live() else {
            return Ok(());
        };
        let mut params = json!({"textDocument": {"uri": caps::uri_from_path(path)}});
        if wants_save_text(&live.capabilities) {
            params["text"] = json!(
                self.inner
                    .docs
                    .lock()
                    .get(path)
                    .map(|d| d.text.clone())
                    .unwrap_or_default()
            );
        }
        live.peer.notify("textDocument/didSave", params).await;
        Ok(())
    }

    pub async fn close(&self, path: &str) {
        self.close_inner(path, true).await;
    }

    async fn close_inner(&self, path: &str, arm_idle: bool) {
        let inner = &self.inner;
        let Some(doc) = inner.docs.lock().remove(path) else {
            return;
        };
        let inst = doc.inst;
        if let Some(live) = inst.live() {
            live.opened.lock().remove(path);
            live.peer
                .notify(
                    "textDocument/didClose",
                    json!({"textDocument": {"uri": caps::uri_from_path(path)}}),
                )
                .await;
        }
        let empty = {
            let mut st = inst.st.lock();
            st.docs.retain(|d| d != path);
            st.docs.is_empty()
        };
        // Stale diagnostics of a closed doc must not linger in the client.
        let _ = inner.push.try_send(Push::Diagnostics {
            path: path.to_string(),
            version: None,
            diagnostics: json!([]),
        });
        if empty && arm_idle {
            inner.arm_idle(inst);
        }
    }

    /// Forward an LSP request for `path`'s server. `rid` lets the client cancel it later.
    pub async fn request(
        &self,
        path: &str,
        method: &str,
        params: Value,
        rid: Option<String>,
    ) -> Result<Value, RpcError> {
        let inst = self
            .inner
            .docs
            .lock()
            .get(path)
            .map(|d| d.inst.clone())
            .ok_or_else(|| RpcError::new(-32602, format!("document not open: {path}")))?;
        let live = inst
            .live()
            .ok_or_else(|| RpcError::new(-32002, "language server not ready"))?;
        if method == "$/capabilities" {
            return Ok(live.capabilities.clone());
        }
        let (id, rx) = live.peer.request_with_id(method, params).await;
        if let Some(r) = &rid {
            self.inner
                .cancels
                .lock()
                .insert(r.clone(), (live.peer.clone(), id));
        }
        let res = tokio::time::timeout(self.inner.cfg.request_timeout, rx).await;
        if let Some(r) = &rid {
            self.inner.cancels.lock().remove(r);
        }
        match res {
            Ok(r) => r.unwrap_or_else(|_| Err(RpcError::closed())),
            Err(_) => {
                live.peer.cancel(id).await;
                Err(RpcError::new(-32001, "language server request timed out"))
            }
        }
    }

    pub async fn cancel(&self, rid: &str) {
        let hit = self.inner.cancels.lock().remove(rid);
        if let Some((peer, id)) = hit {
            peer.cancel(id).await;
        }
    }

    /// The client's answer to a pushed `lsp_apply_edit`.
    pub fn apply_edit_result(&self, id: &str, applied: bool) {
        if let Some(tx) = self.inner.edit_acks.lock().remove(id) {
            let _ = tx.send(applied);
        }
    }

    /// Stop every server (hostd shutdown). Process groups are killed; nothing is handed off
    /// on `upgrade`: clients re-send `lsp_open` after reconnecting.
    pub async fn shutdown(&self) {
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        let insts: Vec<_> = self.inner.insts.lock().drain().map(|(_, v)| v).collect();
        self.inner.docs.lock().clear();
        let mut set = tokio::task::JoinSet::new();
        for i in insts {
            set.spawn(async move {
                i.st.lock().idle_gen += 1;
                if let Some(live) = i.live() {
                    server::shutdown(&live).await;
                }
                i.set_phase(crashed("shut down"));
            });
        }
        set.join_all().await;
    }

    /// Live server instances (tests, `lsp_status`).
    pub fn instance_count(&self) -> usize {
        self.inner.insts.lock().len()
    }

    pub fn open_doc_count(&self) -> usize {
        self.inner.docs.lock().len()
    }

    /// OS pids of ready servers (tests).
    pub fn pids(&self) -> Vec<u32> {
        self.inner
            .insts
            .lock()
            .values()
            .filter_map(|i| i.live().and_then(|l| l.pid))
            .collect()
    }
}

impl Inner {
    /// Root of a live instance of `name` that serves a project file (not a dependency), preferring the
    /// one with the most open documents: the server a dependency file should be opened in.
    fn running_root(&self, name: &str) -> Option<String> {
        let insts = self.insts.lock();
        insts
            .iter()
            .filter(|((n, root), i)| {
                n == name && !registry::is_dependency(Path::new(root)) && i.live().is_some()
            })
            .max_by_key(|(_, i)| i.st.lock().docs.len())
            .map(|((_, root), _)| root.clone())
    }

    fn statuses(&self) -> Vec<ServerStatus> {
        let mut v: Vec<_> = self.insts.lock().values().map(|i| i.status()).collect();
        for (name, text) in self.installing.lock().iter() {
            if v.iter()
                .any(|s| s.name == *name && s.state == State::Downloading)
            {
                continue;
            }
            if let Some(srv) = registry::by_name(name) {
                v.push(ServerStatus {
                    name: srv.name.into(),
                    label: registry::server_label(srv).into(),
                    root: String::new(),
                    language: srv.languages[0].into(),
                    state: State::Downloading,
                    progress: Some(text.clone()),
                    message: None,
                    version: Some(srv.version.into()),
                    source: Some(Source::Managed),
                    log: install::log_dir(&self.base)
                        .join(format!("{}.log", srv.name))
                        .to_string_lossy()
                        .into_owned(),
                    tail: vec![],
                    reason: None,
                });
            }
        }
        v.sort_by(|a, b| (&a.name, &a.root).cmp(&(&b.name, &b.root)));
        v
    }

    fn notify_status(&self) {
        let _ = self.push.try_send(Push::Status(self.statuses()));
    }

    /// An explicit install (`lsp_install`) in progress: shown as a `downloading` entry even
    /// when no document has an instance of that server yet.
    fn set_installing(&self, name: &str, text: Option<String>) {
        match text {
            Some(t) => {
                self.installing.lock().insert(name.to_string(), t);
            }
            None => {
                self.installing.lock().remove(name);
            }
        }
        self.notify_status();
    }

    /// Resolve → start → replay open docs. Also the restart path (after a crash).
    async fn bring_up(self: Arc<Self>, inst: Arc<Inst>) {
        inst.set_phase(Phase::Downloading);
        self.notify_status();
        let me = self.clone();
        let i2 = inst.clone();
        let progress = move |m: &str| {
            *i2.shared.progress.lock() =
                HashMap::from([("install".to_string(), (m.to_string(), String::new()))]);
            me.notify_status();
        };
        let resolved = match &inst.overrides.command {
            Some(cmd) => custom_launch(cmd, &inst.overrides.args),
            None => self
                .resolver
                .resolve(inst.server, &progress)
                .await
                .map_err(|e| Failure::from_install(inst.server.name, &e)),
        };
        let launch = match resolved {
            Ok(l) => l,
            Err(f) => {
                inst.shared.progress.lock().clear();
                inst.set_phase(Phase::Missing(f));
                self.notify_status();
                return;
            }
        };
        inst.shared.progress.lock().clear();
        inst.st.lock().launch = Some((launch.source, launch.version.clone()));
        inst.set_phase(Phase::Starting);
        self.notify_status();
        let init = inst
            .overrides
            .initialization_options
            .clone()
            .unwrap_or_else(|| caps::init_options_for(inst.server.name, &inst.root));
        match server::start(
            &launch,
            inst.shared.clone(),
            &inst.log,
            self.cfg.init_timeout,
            init,
        )
        .await
        {
            Ok(live) => {
                let live = Arc::new(live);
                // Replay documents under the lock-free snapshot, then flip to ready; a doc
                // opened concurrently sees Ready and sends its own didOpen (dedup below).
                inst.set_phase(Phase::Ready(live.clone()));
                let paths = inst.st.lock().docs.clone();
                for p in paths {
                    self.did_open_if_needed(&inst, &p, &live).await;
                }
                self.notify_status();
            }
            Err(e) => {
                inst.set_phase(Phase::Crashed(
                    Failure::new("crashed", format!("{e:#}"))
                        .hint("Open the server log from the status bar, or restart the server."),
                ));
                self.notify_status();
            }
        }
    }

    /// Send `didOpen` for `path` once per (live connection). Tracked by version-0 sentinel
    /// in `opened`, keyed by peer identity.
    async fn did_open_if_needed(&self, inst: &Arc<Inst>, path: &str, live: &Arc<Live>) {
        let (language, version, text) = {
            let docs = self.docs.lock();
            let Some(d) = docs.get(path) else { return };
            if !Arc::ptr_eq(&d.inst, inst) {
                return;
            }
            (d.language.clone(), d.version, d.text.clone())
        };
        if !live.opened.lock().insert(path.to_string()) {
            return;
        }
        live.peer
            .notify(
                "textDocument/didOpen",
                json!({"textDocument": {
                    "uri": caps::uri_from_path(path),
                    "languageId": language,
                    "version": version,
                    "text": text,
                }}),
            )
            .await;
    }

    fn arm_idle(self: &Arc<Self>, inst: Arc<Inst>) {
        let gen_at = {
            let mut st = inst.st.lock();
            st.idle_gen += 1;
            st.idle_gen
        };
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(me.cfg.idle_shutdown).await;
            let still_idle = {
                let st = inst.st.lock();
                st.idle_gen == gen_at && st.docs.is_empty()
            };
            if !still_idle || me.closed.load(Ordering::Acquire) {
                return;
            }
            me.insts
                .lock()
                .remove(&(inst.server.name.to_string(), inst.root.clone()));
            if let Some(live) = inst.live() {
                server::shutdown(&live).await;
            }
            inst.set_phase(crashed("idle"));
            me.notify_status();
        });
    }

    /// The server process went away. Restart if documents are open and the budget allows.
    async fn on_exit(self: Arc<Self>, inst: Arc<Inst>) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let was_ready = inst.live();
        if let Some(l) = &was_ready {
            // Reap: the group may still hold helpers.
            server::kill_group(l.pid);
        }
        let (retry, has_docs) = {
            let mut st = inst.st.lock();
            if st.docs.is_empty() {
                (false, false)
            } else {
                let now = Instant::now();
                while st
                    .restarts
                    .front()
                    .is_some_and(|t| now.duration_since(*t) > self.cfg.restart_window)
                {
                    st.restarts.pop_front();
                }
                let ok = st.restarts.len() < self.cfg.max_restarts;
                if ok {
                    st.restarts.push_back(now);
                }
                (ok, true)
            }
        };
        // Diagnostics from the dead process are stale.
        let docs: Vec<String> = inst.st.lock().docs.clone();
        for p in docs {
            let _ = self.push.try_send(Push::Diagnostics {
                path: p,
                version: None,
                diagnostics: json!([]),
            });
        }
        if !has_docs {
            self.insts
                .lock()
                .remove(&(inst.server.name.to_string(), inst.root.clone()));
            inst.set_phase(crashed("exited"));
            self.notify_status();
        } else if retry {
            inst.set_phase(Phase::Starting);
            inst.shared.progress.lock().clear();
            self.notify_status();
            let backoff = self.cfg.restart_backoff
                * u32::try_from(inst.st.lock().restarts.len())
                    .unwrap_or(1)
                    .max(1);
            tokio::time::sleep(backoff).await;
            self.bring_up(inst).await;
        } else {
            inst.set_phase(Phase::Crashed(
                Failure::new(
                    "crashed",
                    format!("{} crashed repeatedly", inst.server.name),
                )
                .hint("Open the server log from the status bar, then restart the server."),
            ));
            self.notify_status();
        }
    }
}

async fn event_loop(inner: std::sync::Weak<Inner>, mut rx: mpsc::Receiver<(u64, Event)>) {
    while let Some((gen_id, ev)) = rx.recv().await {
        let Some(inner) = inner.upgrade() else { return };
        let inst = inner
            .insts
            .lock()
            .values()
            .find(|i| i.gen_id == gen_id)
            .cloned();
        match ev {
            Event::Diagnostics {
                path,
                version,
                diagnostics,
            } => {
                let _ = inner.push.try_send(Push::Diagnostics {
                    path,
                    version,
                    diagnostics,
                });
            }
            Event::Status => inner.notify_status(),
            Event::ApplyEdit { id, edit, reply } => {
                inner.edit_acks.lock().insert(id.clone(), reply);
                if inner
                    .push
                    .try_send(Push::ApplyEdit {
                        id: id.clone(),
                        edit,
                    })
                    .is_err()
                {
                    // No client could take it: refuse instead of waiting out the timeout.
                    if let Some(tx) = inner.edit_acks.lock().remove(&id) {
                        let _ = tx.send(false);
                    }
                }
            }
            Event::Exited => {
                if let Some(inst) = inst {
                    // Only the current process generation counts; a replaced one is ignored.
                    if inst.live().is_some_and(|l| l.peer.is_dead()) {
                        tokio::spawn(inner.clone().on_exit(inst));
                    }
                }
            }
        }
    }
}
