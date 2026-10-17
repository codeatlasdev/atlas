//! Atlas Mobile gateway: phones reach this hostd over iroh (QUIC dialed by public key,
//! direct when possible, n0 public relays otherwise; end-to-end encrypted either way).
//! No account: a phone is trusted once it presents a one-time pairing code shown by the
//! desktop, and from then on by its iroh key alone.
//!
//! Files under `<dir>/mobile/` (dir 0700, files 0600): `key` (this host's secret key),
//! `devices.json` (paired phones, scopes, revocation), `audit.log` (one JSON line per
//! non-read request from a phone).
//!
//! Wire: one bidirectional stream per connection carrying the same NDJSON as the unix
//! socket (`{"id","cmd","args"}` → `{"id","ok","data"|"error"}`, pushes have `ev`). An
//! unpaired phone may only send `{"id":1,"cmd":"pair","args":{"code","name"}}`.
//!
//! `ATLAS_MOBILE_RELAY=0` disables relays and address lookup (LAN/tests).

mod session;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iroh::endpoint::Connection;
use iroh::{Endpoint, SecretKey};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::HostCmd;
use crate::server::Hub;

pub const ALPN: &[u8] = b"atlas-mobile/1";
const CODE_TTL: Duration = Duration::from_secs(120);
const MAX_BAD_CODES: u32 = 5;

/// What a paired phone may do. Every mobile request needs exactly one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Projects, services, logs, agent state, terminal list.
    Read,
    /// Restart/start/stop services, run actions, send a prompt to an idle agent.
    Act,
    /// Answer agent permission prompts.
    Approve,
    /// Watch a terminal.
    TermRead,
    /// Type into a terminal.
    TermWrite,
}

pub const ALL_SCOPES: [Scope; 5] = [
    Scope::Read,
    Scope::Act,
    Scope::Approve,
    Scope::TermRead,
    Scope::TermWrite,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub node_id: String,
    pub name: String,
    pub paired_at: u64,
    pub scopes: Vec<Scope>,
    #[serde(default)]
    pub revoked: bool,
}

struct PairCode {
    code: String,
    expires: std::time::Instant,
    bad: u32,
}

pub struct Mobile {
    dir: PathBuf,
    key: SecretKey,
    devices: Mutex<Vec<Device>>,
    pair: Mutex<Option<PairCode>>,
    /// Live connections per phone, closed on revoke.
    live: Mutex<HashMap<String, Vec<Connection>>>,
    endpoint: Mutex<Option<Endpoint>>,
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn mobile_dir(dir: &Path) -> PathBuf {
    dir.join("mobile")
}

fn write_private(p: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp~");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(data)?;
    std::fs::rename(tmp, p)
}

/// This host's iroh identity, created on first use.
pub fn load_or_create_key(dir: &Path) -> std::io::Result<SecretKey> {
    let d = mobile_dir(dir);
    std::fs::create_dir_all(&d)?;
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700))?;
    let p = d.join("key");
    if let Ok(b) = std::fs::read(&p)
        && let Ok(bytes) = <[u8; 32]>::try_from(b.as_slice())
    {
        return Ok(SecretKey::from_bytes(&bytes));
    }
    let mut bytes = [0u8; 32];
    std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom")?, &mut bytes)?;
    write_private(&p, &bytes)?;
    Ok(SecretKey::from_bytes(&bytes))
}

impl Mobile {
    pub fn load(dir: &Path) -> std::io::Result<Self> {
        let key = load_or_create_key(dir)?;
        let devices = std::fs::read_to_string(mobile_dir(dir).join("devices.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Ok(Self {
            dir: dir.to_path_buf(),
            key,
            devices: Mutex::new(devices),
            pair: Mutex::new(None),
            live: Mutex::new(HashMap::new()),
            endpoint: Mutex::new(None),
        })
    }

    pub fn node_id(&self) -> String {
        self.key.public().to_string()
    }

    fn save(&self, devices: &[Device]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(devices).map_err(|e| e.to_string())?;
        write_private(&mobile_dir(&self.dir).join("devices.json"), text.as_bytes())
            .map_err(|e| e.to_string())
    }

    /// A paired, non-revoked phone.
    pub fn device(&self, node_id: &str) -> Option<Device> {
        self.devices
            .lock()
            .iter()
            .find(|d| d.node_id == node_id && !d.revoked)
            .cloned()
    }

    /// New one-time code (replaces any previous one).
    pub fn pair_start(&self) -> Value {
        let n = crate::paths::random_hex(4)
            .ok()
            .and_then(|h| u32::from_str_radix(&h, 16).ok())
            .unwrap_or(0);
        let code = format!("{:06}", n % 1_000_000);
        *self.pair.lock() = Some(PairCode {
            code: code.clone(),
            expires: std::time::Instant::now() + CODE_TTL,
            bad: 0,
        });
        let addrs: Vec<String> = self
            .endpoint
            .lock()
            .as_ref()
            .map(|e| e.addr().ip_addrs().map(|a| a.to_string()).collect())
            .unwrap_or_default();
        let relay: Option<String> = self
            .endpoint
            .lock()
            .as_ref()
            .and_then(|e| e.addr().relay_urls().next().map(|u| u.to_string()));
        json!({
            "node_id": self.node_id(),
            "code": code,
            "expires_in_s": CODE_TTL.as_secs(),
            "addrs": addrs,
            "relay_url": relay,
            "hostname": crate::server::hostname(),
        })
    }

