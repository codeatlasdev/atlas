//! Atlas Crew for Atlas Mobile: the phone talks to this Mac only, but the crewmates live on crew
//! servers Atlas Desktop reaches over SSH (`<registry>/desktop/crew-servers.json`). This hostd
//! keeps its own connection to each of them, runs a phone's `crew_call` there, and relays every
//! push they send as `{"ev":"crew_event","server":<host>,"event":<push>}` (the phone's scopes
//! filter it like the push inside, `mobile::line_scope`).
//!
//! The connection is the desktop's (`atlas-desktop/src/remote.rs`): `$HOME` and the token read over
//! `ssh`, then `ssh -N -L <sock>:<remote hostd.sock>`. It starts when a phone first asks
//! (`crew_servers`/`crew_call`) and reconnects with backoff while the host stays in the registry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::HostClient;
use crate::paths;
use crate::server::Hub;

/// A live connection to one crew server and what keeps it open (the ssh tunnel), dropped with it.
pub struct Link {
    pub client: HostClient,
    pub keep: Option<Box<dyn Send + Sync>>,
}

type Connecting = Pin<Box<dyn Future<Output = Result<Link, String>> + Send>>;

/// How to reach a crew server: ssh by default, a local daemon in tests.
#[derive(Clone)]
pub struct Connector {
    dial: Arc<dyn Fn(String) -> Connecting + Send + Sync>,
    /// How long a `crew_call` waits for the link ([`CONNECT_WAIT`]; tests shorten it).
    wait: Duration,
}

impl Connector {
    pub fn new(dial: Arc<dyn Fn(String) -> Connecting + Send + Sync>) -> Self {
        Connector {
            dial,
            wait: CONNECT_WAIT,
        }
    }

    /// The same connector with another connect wait (a test cannot wait 20 s for a server to be down for good).
    pub fn with_wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Connector")
    }
}

#[derive(Deserialize)]
struct Entry {
    host: String,
    #[serde(default)]
    name: String,
    /// The ssh port; absent for 22 (the desktop writes it, `crew-servers.json`). Only the ssh link reads it.
    #[cfg(unix)]
    #[serde(default)]
    port: Option<u16>,
}

/// The ssh port the registry holds for `host`, `None` for the default (and when the file or entry is gone).
#[cfg(unix)]
fn port_of(registry_file: &Path, host: &str) -> Option<u16> {
    let text = std::fs::read_to_string(registry_file).ok()?;
    serde_json::from_str::<Vec<Entry>>(&text)
        .ok()?
        .into_iter()
        .find(|e| e.host == host)
        .and_then(|e| e.port)
}

struct Server {
    name: String,
    client: watch::Sender<Option<HostClient>>,
    state: Mutex<String>,
}

pub struct CrewRelay {
    registry: PathBuf,
    connector: Connector,
    servers: Mutex<HashMap<String, Arc<Server>>>,
    hub: OnceLock<Weak<Hub>>,
    /// Revoked phones whose `push_forget` has not reached every crew server yet (#363).
    pending: Mutex<Vec<Pending>>,
    /// `<hostd dir>/mobile/pending-forget.json`: what `pending` is saved to, so a restart keeps it.
    pending_file: PathBuf,
}

/// A revoked phone and the crew servers (registry hosts) that still hold its push tokens.
#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    node: String,
    servers: Vec<String>,
}

/// The desktop's registry of crew servers, under `<registry>/desktop/`.
const REGISTRY_FILE: &str = "crew-servers.json";
/// Its name before Atlas Crew was called that.
const LEGACY_REGISTRY_FILE: &str = "bot-hosts.json";

/// Adopt a registry saved under its pre-Crew name; log (never fail) when that cannot be done.
fn adopt_legacy_registry(desktop: &Path) {
    use atlas_core::crew::{Adopted, adopt_legacy_path};
    match adopt_legacy_path(
        &desktop.join(LEGACY_REGISTRY_FILE),
        &desktop.join(REGISTRY_FILE),
    ) {
        Ok(Adopted::BothExist) => eprintln!(
            "atlas crew: {LEGACY_REGISTRY_FILE} and {REGISTRY_FILE} both exist; using {REGISTRY_FILE}"
        ),
        Ok(Adopted::Moved | Adopted::Nothing) => {}
        Err(e) => eprintln!("atlas crew: cannot adopt {LEGACY_REGISTRY_FILE}: {e}"),
    }
}

