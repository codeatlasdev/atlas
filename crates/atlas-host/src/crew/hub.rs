//! The crew server's shared state: the store, push tokens, the Telegram bridge, the per-crewmate
//! runners and the waiters a task parks on (an answer to `ask_user`, an action, a GitHub login).
//! `start` brings it up when hostd does: schedulers, triggers, Telegram, approval forwarding, and the
//! recovery of tasks a restart interrupted (`runner::recover`).

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;

use super::audit::{self, Audit};
use super::github::app::{AppService, Routed};
use super::github::{GithubApi, GithubLoginHandle};
use super::store::{self, CrewStore};
use super::telegram::TelegramBridge;
use super::{PushToken, pane, push, runner, schedule, triggers};
use crate::server::Hub;

pub(crate) struct CrewHub {
    pub(crate) store: CrewStore,
    /// The append-only audit (`audit::Audit`): the one writer of every `audit.jsonl`.
    pub(crate) audit: Arc<Audit>,
    pub(crate) dir: std::path::PathBuf,
    /// Live push tokens.
    pub(crate) push_tokens: Mutex<Vec<PushToken>>,
    /// Telegram bridge.
    pub(crate) telegram: Arc<TelegramBridge>,
    /// Runner state per crewmate.
    pub(crate) runners: Arc<runner::RunnerMap>,
    hub_ref: std::sync::OnceLock<std::sync::Weak<Hub>>,
    /// Pending ask_user waiters keyed by ask_id.
    pub(crate) ask_waiters: parking_lot::Mutex<
        std::collections::HashMap<String, tokio::sync::oneshot::Sender<store::AskAnswer>>,
    >,
    /// The server's GitHub App: its key, its installation tokens (`github::app`).
    pub(crate) github_app: Arc<AppService>,
    /// The `GithubApi` the crew uses: the app for a repo it covers, `gh` for any other.
    github_api: Arc<Routed>,
    /// In-progress device-flow login, if any.
    pub(crate) github_login: tokio::sync::Mutex<Option<GithubLoginHandle>>,
    /// Senders that wake when GitHub gets connected (connect_github action kind).
    pub(crate) github_connect_waiters: parking_lot::Mutex<Vec<tokio::sync::oneshot::Sender<()>>>,
    /// Senders for user-input actions keyed by task_id.
    pub(crate) action_waiters:
        parking_lot::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>>,
    /// The GitHub reply of each running task that came from GitHub, by task id (`runner::reporting`).
    pub(crate) reporters:
        parking_lot::Mutex<std::collections::HashMap<String, runner::ReportingLive>>,
    /// What each running task ran, for the guards (`runner::guard`).
    pub(crate) tape: runner::Tape,
    /// The cancel handle of each running engine turn, by task id: `runner::cancel_with` fires it, which
    /// kills the turn's process tree (an engine turn has no pty session to kill).
    pub(crate) turn_cancels: Mutex<std::collections::HashMap<String, super::engine::TurnCancel>>,
    /// The kill switch, mirrored from `crew.json` (`paused`): no task runs, no automation fires.
    paused: std::sync::atomic::AtomicBool,
    /// How many agents run at once (`max_agents` in `crew.json`); a task past that waits "server busy".
    pub(crate) agent_slots: Arc<runner::AgentSlots>,
    /// Engines whose sign-in a failed task reported gone (`engine_login`).
    pub(crate) engine_auth: super::engine_login::EngineAuth,
    /// The last `signed_in` / `needs_signin` / ... answer per engine, for `crew_engine_status` without refresh.
    pub(crate) engine_auth_seen: Mutex<std::collections::HashMap<String, String>>,
    /// Task grants, what each agent was told about its mode, and the wake-up of waiting calls (`approval`).
    pub(crate) approvals: super::approval::State,
}

impl CrewHub {
    pub(crate) fn new(
        dir: &std::path::Path,
        isolation: Option<super::spawn::layout::Setup>,
        isolation_fault: Option<String>,
    ) -> Arc<Self> {
        let push_tokens = push::load_tokens(dir);
        let store = CrewStore::new(dir)
            .with_isolation(isolation)
            .with_isolation_fault(isolation_fault);
        let settings = store.load_crew_settings();
        let (paused, max_agents) = (settings.paused, settings.max_agents);
        let telegram = Arc::new(TelegramBridge::new(dir.to_path_buf()));
        let audit = Arc::new(Audit::new(dir));
        let github_app = Arc::new(AppService::new(dir, audit.clone()));
        Arc::new(Self {
            store,
            github_api: Arc::new(Routed::new(github_app.clone(), audit.clone())),
            github_app,
            audit,
            dir: dir.to_path_buf(),
            push_tokens: Mutex::new(push_tokens),
            telegram,
            runners: Arc::new(runner::RunnerMap::new()),
            hub_ref: std::sync::OnceLock::new(),
            ask_waiters: parking_lot::Mutex::new(std::collections::HashMap::new()),
            github_login: tokio::sync::Mutex::new(None),
            github_connect_waiters: parking_lot::Mutex::new(vec![]),
            action_waiters: parking_lot::Mutex::new(std::collections::HashMap::new()),
            reporters: parking_lot::Mutex::new(std::collections::HashMap::new()),
            tape: runner::Tape::default(),
            turn_cancels: Mutex::new(std::collections::HashMap::new()),
            paused: std::sync::atomic::AtomicBool::new(paused),
            agent_slots: runner::AgentSlots::new(max_agents),
            engine_auth: Default::default(),
            engine_auth_seen: Mutex::new(std::collections::HashMap::new()),
            approvals: Default::default(),
        })
    }

