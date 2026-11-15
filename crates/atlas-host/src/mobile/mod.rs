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
//! The endpoint binds only once a phone is paired or a pairing starts: a hostd nobody
//! pairs never touches the network. `ServeOpts::no_mobile_relay` (`ATLAS_MOBILE_RELAY=0`
//! for `atlas hostd`) turns relays and address lookup off (LAN, tests).

mod addrs;
mod session;
pub(crate) mod shell;
// Unix like the daemon's other end-to-end tests: they spawn `sh` and `cat` in ptys.
#[cfg(all(test, unix))]
mod push_tests;
#[cfg(all(test, unix))]
pub(crate) mod tests;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iroh::endpoint::Connection;
use iroh::{Endpoint, SecretKey};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::paths;
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

/// How much of `audit.log` (its end) the Settings list and activity read.
const AUDIT_TAIL: u64 = 512 * 1024;
const MAX_ACTIVITY: usize = 200;

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
    /// When the phone last connected or disconnected (unix seconds); absent until it first connects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    pub url: String,
    pub name: String,
    pub created_at: u64,
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
    /// Serializes binding, so two pairing requests bind one endpoint.
    binding: tokio::sync::Mutex<()>,
    /// Use n0's public relays (off: direct addresses only, for tests and local runs).
    relay: bool,
    /// The daemon, for the sessions the endpoint accepts (set once it exists).
    hub: std::sync::OnceLock<std::sync::Weak<Hub>>,
    /// Pending mobile invites (url, name, created_at); pruned on expiry.
    invites: Mutex<Vec<Invite>>,
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