/// How long a `crew_call` waits for its crew server to connect.
const CONNECT_WAIT: Duration = Duration::from_secs(20);

/// Pending forgets, beside the phones they belong to (`mobile/devices.json`).
const PENDING_FORGET_FILE: &str = "mobile/pending-forget.json";

impl CrewRelay {
    /// `registry`: where the desktop keeps `desktop/crew-servers.json`; `sockets`: this hostd's dir,
    /// where the ssh tunnels' local ends and the pending forgets go. A registry still under its pre-Crew name
    /// (`bot-hosts.json`) is adopted here too: the desktop may not have run since the update.
    pub fn new(registry: &Path, sockets: &Path, connector: Option<Connector>) -> Self {
        let desktop = registry.join("desktop");
        adopt_legacy_registry(&desktop);
        CrewRelay {
            registry: registry.to_path_buf(),
            connector: connector.clone().unwrap_or_else(|| {
                ssh_connector(desktop.join(REGISTRY_FILE), sockets.join("crew-links"))
            }),
            servers: Mutex::new(HashMap::new()),
            hub: OnceLock::new(),
            pending: Mutex::new(load_pending(&sockets.join(PENDING_FORGET_FILE))),
            pending_file: sockets.join(PENDING_FORGET_FILE),
        }
    }

    /// Connect to every server a revoked phone is still pending on: a link that comes up settles its
    /// forgets (`keep`), so a restarted Mac delivers what the last one could not.
    pub(crate) fn start(self: &Arc<Self>, hub: &Arc<Hub>) {
        let _ = self.hub.set(Arc::downgrade(hub));
        self.prune_pending();
        let hosts: Vec<String> = self
            .pending
            .lock()
            .iter()
            .flat_map(|p| p.servers.clone())
            .collect();
        for e in self
            .entries()
            .into_iter()
            .filter(|e| hosts.contains(&e.host))
        {
            self.ensure(&e);
        }
    }