    /// Whether the kill switch is on. Every path that would start a task or fire an automation asks.
    pub(crate) fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Flip the kill switch (`crew_pause` / `crew_resume`), store it in `crew.json` and tell the
    /// clients. Pausing cancels every running task (`cancelled`, error "paused") and keeps the
    /// queues; resuming starts them. Doing either twice changes nothing.
    pub(crate) fn set_paused(&self, hub: &Hub, paused: bool) -> Result<(), String> {
        // An unreadable crew.json holds the crew paused. Pausing again is safe to write; resuming is the
        // user's explicit decision to leave that state: keep the old bytes aside and start from a clean file.
        self.store
            .recover_crew_settings()
            .map_err(|e| format!("recover_crew_settings: {e}"))?;
        let mut settings = self.store.load_crew_settings();
        settings.paused = paused;
        self.store
            .save_crew_settings(&settings)
            .map_err(|e| format!("save_crew_settings: {e}"))?;
        let was = self
            .paused
            .swap(paused, std::sync::atomic::Ordering::SeqCst);
        if paused {
            runner::cancel_all(hub, "paused");
        } else if was {
            runner::resume_all(hub);
        }
        if was != paused {
            self.audit
                .record(None, if paused { "paused" } else { "resumed" }, json!({}));
        }
        hub.broadcast(super::events::crew_paused(paused));
        Ok(())
    }

    pub(crate) fn start(self: &Arc<Self>, hub: std::sync::Weak<Hub>) {
        let _ = self.hub_ref.set(hub.clone());
        // Uploads abandoned before this start (an app killed mid-send) are thrown away now, not at the next
        // upload of their crewmate, which may never come.
        super::attachments::sweep_stale_uploads(&self.store);
        super::voice::sweep_at_start(&self.dir);
        self.telegram.set_hub(hub.clone());
        self.telegram.clone().start();
        schedule::start(hub.clone());
        triggers::start(hub.clone());
        audit::observe(hub.clone());
        runner::observe_tape(hub.clone());
        forward_approvals(hub.clone());
        if let Some(hub_arc) = hub.upgrade() {
            runner::recover(&hub_arc);
        }
    }

    /// The GitHub API the crew uses: the one place that chooses between the GitHub App (a repo an installation
    /// covers) and the `gh` CLI as the crew user (everything else, and every server without an app). A fake under test.
    pub(crate) fn github(&self) -> Arc<dyn GithubApi> {
        #[cfg(test)]
        if let Some(api) = super::github::TEST_GITHUB_API.lock().clone() {
            return api;
        }
        self.github_api.clone()
    }

    /// Whether anyone can answer a crewmate's tool prompt right now: a linked Telegram chat, a
    /// desktop client connected, or a phone connected. A registered push token is not one: it only
    /// says where to notify, and with the phone offline nobody would ever answer (the prompt would
    /// hold the task for its whole 30 minutes).
    pub(crate) fn can_answer_crewmate_pane(&self) -> bool {
        if self.telegram.has_linked_chat() {
            return true;
        }
        self.hub_arc().is_some_and(|h| {
            !h.subscribers.lock().is_empty() || !h.mobile.live_node_ids().is_empty()
        })
    }

    /// `push_register` from `node_id`: the paired phone that sent it (a `phone:<node>` connection),
    /// or `""` for a client that is not a phone (a crew server only hears the Mac's relay). The node
    /// id is what lets revoking that phone take its tokens, and lets a connected phone be left out
    /// of a push (`mobile.live_node_ids`). The same token registered again replaces its entry.
    pub(crate) fn register_token(&self, node_id: &str, token: &str, platform: &str, grant: String) {
        let mut tokens = self.push_tokens.lock();
        tokens.retain(|t| t.token != token);
        tokens.push(push::new_token(node_id, token, platform, grant));
        push::save_tokens(&self.dir, &tokens);
    }