/// This host's iroh identity, created on first use.
pub fn load_or_create_key(dir: &Path) -> std::io::Result<SecretKey> {
    let d = mobile_dir(dir);
    std::fs::create_dir_all(&d)?;
    // Windows: the folder keeps the per-user ACL of `~/.atlas` (the profile is private).
    #[cfg(unix)]
    std::fs::set_permissions(&d, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    let p = d.join("key");
    match std::fs::read(&p) {
        // A key that is there but unreadable is an error, never a silent new identity:
        // every paired phone dials this node id.
        Ok(b) => {
            let bytes = <[u8; 32]>::try_from(b.as_slice()).map_err(|_| {
                std::io::Error::other(format!(
                    "{}: not a 32-byte key; remove it to re-pair every phone",
                    p.display()
                ))
            })?;
            return Ok(SecretKey::from_bytes(&bytes));
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        Err(_) => {}
    }
    let mut bytes = [0u8; 32];
    atlas_core::sys::random_bytes(&mut bytes)?;
    paths::write_private(&p, &bytes)?;
    Ok(SecretKey::from_bytes(&bytes))
}

impl Mobile {
    pub fn load(dir: &Path, relay: bool) -> std::io::Result<Self> {
        let key = load_or_create_key(dir)?;
        let devices = std::fs::read_to_string(mobile_dir(dir).join("devices.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let invites = std::fs::read_to_string(mobile_dir(dir).join("invites.json"))
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
            binding: tokio::sync::Mutex::new(()),
            relay,
            hub: std::sync::OnceLock::new(),
            invites: Mutex::new(invites),
        })
    }

    pub fn node_id(&self) -> String {
        self.key.public().to_string()
    }

    fn save(&self, devices: &[Device]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(devices).map_err(|e| e.to_string())?;
        paths::write_private(&mobile_dir(&self.dir).join("devices.json"), text.as_bytes())
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

    /// New one-time code (replaces any previous one, and with it the pending invites made for that one:
    /// their link would fail with "bad code").
    /// Fails rather than hand out a guessable code when the system has no randomness.
    pub fn pair_start(&self) -> Result<Value, String> {
        let mut n = [0u8; 4];
        atlas_core::sys::random_bytes(&mut n).map_err(|e| format!("random: {e}"))?;
        let code = format!("{:06}", u32::from_le_bytes(n) % 1_000_000);
        *self.pair.lock() = Some(PairCode {
            code: code.clone(),
            expires: std::time::Instant::now() + CODE_TTL,
            bad: 0,
        });
        {
            let mut invites = self.invites.lock();
            if !invites.is_empty() {
                invites.clear();
                self.save_invites(&invites);
            }
        }
        let addr = self.endpoint.lock().as_ref().map(|e| e.addr());
        let direct: Vec<std::net::SocketAddr> =
            addr.iter().flat_map(|a| a.ip_addrs().copied()).collect();
        let published: Vec<String> = addrs::publishable(&direct, &addrs::local_interfaces())
            .iter()
            .map(|a| a.to_string())
            .collect();
        let relay = addr
            .as_ref()
            .and_then(|a| a.relay_urls().next().map(|u| u.to_string()));
        Ok(json!({
            "node_id": self.node_id(),
            "code": code,
            "expires_in_s": CODE_TTL.as_secs(),
            "addrs": published,
            "relay_url": relay,
            "hostname": crate::server::hostname(),
        }))
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
            last_seen: None,
        };
        // Trusted only once it is on disk: a phone told "failed" must not be paired.
        let mut all = self.devices.lock();
        let mut next: Vec<Device> = all
            .iter()
            .filter(|d| d.node_id != node_id)
            .cloned()
            .collect();
        next.push(dev.clone());
        self.save(&next)?;
        *all = next;
        Ok(dev)
    }

    pub fn devices(&self) -> Vec<Device> {
        self.devices.lock().clone()
    }

    /// Takes effect at once, even when saving fails (the error is still returned): taking
    /// trust away must never wait on the disk.
    pub fn revoke(&self, node_id: &str) -> Result<(), String> {
        let saved = {
            let mut all = self.devices.lock();
            let d = all
                .iter_mut()
                .find(|d| d.node_id == node_id)
                .ok_or("unknown device")?;
            d.revoked = true;
            self.save(&all)
        };
        for c in self.live.lock().remove(node_id).unwrap_or_default() {
            c.close(1u32.into(), b"revoked");
        }
        saved
    }

    /// Like [`revoke`](Self::revoke), applies at once and reports a failed save.
    pub fn set_scopes(&self, node_id: &str, scopes: Vec<Scope>) -> Result<(), String> {
        let mut all = self.devices.lock();
        let d = all
            .iter_mut()
            .find(|d| d.node_id == node_id)
            .ok_or("unknown device")?;
        d.scopes = scopes;
        self.save(&all)
    }

    /// Append one line to `audit.log`. What a person typed or sent is logged as its size only, at any depth
    /// (a relayed `crew_call` carries the real command's arguments inside `args`): `pty_write` data, a
    /// prompt, a commit message, a PR title or body, and `value`, which is a secret the phone provided.
    pub fn audit(&self, node_id: &str, cmd: &str, args: &Value) {
        let mut args = args.clone();
        redact(&mut args);
        let line = json!({"ts": now_s(), "node_id": node_id, "cmd": cmd, "args": args});
        if let Ok(mut f) =
            atlas_core::sys::owner_only(std::fs::OpenOptions::new().create(true).append(true))
                .open(mobile_dir(&self.dir).join("audit.log"))
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Rename a phone (the name it sent when pairing is the same for every Atlas Mobile).
    pub fn rename(&self, node_id: &str, name: &str) -> Result<(), String> {
        let name: String = name.trim().chars().take(64).collect();
        if name.is_empty() {
            return Err("name is empty".into());
        }
        let mut all = self.devices.lock();
        let d = all
            .iter_mut()
            .find(|d| d.node_id == node_id && !d.revoked)
            .ok_or("unknown device")?;
        d.name = name;
        self.save(&all)
    }

    fn save_invites(&self, invites: &[Invite]) {
        if let Ok(text) = serde_json::to_string_pretty(invites) {
            let _ =
                paths::write_private(&mobile_dir(&self.dir).join("invites.json"), text.as_bytes());
        }
    }

    /// Desktop-only: store a pending invite and push it to connected phones.
    pub(crate) fn mobile_invite(&self, url: String, name: String, hub: &crate::server::Hub) {
        let now = now_s();
        let ttl = CODE_TTL.as_secs();
        let mut invites = self.invites.lock();
        invites.retain(|i| now.saturating_sub(i.created_at) < ttl);
        invites.push(Invite {
            url: url.clone(),
            name: name.clone(),
            created_at: now,
        });
        self.save_invites(&invites);
        drop(invites);
        hub.broadcast(json!({"ev": "computer_invite", "url": url, "name": name}).to_string());
    }

    /// Phone-facing: return non-expired pending invites (pruning as a side-effect).
    pub fn mobile_invites(&self) -> Vec<Invite> {
        let now = now_s();
        let ttl = CODE_TTL.as_secs();
        let mut invites = self.invites.lock();
        invites.retain(|i| now.saturating_sub(i.created_at) < ttl);
        let out = invites.clone();
        self.save_invites(&invites);
        out
    }

    /// Phone-facing: remove a specific invite by url.
    pub fn mobile_invite_dismiss(&self, url: &str) {
        let mut invites = self.invites.lock();
        invites.retain(|i| i.url != url);
        self.save_invites(&invites);
    }
}

/// Replace what a person typed or sent, at any depth, with its size: `data` is base64 (bytes), the rest chars.
fn redact(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                match (k.as_str(), &*x) {
                    ("data", Value::String(d)) => *x = json!(format!("<{} bytes base64>", d.len())),
                    ("text" | "message" | "title" | "body" | "value", Value::String(t)) => {
                        *x = json!(format!("<{} chars>", t.chars().count()));
                    }
                    _ => redact(x),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(redact),
        _ => {}
    }
}

/// The pairing link a phone scans, from a `mobile_pair_start` reply:
/// `atlas-mobile://pair?node=<id>&code=<code>&name=<host>[&addr=ip:port…][&relay=url]`. The code is not what
/// protects the link (the iroh keys are); it proves the phone was in front of the screen that showed it. The
/// desktop's `pairUrl` (`apps/desktop/src/lib/settings/mobile.ts`) builds the same link; a fixture pins both.
pub fn pair_url(start: &Value) -> Result<String, String> {
    let field = |k: &str| {
        start[k]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("pairing reply has no {k}"))
    };
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    q.append_pair("node", field("node_id")?)
        .append_pair("code", field("code")?)
        .append_pair("name", start["hostname"].as_str().unwrap_or_default());
    for a in start["addrs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        q.append_pair("addr", a);
    }
    if let Some(r) = start["relay_url"].as_str() {
        q.append_pair("relay", r);
    }
    Ok(format!("atlas-mobile://pair?{}", q.finish()))
}

/// The scope a push needs to reach a phone; `None` = never sent to one (the desktop's
/// editor and file watching).
pub fn push_scope(ev: &str) -> Option<Scope> {
    Some(match ev {
        "pty_output" | "pty_replay" | "pty_title" | "pty_activity" | "pty_exit" | "pty_dropped" => {
            Scope::TermRead
        }
        // The prompt carries the tool input (a shell command, a file path).
        "agent_approval" | "agent_approval_done" => Scope::Approve,
        "agent_status" => Scope::Read,
        "crewmate_task" => Scope::Read,
        "computer_invite" => Scope::Read,
        _ => return None,
    })
}

/// [`push_scope`] of a whole push line. A `crew_event` (a crew server's push relayed by
/// `crate::crew`) needs what the event inside it needs, and never a terminal's.
pub fn line_scope(line: &str) -> Option<Scope> {
    #[derive(serde::Deserialize)]
    struct Ev<'a> {
        #[serde(borrow)]
        ev: std::borrow::Cow<'a, str>,
        #[serde(default)]
        event: Option<Inner>,
    }
    #[derive(serde::Deserialize)]
    struct Inner {
        ev: String,
    }
    let e = serde_json::from_str::<Ev>(line).ok()?;
    if e.ev != "crew_event" {
        return push_scope(&e.ev);
    }
    match push_scope(&e.event?.ev)? {
        Scope::TermRead | Scope::TermWrite => None,
        s => Some(s),
    }
}

/// The scope a mobile request needs; `Err` = never available from a phone.
pub fn required_scope(cmd: &HostCmd) -> Result<Scope, &'static str> {
    use HostCmd as C;
    Ok(match cmd {
        C::PtyList {}
        | C::PtyInfo { .. }
        | C::AgentStatusAll {}
        | C::AgentIntegrations {}
        | C::AgentTree { .. }
        | C::AgentHistory { .. } => Scope::Read,
        C::ProjectList {} | C::ProjectDetail { .. } | C::ProjectLogs { .. } | C::Info {} => {
            Scope::Read
        }
        C::WorktreeList { .. } => Scope::Read,
        C::ProjectService { .. }
        | C::ProjectAction { .. }
        | C::AgentSend { .. }
        | C::AgentInterrupt { .. }
        | C::AgentLaunch { .. }
        // A new shell is a process on the computer: `act`, and typing into it still needs `term_write`
        // (and the phone's Face ID gate). It takes registry ids, never a path (`pty_create` stays refused).
        | C::PtyOpen { .. }
        | C::WorktreeCreate { .. }
        | C::WorktreeRemove { .. } => Scope::Act,
        C::GitStatus { .. } | C::GitDiff { .. } => Scope::Read,
        C::GitStage { .. }
        | C::GitUnstage { .. }
        | C::GitCommit { .. }
        | C::GitCommitMessage { .. }
        | C::GitPush { .. }
        | C::GitPrCreate { .. } => Scope::Act,
        C::AgentApprove { .. } => Scope::Approve,
        // pty_read returns the scrollback text: reading a terminal, not listing it.
        C::PtyAttach { .. } | C::PtyDetach { .. } | C::PtyRead { .. } => Scope::TermRead,
        C::PtyWrite { .. } | C::PtySignal { .. } => Scope::TermWrite,
        C::CrewmateList {}
        | C::CrewmateTasks { .. }
        | C::CrewmateMemory { .. }
        // The bytes of an image sent with a task: reading what the thread shows.
        | C::CrewmateAttachment { .. } => Scope::Read,
        // Uploading the pieces of an image only ever feeds the `crewmate_task` that names it.
        C::CrewmateTask { .. } | C::CrewmateAttachmentPut { .. } | C::CrewmateCancel { .. } => {
            Scope::Act
        }
        C::CrewmateCreate { .. }
        | C::CrewmateUpdate { .. }
        | C::CrewmateDelete { .. }
        | C::CrewmateMemorySet { .. }
        | C::CrewmateRoutineSet { .. } => Scope::Act,
        C::PushRegister { .. } | C::PushUnregister { .. } => Scope::Read,
        C::CrewTelegramSet { .. } | C::CrewTelegramLink {} | C::CrewTelegramStatus {} => {
            return Err("not available on mobile");
        }
        C::CrewmateTriggersSet { .. } => Scope::Act,
        C::CrewmateAnswer { .. } => Scope::Act,
        C::CrewmateUsage { .. } | C::CrewmateAudit { .. } | C::CrewAudit { .. } => Scope::Read,
        C::EngineLimits {} => Scope::Read,
        C::CrewmateSecretSet { .. } => return Err("not available on mobile"),
        C::MobileInvite { .. } => return Err("not available on mobile"),
        C::MobileInvites {} => Scope::Read,
        C::MobileInviteDismiss { .. } => Scope::Act,
        C::CrewGithubStatus {} | C::CrewGithubLoginPoll {} => Scope::Read,
        C::CrewGithubLoginStart {} | C::CrewGithubLogout {} => Scope::Act,
        C::CrewmateActionAnswer { .. } | C::CrewGithubConnectResume {} => Scope::Act,
        C::CrewPause {} | C::CrewResume {} => Scope::Act,
        C::CrewSettingsGet {} => Scope::Read,
        C::CrewSettingsSet { .. } => Scope::Act,
        // Reading a status runs the engine's own status command; the sign-in terminal starts a login: Act (and
        // the phone's Face ID gate). Both are for the crew server itself: through a Mac's relay a terminal
        // cannot be attached (`CrewCall` below), so the phone pairs with the crew server for this.
        C::CrewEngineStatus { .. } => Scope::Read,
        C::CrewEngineLogin { .. } => Scope::Act,
        C::CrewServers {} => Scope::Read,
        // What the relayed command needs here. Terminals stay out: their output streams to the
        // relaying connection, not to one phone.
        C::CrewCall { cmd, args, .. } => {
            let inner = HostCmd::crew_inner(cmd, args).map_err(|_| "bad crew request")?;
            // A sign-in or a new shell is a terminal on the crew server: relayed, the phone could never attach to
            // it (a Mac's relay holds that connection). Pair the phone with the crew server itself.
            if matches!(
                inner,
                C::CrewCall { .. }
                    | C::CrewServers {}
                    | C::CrewEngineLogin { .. }
                    | C::PtyOpen { .. }
            ) {
                return Err("not available on mobile");
            }
            match required_scope(&inner)? {
                Scope::TermRead | Scope::TermWrite => return Err("not available on mobile"),
                s => s,
            }
        }
        _ => return Err("not available on mobile"),
    })
}

