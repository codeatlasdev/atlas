//! Atlas Crew for Atlas Mobile: the phone talks to this Mac only, but the crewmates live on bot
//! computers Atlas Desktop reaches over SSH (`<registry>/desktop/bot-hosts.json`). This hostd
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
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::HostClient;
use crate::server::Hub;

/// A live connection to one bot computer and what keeps it open (the ssh tunnel), dropped with it.
pub struct Link {
    pub client: HostClient,
    pub keep: Option<Box<dyn Send + Sync>>,
}

type Connecting = Pin<Box<dyn Future<Output = Result<Link, String>> + Send>>;

/// How to reach a bot computer: ssh by default, a local daemon in tests.
#[derive(Clone)]
pub struct Connector(pub Arc<dyn Fn(String) -> Connecting + Send + Sync>);

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
}

struct Server {
    name: String,
    client: watch::Sender<Option<HostClient>>,
    state: Mutex<String>,
}

pub struct Crew {
    registry: PathBuf,
    connector: Connector,
    servers: Mutex<HashMap<String, Arc<Server>>>,
    hub: OnceLock<Weak<Hub>>,
}

/// How long a `crew_call` waits for its bot computer to connect.
const CONNECT_WAIT: Duration = Duration::from_secs(20);

impl Crew {
    /// `registry`: where the desktop keeps `desktop/bot-hosts.json`; `sockets`: where the ssh
    /// tunnels' local ends go (ssh connector only).
    pub fn new(registry: &Path, sockets: &Path, connector: Option<Connector>) -> Self {
        Crew {
            registry: registry.to_path_buf(),
            connector: connector.unwrap_or_else(|| ssh_connector(sockets.join("crew"))),
            servers: Mutex::new(HashMap::new()),
            hub: OnceLock::new(),
        }
    }

    pub(crate) fn start(&self, hub: &Arc<Hub>) {
        let _ = self.hub.set(Arc::downgrade(hub));
    }

    /// The bot computers in the registry; ssh-unsafe names are left out.
    fn entries(&self) -> Vec<Entry> {
        let p = self.registry.join("desktop").join("bot-hosts.json");
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
            .ok_or_else(|| format!("unknown bot computer {server:?}"))?;
        let mut rx = self.ensure(&entry).client.subscribe();
        let client = tokio::time::timeout(CONNECT_WAIT, async {
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
async fn keep(crew: Weak<Crew>, host: String, server: Arc<Server>) {
    let mut backoff = Duration::from_secs(2);
    loop {
        let Some(c) = crew.upgrade() else { return };
        if !c.entries().iter().any(|e| e.host == host) {
            c.servers.lock().remove(&host);
            return;
        }
        *server.state.lock() = "connecting".into();
        let attempt = (c.connector.0)(host.clone());
        let hub = c.hub.get().cloned();
        drop(c);
        match attempt.await {
            Ok(link) => {
                backoff = Duration::from_secs(2);
                *server.state.lock() = "connected".into();
                let mut events = link.client.events();
                // `send_replace`: `send` drops the value when nobody is waiting yet.
                server.client.send_replace(Some(link.client.clone()));
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

#[cfg(unix)]
fn ssh_connector(dir: PathBuf) -> Connector {
    Connector(Arc::new(move |host| Box::pin(ssh_link(dir.clone(), host))))
}

#[cfg(not(unix))]
fn ssh_connector(_dir: PathBuf) -> Connector {
    Connector(Arc::new(|_| {
        Box::pin(async { Err("bot computers are reached from macOS and Linux only".to_string()) })
    }))
}

#[cfg(unix)]
const SSH_OPTS: [&str; 6] = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ServerAliveInterval=15",
];

/// `$HOME` and the hostd token of `host`, then a tunnel to its hostd socket.
#[cfg(unix)]
async fn ssh_link(dir: PathBuf, host: String) -> Result<Link, String> {
    use atlas_core::sys::CommandExt as _;
    use std::process::Stdio;
    use tokio::process::Command;

    atlas_core::ssh::valid_target(&host)?;
    let out = tokio::time::timeout(
        Duration::from_secs(20),
        Command::new("ssh")
            .no_window()
            .args(SSH_OPTS)
            .arg(&host)
            .arg(r#"printf '%s\n' "$HOME"; cat "$HOME/.atlas/hostd.token""#)
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
        return Err(format!("{host}: no atlas hostd there (atlas bot install)"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let sock = dir.join(format!("{host}.sock"));
    let _ = std::fs::remove_file(&sock);
    let tunnel = Command::new("ssh")
        .no_window()
        .args(SSH_OPTS)
        .args([
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "StreamLocalBindUnlink=yes",
        ])
        .arg("-L")
        .arg(format!("{}:{home}/.atlas/hostd.sock", sock.display()))
        .arg(&host)
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