    /// `push_unregister`.
    pub(crate) fn unregister_token(&self, token: &str) {
        let mut tokens = self.push_tokens.lock();
        tokens.retain(|t| t.token != token);
        push::save_tokens(&self.dir, &tokens);
    }

    /// Drop all push tokens for a revoked device.
    pub(crate) fn revoke_device_tokens(&self, node_id: &str) {
        let mut tokens = self.push_tokens.lock();
        tokens.retain(|t| t.node_id != node_id);
        push::save_tokens(&self.dir, &tokens);
    }

    /// Drop the tokens the relay will never accept again (it answered 403 or 410).
    pub(crate) fn drop_tokens(&self, expired: &[String]) {
        let mut tokens = self.push_tokens.lock();
        tokens.retain(|t| !expired.contains(&t.token));
        push::save_tokens(&self.dir, &tokens);
    }

    pub(crate) fn hub_arc(&self) -> Option<Arc<Hub>> {
        self.hub_ref.get()?.upgrade()
    }

    /// Whether an `ask_user` call is blocked on this question right now.
    pub(crate) fn ask_waiting(&self, ask_id: &str) -> bool {
        self.ask_waiters
            .lock()
            .get(ask_id)
            .is_some_and(|tx| !tx.is_closed())
    }

    /// Wake a blocked ask_user call with its answer. Returns true only if the call took it.
    pub(crate) fn answer_ask(&self, ask_id: &str, answer: store::AskAnswer) -> bool {
        self.ask_waiters
            .lock()
            .remove(ask_id)
            .is_some_and(|tx| tx.send(answer).is_ok())
    }

    /// Register a waiter for an ask_user call; returns a receiver to await.
    pub(crate) fn wait_ask(
        &self,
        ask_id: String,
    ) -> tokio::sync::oneshot::Receiver<store::AskAnswer> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.ask_waiters.lock().insert(ask_id, tx);
        rx
    }

    /// Signal all connect_github waiters that GitHub is now connected.
    pub(crate) fn notify_github_connected(&self) {
        let waiters: Vec<_> = std::mem::take(&mut *self.github_connect_waiters.lock());
        for tx in waiters {
            let _ = tx.send(());
        }
    }

    /// Register a one-shot waiter for a user-input action; returns the receiver.
    pub(crate) fn wait_action(&self, task_id: String) -> tokio::sync::oneshot::Receiver<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.action_waiters.lock().insert(task_id, tx);
        rx
    }

    /// Whether a runner is waiting for an answer to this task's action right now (so an answer would be read).
    pub(crate) fn action_waiting(&self, task_id: &str) -> bool {
        self.action_waiters
            .lock()
            .get(task_id)
            .is_some_and(|tx| !tx.is_closed())
    }

    /// Deliver an answer to a waiting user-input action. Returns true only if a runner took it.
    pub(crate) fn answer_action(&self, task_id: &str, value: String) -> bool {
        self.action_waiters
            .lock()
            .remove(task_id)
            .is_some_and(|tx| tx.send(value).is_ok())
    }

    /// Cancel a waiting user-input action (drops the sender → receiver gets Err).
    pub(crate) fn cancel_action(&self, task_id: &str) {
        self.action_waiters.lock().remove(task_id);
    }
}

/// Forward an approval a crewmate's agent is waiting for to Telegram, and to the phones that are not
/// connected (push), as agent events arrive. Runs until hostd stops.
fn forward_approvals(hub_w: std::sync::Weak<Hub>) {
    tokio::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        let Some(h) = hub_w.upgrade() else { return };
        let mut rx = h.agents.subscribe();
        loop {
            let ev = match rx.recv().await {
                Ok(v) => v,
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            };
            if ev["ev"] != "agent_approval" {
                continue;
            }
            let pane = match ev["pane_key"].as_str() {
                Some(p) if pane::is_crewmate_pane(p) => p.to_string(),
                _ => continue,
            };
            let Some(h) = hub_w.upgrade() else { return };
            let crewmate_id = pane::crewmate_id(&pane).unwrap_or("");
            let crewmate_name = h
                .crew
                .store
                .load_crewmate(crewmate_id)
                .ok()
                .map(|b| b.name)
                .unwrap_or_default();
            let req = ev["req"].as_str().unwrap_or_default();
            h.crew.telegram.send_approval(
                &crewmate_name,
                ev["tool_name"].as_str().unwrap_or("?"),
                &ev["tool_input"],
                req,
                &pane,
            );
            let tokens: Vec<_> = h.crew.push_tokens.lock().clone();
            let live = h.mobile.live_node_ids();
            let offline: Vec<_> = tokens
                .into_iter()
                .filter(|t| !live.contains(&t.node_id))
                .collect();
            if !offline.is_empty() {
                tokio::spawn(async move {
                    crate::crew::push::notify(&offline, "needs_you").await;
                });
            }
        }
    });
}
