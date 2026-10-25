//! The lab lane: a SHADOW hostd that lets a branch's hostd code (and a phone build of that branch)
//! be validated against the maintainer's real agents without touching his installation.
//!
//! `atlas hostd --lab` runs a second daemon in its own directory (`~/.atlas/lab`: socket, pid,
//! token, mobile key, `devices.json`, invites, audit log; it never writes under the main one). It
//! is a *client* of the main daemon's socket:
//!
//! - **Mirrors** what main knows about agents: it seeds from `agent_status_all` and follows the
//!   `agent_status` / `agent_approval` / `agent_approval_done` pushes into its own
//!   [`AgentHub`](crate::agents::AgentHub), so `agent_tree`, `agent_history` and `project_*` run
//!   with THIS binary's code over the very same session files and project registry.
//! - **Forwards** what acts on terminals and agents (see [`route`]): `pty_list`, `pty_read`,
//!   `pty_write`, `pty_signal`, `agent_send`, `agent_interrupt`, `agent_approve`, `agent_launch`.
//!   Terminal output reaches the phone through a connection per attached phone ([`Link`]).
//! - **Owns nothing of main's**: it never starts the hook server (that would steal the endpoint
//!   file the user's agents read), never adopts or hands off ptys and refuses to create, kill or
//!   resize one. A prompt is held by main only for main's own phones, so the lab never makes a real
//!   agent wait on an experimental phone.
//!
//! It outlives main restarting (every `atlas update` restarts the maintainer's hostd): forwarded
//! calls fail with a clear error meanwhile and the follower reconnects and re-seeds.

#[cfg(all(test, unix))]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::HostClient;
use crate::agents::AgentStatus;
use crate::protocol::HostCmd;
use crate::pty::Out;
use crate::server::Hub;

/// Directory name of the lab under the main hostd dir (`~/.atlas/lab`).
pub const DIR_NAME: &str = "lab";

/// What the lab does with one command.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// Run in main's hostd and return its answer (terminals and agent actions).
    Main,
    /// Answer here, with this binary's code (agent history/tree, projects, pairing, info).
    Local,
    /// `pty_attach` / `pty_detach`: through the connection's own [`Link`] to main.
    Attach,
    /// Never done by a lab: it mirrors terminals and agents, it does not own them.
    Refuse(&'static str),
}

/// The route of every command. Exhaustive on purpose: a new `HostCmd` must choose.
pub fn route(cmd: &HostCmd) -> Route {
    use HostCmd as C;
    match cmd {
        C::PtyList {}
        | C::PtyInfo { .. }
        | C::PtyRead { .. }
        | C::PtyWrite { .. }
        | C::PtySignal { .. }
        | C::AgentSend { .. }
        | C::AgentInterrupt { .. }
        | C::AgentApprove { .. }
        | C::AgentLaunch { .. } => Route::Main,
        C::PtyAttach { .. } | C::PtyDetach { .. } => Route::Attach,
        C::PtyCreate(_) | C::PtyKill { .. } | C::PtyClear { .. } | C::PtyResize { .. } => {
            Route::Refuse("lab hostd mirrors terminals, it never creates, kills or resizes one")
        }
        C::PtyAttachRaw { .. } => Route::Refuse("lab hostd has no raw terminal attach"),
        C::AgentBind { .. } | C::AgentPaneExited { .. } => {
            Route::Refuse("lab hostd mirrors agents from main, it is not told about panes")
        }
        C::Upgrade { .. } => {
            Route::Refuse("lab hostd has no live upgrade: restart it with `bun run mobile:lab`")
        }
        C::Exec { .. }
        | C::FsList { .. }
        | C::FsRead { .. }
        | C::FsWrite { .. }
        | C::FsStat { .. }
        | C::FsMkdir { .. }
        | C::FsRename { .. }
        | C::FsRemove { .. }
        | C::FsWatch { .. }
        | C::FsUnwatch { .. }
        | C::Info {}
        | C::AgentTree { .. }
        | C::AgentHistory { .. }
        | C::AgentStatusAll {}
        | C::AgentIntegrations {}
        | C::ProjectList {}
        | C::ProjectDetail { .. }
        | C::ProjectLogs { .. }
        | C::ProjectService { .. }
        | C::ProjectAction { .. }
        | C::MobilePairStart {}
        | C::MobileDevices {}
        | C::MobileRevoke { .. }
        | C::MobileSetScopes { .. }
        | C::Shutdown {}
        | C::LspOpen { .. }
        | C::LspChange { .. }
        | C::LspClose { .. }
        | C::LspSave { .. }
        | C::LspRequest { .. }
        | C::LspCancel { .. }
        | C::LspStatus {}
        | C::LspCatalog {}
        | C::LspInstall { .. }
        | C::LspUninstall { .. }
        | C::LspRestart { .. }
        | C::LspLog { .. }
        | C::LspApplyEditResult { .. } => Route::Local,
    }
}