/// Open the gateway for a daemon that already has phones; one with none stays off the
/// network until a pairing starts ([`Mobile::ensure_endpoint`]). Spawned so that a slow or
/// offline network never delays the unix socket.
pub(crate) fn start(hub: &Arc<Hub>) {
    let _ = hub.mobile.hub.set(Arc::downgrade(hub));
    if hub.mobile.devices.lock().iter().any(|d| !d.revoked) {
        let mobile = hub.mobile.clone();
        tokio::spawn(async move {
            if let Err(e) = mobile.ensure_endpoint().await {
                eprintln!("hostd: mobile gateway: {e}");
            }
        });
    }
}

/// Serve every phone `ep` accepts. A plain function, not part of an async fn's body: a
/// session can start the endpoint (pairing), so a future that spawned sessions inline
/// would have to name its own type.
fn accept_phones(ep: Endpoint, hub: std::sync::Weak<Hub>) {
    tokio::spawn(async move {
        while let Some(inc) = ep.accept().await {
            let Some(hub) = hub.upgrade() else { break };
            tokio::spawn(async move {
                let Ok(conn) = inc.await else { return };
                session::run(hub, conn).await;
            });
        }
    });
}

impl Mobile {
    /// Bind the iroh endpoint (once) and accept phones until the process exits.
    pub(crate) async fn ensure_endpoint(self: &Arc<Self>) -> Result<(), String> {
        let _one = self.binding.lock().await;
        if self.endpoint.lock().is_some() {
            return Ok(());
        }
        let builder = if self.relay {
            Endpoint::builder(iroh::endpoint::presets::N0)
        } else {
            Endpoint::builder(iroh::endpoint::presets::Minimal)
        };
        let ep = builder
            .secret_key(self.key.clone())
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| format!("mobile gateway: {e}"))?;
        *self.endpoint.lock() = Some(ep.clone());
        accept_phones(ep, self.hub.get().cloned().unwrap_or_default());
        Ok(())
    }

    /// A connected phone may answer permission prompts (scopes as they are now).
    pub fn can_approve(&self) -> bool {
        let live = self.live.lock();
        self.devices.lock().iter().any(|d| {
            !d.revoked && d.scopes.contains(&Scope::Approve) && live.contains_key(&d.node_id)
        })
    }

    /// Node IDs of phones currently connected (at least one live stream).
    pub fn live_node_ids(&self) -> Vec<String> {
        self.live.lock().keys().cloned().collect()
    }

    fn track(&self, node: &str, c: &Connection) {
        self.live
            .lock()
            .entry(node.to_string())
            .or_default()
            .push(c.clone());
        self.touch(node);
    }

    fn untrack(&self, node: &str, c: &Connection) {
        {
            let mut l = self.live.lock();
            if let Some(v) = l.get_mut(node) {
                v.retain(|x| x.stable_id() != c.stable_id());
                if v.is_empty() {
                    l.remove(node);
                }
            }
        }
        self.touch(node);
    }

    /// Remember that the phone was here now (a connection opened or closed). Informational: a failed save is
    /// logged, never a reason to refuse the phone.
    pub(crate) fn touch(&self, node: &str) {
        let mut all = self.devices.lock();
        if let Some(d) = all.iter_mut().find(|d| d.node_id == node && !d.revoked) {
            d.last_seen = Some(now_s());
            if let Err(e) = self.save(&all) {
                eprintln!("hostd: mobile: saving last seen: {e}");
            }
        }
    }

    /// The paired phones as Settings shows them: every `Device` field plus `connected` (a stream is open
    /// now) and `last_seen`: when the phone last connected or disconnected, or acted (`audit.log`) if later.
    pub fn devices_view(&self) -> Vec<Value> {
        let audit = self.audit_tail();
        let live = self.live_node_ids();
        self.devices()
            .into_iter()
            .map(|d| {
                let acted = audit
                    .iter()
                    .filter(|e| e["node_id"] == d.node_id.as_str())
                    .filter_map(|e| e["ts"].as_u64())
                    .max();
                let seen = d.last_seen.max(acted);
                let mut v = json!(d);
                v["connected"] = json!(!d.revoked && live.contains(&d.node_id));
                if let Some(s) = seen {
                    v["last_seen"] = json!(s);
                }
                v
            })
            .collect()
    }

    /// What a phone did, newest first: `{ts, cmd, args}` from `audit.log` (typed text is only a length there).
    pub fn activity(&self, node_id: &str, limit: usize) -> Vec<Value> {
        self.audit_tail()
            .into_iter()
            .rev()
            .filter(|e| e["node_id"] == node_id)
            .take(limit.min(MAX_ACTIVITY))
            .map(|e| json!({"ts": e["ts"], "cmd": e["cmd"], "args": e["args"]}))
            .collect()
    }

    /// The last [`AUDIT_TAIL`] bytes of `audit.log`, parsed (a cut first line and unreadable lines are skipped).
    fn audit_tail(&self) -> Vec<Value> {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut f) = std::fs::File::open(mobile_dir(&self.dir).join("audit.log")) else {
            return Vec::new();
        };
        let cut = f
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0)
            .saturating_sub(AUDIT_TAIL);
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(cut)).is_err() || f.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&buf);
        let mut lines = text.lines();
        if cut > 0 {
            lines.next();
        }
        lines
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .collect()
    }

    /// This endpoint's dialable address (tests, `mobile_pair_start`).
    pub fn endpoint(&self) -> Option<Endpoint> {
        self.endpoint.lock().clone()
    }
}