    /// The crew servers in the registry; ssh-unsafe names are left out.
    fn entries(&self) -> Vec<Entry> {
        let p = self.registry.join("desktop").join(REGISTRY_FILE);
        std::fs::read_to_string(p)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<Entry>>(&t).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|e| atlas_core::ssh::valid_target(&e.host).is_ok())
            .collect()
    }

    /// `{servers: [{id, name, state}]}`, starting a connection to each one not yet kept.
    pub fn servers(self: &Arc<Self>) -> Value {
        let list: Vec<Value> = self
            .entries()
            .into_iter()
            .map(|e| {
                let s = self.ensure(&e);
                json!({"id": e.host, "name": s.name, "state": *s.state.lock()})
            })
            .collect();
        json!({ "servers": list })
    }

    /// A revoked phone's push tokens leave every crew server of the registry (`push_forget`). The
    /// forget is saved first, then tried on each server in a task of its own (a server that is down
    /// must not hold the others up). A server that cannot be reached keeps the phone pending: the
    /// relay sends it when that server connects (`keep`) and when this hostd starts (#363). The
    /// phone is already cut off from this Mac either way.
    pub fn forget_phone(self: &Arc<Self>, node: &str) {
        let hosts: Vec<String> = self.entries().into_iter().map(|e| e.host).collect();
        if hosts.is_empty() {
            return;
        }
        {
            let mut pending = self.pending.lock();
            match pending.iter_mut().find(|p| p.node == node) {
                Some(p) => p.servers = hosts.clone(),
                None => pending.push(Pending {
                    node: node.to_string(),
                    servers: hosts.clone(),
                }),
            }
            self.save_pending(&pending);
        }
        for host in hosts {
            let relay = self.clone();
            tokio::spawn(async move { relay.settle(&host).await });
        }
    }

    /// Send `push_forget` for every phone still pending on `host`; a server it reached is taken off
    /// that phone's entry. Failures stay pending and are logged.
    async fn settle(self: &Arc<Self>, host: &str) {
        let nodes: Vec<String> = self
            .pending
            .lock()
            .iter()
            .filter(|p| p.servers.iter().any(|s| s == host))
            .map(|p| p.node.clone())
            .collect();
        for node in nodes {
            match self.call(host, "push_forget", json!({"node": node})).await {
                Ok(_) => {
                    let mut pending = self.pending.lock();
                    for p in pending.iter_mut().filter(|p| p.node == node) {
                        p.servers.retain(|s| s != host);
                    }
                    pending.retain(|p| !p.servers.is_empty());
                    self.save_pending(&pending);
                }
                Err(err) => eprintln!(
                    "atlas hostd: could not clear a revoked phone's push tokens on {host} (retried when it connects): {err}"
                ),
            }
        }
    }

    /// A server the desktop removed from its registry is never dialled again: forget it.
    fn prune_pending(&self) {
        let hosts: Vec<String> = self.entries().into_iter().map(|e| e.host).collect();
        let mut pending = self.pending.lock();
        let mut changed = false;
        for p in pending.iter_mut() {
            let n = p.servers.len();
            p.servers.retain(|s| hosts.contains(s));
            changed |= p.servers.len() != n;
        }
        pending.retain(|p| !p.servers.is_empty());
        if changed {
            self.save_pending(&pending);
        }
    }

    fn save_pending(&self, pending: &[Pending]) {
        let saved = serde_json::to_vec(pending)
            .map_err(|e| e.to_string())
            .and_then(|b| paths::write_private(&self.pending_file, &b).map_err(|e| e.to_string()));
        if let Err(e) = saved {
            eprintln!("atlas hostd: cannot save the pending push forgets: {e}");
        }
    }

    /// Run `cmd` on `server`, waiting for its connection when it is still coming up.
    pub async fn call(
        self: &Arc<Self>,
        server: &str,
        cmd: &str,
        args: Value,
    ) -> Result<Value, String> {
        let entry = self
            .entries()
            .into_iter()
            .find(|e| e.host == server)
            .ok_or_else(|| format!("unknown crew server {server:?}"))?;
        let mut rx = self.ensure(&entry).client.subscribe();
        let client = tokio::time::timeout(self.connector.wait, async {
            loop {
                if let Some(c) = rx.borrow_and_update().clone() {
                    return Some(c);
                }
                rx.changed().await.ok()?;
            }
        })
        .await
        .ok()
        .flatten()
        .ok_or_else(|| {
            let state = self
                .servers
                .lock()
                .get(server)
                .map(|s| s.state.lock().clone())
                .unwrap_or_default();
            format!("{server} is not reachable ({state})")
        })?;
        client.call(cmd, args).await.map_err(|e| format!("{e:#}"))
    }

    fn ensure(self: &Arc<Self>, e: &Entry) -> Arc<Server> {
        let mut servers = self.servers.lock();
        if let Some(s) = servers.get(&e.host) {
            return s.clone();
        }
        let s = Arc::new(Server {
            name: if e.name.is_empty() {
                e.host.clone()
            } else {
                e.name.clone()
            },
            client: watch::channel(None).0,
            state: Mutex::new("connecting".into()),
        });
        servers.insert(e.host.clone(), s.clone());
        tokio::spawn(keep(Arc::downgrade(self), e.host.clone(), s.clone()));
        s
    }
}

