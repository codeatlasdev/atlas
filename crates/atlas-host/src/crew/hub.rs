//! The crew server's shared state: the store, push tokens, the Telegram bridge, the per-crewmate
//! runners and the waiters a task parks on (an answer to `ask_user`, an action, a GitHub login).
//! `start` brings it up when hostd does: schedulers, triggers, Telegram, approval forwarding, and the
//! recovery of tasks a restart interrupted.

use std::sync::Arc;

use parking_lot::Mutex;

use super::github::GithubLoginHandle;
use super::store::{self, CrewStore, TaskState};
use super::telegram::TelegramBridge;
use super::{PushToken, now_s, pane, push, runner, schedule, triggers};
use crate::server::Hub;

pub(crate) struct CrewHub {
    pub(crate) store: CrewStore,
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
    /// In-progress device-flow login, if any.
    pub(crate) github_login: tokio::sync::Mutex<Option<GithubLoginHandle>>,
    /// Senders that wake when GitHub gets connected (connect_github action kind).
    pub(crate) github_connect_waiters: parking_lot::Mutex<Vec<tokio::sync::oneshot::Sender<()>>>,
    /// Senders for user-input actions keyed by task_id.
    pub(crate) action_waiters:
        parking_lot::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<String>>>,
}

impl CrewHub {
    pub(crate) fn new(dir: &std::path::Path) -> Arc<Self> {
        let push_tokens = push::load_tokens(dir);
        let telegram = Arc::new(TelegramBridge::new(dir.to_path_buf()));
        Arc::new(Self {
            store: CrewStore::new(dir),
            dir: dir.to_path_buf(),
            push_tokens: Mutex::new(push_tokens),
            telegram,
            runners: Arc::new(runner::RunnerMap::new()),
            hub_ref: std::sync::OnceLock::new(),
            ask_waiters: parking_lot::Mutex::new(std::collections::HashMap::new()),
            github_login: tokio::sync::Mutex::new(None),
            github_connect_waiters: parking_lot::Mutex::new(vec![]),
            action_waiters: parking_lot::Mutex::new(std::collections::HashMap::new()),
        })
    }

    pub(crate) fn start(self: &Arc<Self>, hub: std::sync::Weak<Hub>) {
        let _ = self.hub_ref.set(hub.clone());
        self.telegram.set_hub(hub.clone());
        self.telegram.clone().start();
        schedule::start(hub.clone());
        triggers::start(hub.clone());
        forward_approvals(hub.clone());
        if let Some(hub_arc) = hub.upgrade() {
            self.recover_tasks(&hub_arc);
        }
    }

    /// Re-enqueue any tasks that were Queued/Running when the daemon last stopped.
    /// Mark Running → Failed (interrupted), leave Queued for re-enqueue.
    fn recover_tasks(&self, hub: &Arc<Hub>) {
        for crewmate in &self.store.list_crewmates() {
            let tasks = self
                .store
                .read_tasks(&crewmate.id, store::default_task_limit());
            for t in tasks.into_iter().rev() {
                match t.state {
                    TaskState::Queued => {
                        runner::enqueue(hub, &self.store, crewmate, t);
                    }
                    TaskState::Running | TaskState::WaitingAction => {
                        let mut t2 = t;
                        t2.state = TaskState::Failed;
                        t2.action = None;
                        t2.error = Some("interrupted by daemon restart".to_string());
                        t2.finished_at = Some(now_s());
                        let _ = self.store.append_task(&crewmate.id, &t2);
                    }
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn can_answer_crewmate_pane(&self) -> bool {
        if self.telegram.has_linked_chat() || !self.push_tokens.lock().is_empty() {
            return true;
        }
        self.hub_arc()
            .is_some_and(|h| !h.subscribers.lock().is_empty())
    }

    /// Drop all push tokens for a revoked device.
    pub(crate) fn revoke_device_tokens(&self, node_id: &str) {
        let mut tokens = self.push_tokens.lock();
        tokens.retain(|t| t.node_id != node_id);
        push::save_tokens(&self.dir, &tokens);
    }

    pub(crate) fn hub_arc(&self) -> Option<Arc<Hub>> {
        self.hub_ref.get()?.upgrade()
    }

    /// Wake a blocked ask_user call with its answer. Returns true if someone was waiting.
    pub(crate) fn answer_ask(&self, ask_id: &str, answer: store::AskAnswer) -> bool {
        if let Some(tx) = self.ask_waiters.lock().remove(ask_id) {
            let _ = tx.send(answer);
            true
        } else {
            false
        }
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

    /// Deliver an answer to a waiting user-input action. Returns true if someone waited.
    pub(crate) fn answer_action(&self, task_id: &str, value: String) -> bool {
        if let Some(tx) = self.action_waiters.lock().remove(task_id) {
            let _ = tx.send(value);
            true
        } else {
            false
        }
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