/// A lab's answer to one request, or `None` when this binary's own code should answer it
/// (`Route::Local`). `name`/`args` are the wire request, so a forwarded call reaches main exactly as
/// it was sent. `Attach` is handled by the connection ([`Link`]), never here.
pub(crate) async fn dispatch(
    lab: &Lab,
    cmd: &HostCmd,
    name: &str,
    args: &Value,
) -> Option<Result<Value, String>> {
    match route(cmd) {
        Route::Main => Some(lab.call(name, args.clone()).await),
        Route::Refuse(why) => Some(Err(why.to_string())),
        Route::Local | Route::Attach => None,
    }
}

/// The main daemon this lab follows, and the shared connection to it.
pub struct Lab {
    /// Main's directory: the project registry is read from here, never written.
    main_dir: PathBuf,
    client: Mutex<Option<HostClient>>,
}

/// Why a forwarded call could not reach main.
const MAIN_DOWN: &str = "the main atlas hostd is not reachable (restarting?); the lab keeps trying";

/// What `serve_with` needs before it binds anything: a reachable main hostd and a lab dir that is
/// not main's. The first connection is returned seeded, so a lab never starts empty by accident.
pub async fn connect(main_dir: &Path, lab_dir: &Path) -> Result<(Arc<Lab>, HostClient)> {
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    if same(main_dir, lab_dir) {
        return Err(anyhow!(
            "lab dir must differ from the main hostd dir ({}): the lab never touches main's files",
            main_dir.display()
        ));
    }
    let client = HostClient::connect_dir(main_dir, "lab")
        .await
        .map_err(|e| {
            anyhow!(
                "no main atlas hostd at {} ({e:#}): start Atlas (or `atlas hostd`) first; the lab \
             follows your real hostd and does not run without it",
                main_dir.display()
            )
        })?;
    let lab = Arc::new(Lab {
        main_dir: main_dir.to_path_buf(),
        client: Mutex::new(Some(client.clone())),
    });
    Ok((lab, client))
}

impl Lab {
    pub fn main_dir(&self) -> &Path {
        &self.main_dir
    }

    /// Forward one command to main and return its reply (its errors verbatim).
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value, String> {
        let client = self.client.lock().clone().ok_or(MAIN_DOWN)?;
        client.call(cmd, args).await.map_err(|e| {
            let msg = format!("{e:#}");
            if msg.contains("disconnected") {
                MAIN_DOWN.to_string()
            } else {
                msg
            }
        })
    }

    /// `(pane_key, cwd)` of main's live terminals that belong to a pane: what the project list
    /// needs to attach agents to projects (the lab owns no terminal itself).
    pub async fn panes(&self) -> Vec<(String, String)> {
        let Ok(list) = self.call("pty_list", json!({})).await else {
            return Vec::new();
        };
        list["ptys"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["exited"].is_null())
            .filter_map(|p| {
                Some((
                    p["scope"]["pane_key"].as_str()?.to_string(),
                    p["cwd"].as_str()?.to_string(),
                ))
            })
            .collect()
    }