    /// Check a code from an unpaired phone; on success it is paired with every scope.
    pub fn pair(&self, node_id: &str, code: &str, name: &str) -> Result<Device, String> {
        {
            let mut g = self.pair.lock();
            let Some(p) = g.as_mut() else {
                return Err("no pairing in progress".into());
            };
            if p.expires < std::time::Instant::now() {
                *g = None;
                return Err("code expired".into());
            }
            if p.code != code {
                p.bad += 1;
                if p.bad >= MAX_BAD_CODES {
                    *g = None;
                }
                return Err("bad code".into());
            }
            *g = None;
        }
        let dev = Device {
            node_id: node_id.to_string(),
            name: name.chars().take(64).collect(),
            paired_at: now_s(),
            scopes: ALL_SCOPES.to_vec(),
            revoked: false,
        };
        let mut all = self.devices.lock();
        all.retain(|d| d.node_id != node_id);
        all.push(dev.clone());
        self.save(&all)?;
        Ok(dev)
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().clone()
    }

    pub fn revoke(&self, node_id: &str) -> Result<(), String> {
        {
            let mut all = self.devices.lock();
            let d = all
                .iter_mut()
                .find(|d| d.node_id == node_id)
                .ok_or("unknown device")?;
            d.revoked = true;
            self.save(&all)?;
        }
        for c in self.live.lock().remove(node_id).unwrap_or_default() {
            c.close(1u32.into(), b"revoked");
        }
        Ok(())
    }

    pub fn set_scopes(&self, node_id: &str, scopes: Vec<Scope>) -> Result<(), String> {
        let mut all = self.devices.lock();
        let d = all
            .iter_mut()
            .find(|d| d.node_id == node_id)
            .ok_or("unknown device")?;
        d.scopes = scopes;
        self.save(&all)
    }

    /// Append one line to `audit.log`. `pty_write` data is never logged, only its size.
    pub fn audit(&self, node_id: &str, cmd: &str, args: &Value) {
        let mut args = args.clone();
        if let Some(d) = args.get("data").and_then(Value::as_str) {
            args["data"] = json!(format!("<{} bytes base64>", d.len()));
        }
        if let Some(t) = args.get("text").and_then(Value::as_str) {
            args["text"] = json!(format!("<{} chars>", t.chars().count()));
        }
        let line = json!({"ts": now_s(), "node_id": node_id, "cmd": cmd, "args": args});
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(mobile_dir(&self.dir).join("audit.log"))
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// The scope a mobile request needs; `Err` = never available from a phone.
pub fn required_scope(cmd: &HostCmd) -> Result<Scope, &'static str> {
    use HostCmd as C;
    Ok(match cmd {
        C::PtyList {} | C::PtyInfo { .. } | C::PtyRead { .. } | C::AgentStatusAll {} => Scope::Read,
        C::ProjectList {} | C::ProjectLogs { .. } | C::Info {} => Scope::Read,
        C::ProjectService { .. } | C::ProjectAction { .. } | C::AgentSend { .. } => Scope::Act,
        C::AgentApprove { .. } => Scope::Approve,
        C::PtyAttach { .. } | C::PtyDetach { .. } => Scope::TermRead,
        C::PtyWrite { .. } | C::PtySignal { .. } => Scope::TermWrite,
        _ => return Err("not available on mobile"),
    })
}

fn preset_relay() -> bool {
    std::env::var("ATLAS_MOBILE_RELAY").map_or(true, |v| v != "0")
}

/// Bind the iroh endpoint and accept phones until the process exits. Spawned so that a
/// slow or offline network never delays the unix socket.
pub(crate) fn start(hub: Arc<Hub>) {
    tokio::spawn(async move {
        let mobile = hub.mobile.clone();
        let builder = if preset_relay() {
            Endpoint::builder(iroh::endpoint::presets::N0)
        } else {
            Endpoint::builder(iroh::endpoint::presets::Minimal)
        };
        let ep = match builder
            .secret_key(mobile.key.clone())
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
        {
            Ok(e) => e,
            Err(e) => {
                eprintln!("hostd: mobile gateway: {e}");
                return;
            }
        };
        *mobile.endpoint.lock() = Some(ep.clone());
        while let Some(inc) = ep.accept().await {
            let hub = hub.clone();
            tokio::spawn(async move {
                let Ok(conn) = inc.await else { return };
                session::run(hub, conn).await;
            });
        }
    });
}

impl Mobile {
    fn track(&self, node: &str, c: &Connection) {
        self.live
            .lock()
            .entry(node.to_string())
            .or_default()
            .push(c.clone());
    }

    fn untrack(&self, node: &str, c: &Connection) {
        let mut l = self.live.lock();
        if let Some(v) = l.get_mut(node) {
            v.retain(|x| x.stable_id() != c.stable_id());
            if v.is_empty() {
                l.remove(node);
            }
        }
    }

    /// This endpoint's dialable address (tests, `mobile_pair_start`).
    pub fn endpoint(&self) -> Option<Endpoint> {
        self.endpoint.lock().clone()
    }
}