/// Keep `host` connected while it stays in the registry, relaying its pushes.
async fn keep(crew: Weak<CrewRelay>, host: String, server: Arc<Server>) {
    let mut backoff = Duration::from_secs(2);
    loop {
        let Some(c) = crew.upgrade() else { return };
        if !c.entries().iter().any(|e| e.host == host) {
            c.servers.lock().remove(&host);
            return;
        }
        *server.state.lock() = "connecting".into();
        let attempt = (c.connector.dial)(host.clone());
        let hub = c.hub.get().cloned();
        drop(c);
        match attempt.await {
            Ok(link) => {
                backoff = Duration::from_secs(2);
                *server.state.lock() = "connected".into();
                let mut events = link.client.events();
                // `send_replace`: `send` drops the value when nobody is waiting yet.
                server.client.send_replace(Some(link.client.clone()));
                if let Some(c) = crew.upgrade() {
                    c.prune_pending();
                    let host = host.clone();
                    tokio::spawn(async move { c.settle(&host).await });
                }
                let relay = {
                    let host = host.clone();
                    tokio::spawn(async move {
                        while let Ok(ev) = events.recv().await {
                            let Some(hub) = hub.as_ref().and_then(Weak::upgrade) else {
                                return;
                            };
                            hub.broadcast(
                                json!({"ev": "crew_event", "server": host, "event": *ev})
                                    .to_string(),
                            );
                        }
                    })
                };
                link.client.closed().await;
                relay.abort();
                server.client.send_replace(None);
                drop(link);
                *server.state.lock() = "disconnected".into();
            }
            Err(e) => *server.state.lock() = e,
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

/// The pending forgets saved by the last hostd; unreadable ones are logged, never silently dropped.
fn load_pending(file: &Path) -> Vec<Pending> {
    match std::fs::read(file) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
            eprintln!("atlas hostd: ignoring {}: {e}", file.display());
            Vec::new()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            eprintln!("atlas hostd: cannot read {}: {e}", file.display());
            Vec::new()
        }
    }
}

#[cfg(unix)]
fn ssh_connector(registry_file: PathBuf, dir: PathBuf) -> Connector {
    Connector::new(Arc::new(move |host| {
        Box::pin(ssh_link(registry_file.clone(), dir.clone(), host))
    }))
}

#[cfg(not(unix))]
fn ssh_connector(_registry_file: PathBuf, _dir: PathBuf) -> Connector {
    Connector::new(Arc::new(|_| {
        Box::pin(async { Err("crew servers are reached from macOS and Linux only".to_string()) })
    }))
}

/// `$HOME` and the hostd token of `host`, then a tunnel to its hostd socket. `host`'s port is the
/// one its registry entry carries (`registry_file`), read at every (re)connect.
#[cfg(unix)]
async fn ssh_link(registry_file: PathBuf, dir: PathBuf, host: String) -> Result<Link, String> {
    use atlas_core::sys::CommandExt as _;
    use std::process::Stdio;
    use tokio::process::Command;

    atlas_core::ssh::valid_target(&host)?;
    let target = atlas_core::ssh::SshTarget::new(host.clone(), port_of(&registry_file, &host));
    let out = tokio::time::timeout(
        Duration::from_secs(20),
        Command::new("ssh")
            .no_window()
            .args(target.raw_argv(r#"printf '%s\n' "$HOME"; cat "$HOME/.atlas/hostd.token""#))
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| format!("ssh {host}: timed out"))?
    .map_err(|e| format!("ssh: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ssh {host}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let (home, token) = (
        lines.next().unwrap_or(""),
        lines.next().unwrap_or("").trim(),
    );
    if home.is_empty() || token.is_empty() {
        return Err(format!("{host}: no atlas hostd there (atlas crew install)"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let sock = dir.join(format!("{host}.sock"));
    let _ = std::fs::remove_file(&sock);
    let tunnel = Command::new("ssh")
        .no_window()
        .args(target.tunnel_argv(&format!("{}:{home}/.atlas/hostd.sock", sock.display())))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("ssh tunnel: {e}"))?;
    let mut last = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match HostClient::connect(&sock, token, "crew").await {
            Ok(client) => {
                return Ok(Link {
                    client,
                    keep: Some(Box::new(Mutex::new(tunnel))),
                });
            }
            Err(e) => last = format!("{e:#}"),
        }
    }
    Err(format!("{host}: tunnel up but hostd unreachable: {last}"))
}

#[cfg(test)]
mod tests;