    /// Seed from `agent_status_all` and follow main's pushes until its connection drops, then
    /// reconnect with a growing pause. `first` is the connection made at startup.
    pub(crate) async fn follow(self: Arc<Self>, hub: std::sync::Weak<Hub>, first: HostClient) {
        let mut next = Some(first);
        let mut pause = Duration::from_millis(250);
        loop {
            let client = match next.take() {
                Some(c) => c,
                None => match HostClient::connect_dir(&self.main_dir, "lab").await {
                    Ok(c) => c,
                    Err(_) => {
                        tokio::time::sleep(pause).await;
                        pause = (pause * 2).min(Duration::from_secs(5));
                        if hub.strong_count() == 0 {
                            return;
                        }
                        continue;
                    }
                },
            };
            pause = Duration::from_millis(250);
            // Subscribe BEFORE the snapshot: what happens meanwhile is buffered, not lost.
            let mut events = client.events();
            let Some(h) = hub.upgrade() else { return };
            match seed(&h, &client).await {
                Ok(()) => *self.client.lock() = Some(client.clone()),
                Err(e) => {
                    eprintln!("hostd lab: could not seed from main: {e:#}");
                    tokio::time::sleep(pause).await;
                    continue;
                }
            }
            drop(h);
            loop {
                let ev = match events.recv().await {
                    Ok(ev) => ev,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Missed pushes: the snapshot is the truth again.
                        let Some(h) = hub.upgrade() else { return };
                        let _ = seed(&h, &client).await;
                        continue;
                    }
                    Err(_) => break,
                };
                let Some(h) = hub.upgrade() else { return };
                match ev["ev"].as_str() {
                    Some("agent_status") => {
                        if let (Some(pane), Ok(st)) = (
                            ev["pane_key"].as_str(),
                            serde_json::from_value::<AgentStatus>(ev["status"].clone()),
                        ) {
                            h.agents.mirror_status(pane, st);
                        }
                    }
                    Some("agent_approval") => h.agents.mirror_approval((*ev).clone()),
                    Some("agent_approval_done") => h.agents.mirror_approval_done(
                        ev["req"].as_str().unwrap_or_default(),
                        ev["pane_key"].as_str().unwrap_or_default(),
                    ),
                    Some("disconnected") => break,
                    _ => {}
                }
            }
            *self.client.lock() = None;
        }
    }
}

/// One `agent_status_all` from main into the lab's own hub.
async fn seed(hub: &Hub, client: &HostClient) -> Result<()> {
    let all = client.call("agent_status_all", json!({})).await?;
    let panes: HashMap<String, AgentStatus> =
        serde_json::from_value(all["panes"].clone()).unwrap_or_default();
    let approvals = all["pending_approvals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    hub.agents.mirror_all(panes, approvals);
    Ok(())
}

/// A connection's attach state in a lab: its own connection to main, on which the terminals this
/// phone attached are subscribed, so main's `pty_*` pushes can be relayed back to this phone alone
/// and dropping it detaches everything on main. One per phone session (or unix connection).
#[derive(Default)]
pub struct Link {
    client: Option<HostClient>,
}

impl Link {
    /// `pty_attach` on main through this connection's own client; its `pty_*` pushes go to `push`.
    pub async fn attach(&mut self, lab: &Lab, args: Value, push: Out) -> Result<Value, String> {
        if self.client.is_none() {
            let c = HostClient::connect_dir(&lab.main_dir, "lab-attach")
                .await
                .map_err(|_| MAIN_DOWN.to_string())?;
            // Subscribed before the attach: the replay is the first thing main pushes.
            let mut events = c.events();
            tokio::spawn(async move {
                use tokio::sync::broadcast::error::RecvError;
                loop {
                    match events.recv().await {
                        Ok(ev) => {
                            let relay = ev["ev"].as_str().is_some_and(|e| e.starts_with("pty_"));
                            if relay && push.send(ev.to_string()).is_err() {
                                return;
                            }
                            if ev["ev"] == "disconnected" {
                                return;
                            }
                        }
                        Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => return,
                    }
                }
            });
            self.client = Some(c);
        }
        let res = self
            .client
            .as_ref()
            .expect("set above")
            .call("pty_attach", args)
            .await
            .map_err(|e| format!("{e:#}"));
        if res.as_ref().is_err_and(|e| e.contains("disconnected")) {
            self.client = None;
            return Err(MAIN_DOWN.into());
        }
        res
    }

    /// `pty_detach` on main for a terminal this connection attached.
    pub async fn detach(&mut self, args: Value) -> Result<Value, String> {
        let Some(c) = &self.client else {
            return Ok(json!({}));
        };
        c.call("pty_detach", args)
            .await
            .map_err(|e| format!("{e:#}"))
    }
}
