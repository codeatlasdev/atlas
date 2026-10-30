//! Bot subsystem: hub, public API, and HostCmd routing.

pub(crate) mod engine;
pub mod mcp;
pub(crate) mod project;
pub(crate) mod push;
pub(crate) mod runner;
pub(crate) mod schedule;
pub(crate) mod store;
pub(crate) mod telegram;
#[cfg(test)]
mod tests;
pub(crate) mod triggers;

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::bots::telegram::TelegramBridge;
use crate::paths;
use crate::protocol::HostCmd;
use crate::server::Hub;

pub(crate) use push::PushToken;
pub(crate) use store::{Approval, Bot, BotStore, Routine, Task, TaskState};

fn default_bot_task_limit() -> usize {
    50
}

// ─── GitHub device-flow login state ──────────────────────────────────────────

/// In-progress `gh auth login` flow.
#[allow(dead_code)] // fields read via json! macro in handlers
pub(crate) struct GithubLoginHandle {
    pub user_code: String,
    pub verification_uri: String,
    /// Shared state updated by the background waiter.
    pub state: Arc<parking_lot::Mutex<GithubLoginState>>,
}

#[derive(Debug, Clone)]
pub(crate) enum GithubLoginState {
    Pending,
    Done,
    Error(String),
}

// ─── BotHub ──────────────────────────────────────────────────────────────────

pub(crate) struct BotHub {
    pub(crate) store: BotStore,
    pub(crate) dir: std::path::PathBuf,
    /// Live push tokens.
    pub(crate) push_tokens: Mutex<Vec<PushToken>>,
    /// Telegram bridge.
    pub(crate) telegram: Arc<TelegramBridge>,
    /// Runner state per bot.
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

impl BotHub {
    pub(crate) fn new(dir: &std::path::Path) -> Arc<Self> {
        let push_tokens = push::load_tokens(dir);
        let telegram = Arc::new(TelegramBridge::new(dir.to_path_buf()));
        Arc::new(Self {
            store: BotStore::new(dir),
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

        // Subscribe to agent_approval events for bot panes → telegram + push.
        {
            let hub_w = hub.clone();
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
                        Some(p) if p.starts_with("bot:") => p.to_string(),
                        _ => continue,
                    };
                    let Some(h) = hub_w.upgrade() else { return };
                    let bot_id = pane.split(':').nth(1).unwrap_or("");
                    let bot_name = h
                        .bots
                        .store
                        .load_bot(bot_id)
                        .ok()
                        .map(|b| b.name)
                        .unwrap_or_default();
                    let req = ev["req"].as_str().unwrap_or_default();
                    h.bots.telegram.send_approval(
                        &bot_name,
                        ev["tool_name"].as_str().unwrap_or("?"),
                        &ev["tool_input"],
                        req,
                        &pane,
                    );
                    let tokens: Vec<_> = h.bots.push_tokens.lock().clone();
                    let live = h.mobile.live_node_ids();
                    let offline: Vec<_> = tokens
                        .into_iter()
                        .filter(|t| !live.contains(&t.node_id))
                        .collect();
                    if !offline.is_empty() {
                        tokio::spawn(async move {
                            crate::bots::push::notify(&offline, "needs_you").await;
                        });
                    }
                }
            });
        }

        // Re-enqueue any tasks that were Queued/Running when the daemon last stopped.
        // Mark Running → Failed (interrupted), leave Queued for re-enqueue.
        let bots = self.store.list_bots();
        if let Some(hub_arc) = hub.upgrade() {
            for bot in &bots {
                let tasks = self.store.read_tasks(&bot.id, default_bot_task_limit());
                for t in tasks.into_iter().rev() {
                    match t.state {
                        TaskState::Queued => {
                            runner::enqueue(&hub_arc, &self.store, bot, t);
                        }
                        TaskState::Running | TaskState::WaitingAction => {
                            let mut t2 = t;
                            t2.state = TaskState::Failed;
                            t2.action = None;
                            t2.error = Some("interrupted by daemon restart".to_string());
                            t2.finished_at = Some(now_s());
                            let _ = self.store.append_task(&bot.id, &t2);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub(crate) fn can_answer_bot_pane(&self) -> bool {
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

/// Start checking out a Git project in the background as soon as the bot has it, so the first task does not wait for
/// the clone (~90 s on atlas-test-ship before). Failures are left to the task, which reports them (a private repo also
/// needs GitHub connected first). `project::prepare` serializes with a task that asks while this runs.
fn warm_checkout(hub: &Hub, bot: &Bot) {
    let Some(project @ store::ProjectRef::Git { .. }) = bot.project.clone() else {
        return;
    };
    let (id, dir) = (bot.id.clone(), hub.bots.dir.clone());
    tokio::spawn(async move {
        if let Err(e) = project::prepare(&project, &id, &dir).await {
            eprintln!("atlas bots: checkout for {id} not ready yet: {e}");
        }
    });
}

// ─── handle() ────────────────────────────────────────────────────────────────

/// Route bot `HostCmd` variants. Called from `server.rs handle()`.
pub(crate) async fn handle(hub: &Hub, cmd: HostCmd) -> Result<Value, String> {
    if !hub.bot_host {
        return Err("this computer is not a bot server (atlas bot install)".into());
    }
    match cmd {
        // ── BotList ─────────────────────────────────────────────────────────
        HostCmd::BotList {} => {
            let bots = hub.bots.store.list_bots();
            let list: Vec<Value> = bots
                .into_iter()
                .map(|b| {
                    let running_task = hub.bots.runners.running_task(&b.id);
                    let pending_approval = hub.agents.pending_approvals().into_iter().find(|v| {
                        v["pane_key"]
                            .as_str()
                            .map(|pk| pk.starts_with(&format!("bot:{}:", b.id)))
                            .unwrap_or(false)
                    });
                    let state = if pending_approval.is_some() {
                        "needs_you"
                    } else if running_task.is_some() {
                        "working"
                    } else {
                        "idle"
                    };
                    let queued = hub.bots.runners.queue_len(&b.id);
                    let last: Option<Value> = {
                        let tasks = hub.bots.store.read_tasks(&b.id, 1);
                        tasks.first().map(|t| {
                            let text: String = t
                                .result
                                .as_deref()
                                .or(t.error.as_deref())
                                .unwrap_or(&t.prompt)
                                .chars()
                                .take(140)
                                .collect();
                            json!({
                                "text": text,
                                "state": t.state,
                                "ts": t.ts,
                            })
                        })
                    };
                    let mut v = json!({
                        "id": b.id,
                        "name": b.name,
                        "role": b.role,
                        "agent": b.agent,
                        "approval": b.approval,
                        "state": state,
                        "queued": queued,
                    });
                    if let Some(task_id) = running_task {
                        let pane_key = format!("bot:{}:{task_id}", b.id);
                        v["running_task"] = json!({ "id": task_id, "pane": pane_key });
                    }
                    if let Some(pa) = pending_approval {
                        v["pending_approval"] = pa;
                    }
                    if let Some(look) = &b.look {
                        v["look"] = json!(look);
                    }
                    if let Some(l) = last {
                        v["last"] = l;
                    }
                    if let Some(engine) = &b.engine {
                        let approval_supported = engine.kind != "opencode";
                        v["engine"] = json!({"kind": engine.kind});
                        v["approval_supported"] = json!(approval_supported);
                    }
                    if let Some(project) = &b.project {
                        v["project"] = json!(project);
                    }
                    if let Some(step) = &b.setup_step {
                        v["setup_step"] = json!(step);
                    }
                    // Include triggers with computed next_fire.
                    if !b.triggers.is_empty() {
                        let now = crate::bots::triggers::now_s_pub();
                        let triggers: Vec<Value> = b
                            .triggers
                            .iter()
                            .map(|t| {
                                let next = crate::bots::triggers::compute_next_fire(
                                    &t.kind,
                                    t.last_fired,
                                    t.tz.as_deref(),
                                    now,
                                );
                                let mut tv = serde_json::to_value(t).unwrap_or_default();
                                tv["next_fire"] = json!(next);
                                tv
                            })
                            .collect();
                        v["triggers"] = json!(triggers);
                    } else {
                        v["triggers"] = json!([]);
                    }
                    v
                })
                .collect();
            Ok(json!({ "bots": list }))
        }

        // ── BotCreate ───────────────────────────────────────────────────────
        HostCmd::BotCreate {
            name,
            role,
            agent,
            approval,
            workdir,
            look,
            engine,
            project,
            setup_step,
        } => {
            let name = name.trim().to_string();
            if name.is_empty() || name.len() > 40 {
                return Err("name must be 1–40 characters".into());
            }
            if role.len() > 2000 {
                return Err("role must be ≤ 2000 characters".into());
            }
            let approval_enum: Approval = serde_json::from_value(json!(approval))
                .map_err(|_| format!("invalid approval: {approval}"))?;
            // opencode is forced --auto; reject ask at creation time.
            if let Some(eng) = &engine
                && eng.kind == "opencode"
                && approval_enum == Approval::Ask
            {
                return Err(
                    "OpenCode bots run without approval prompts (approval must be auto)".into(),
                );
            }
            if atlas_core::agents::get(&agent).is_none() {
                return Err(format!("unknown agent '{agent}'"));
            }
            if let Some(l) = &look {
                l.validate()?;
            }
            let id = paths::new_id();
            if let Some(ref wd) = workdir {
                std::fs::create_dir_all(wd).map_err(|e| format!("cannot create workdir: {e}"))?;
            }
            let bot = Bot {
                id: id.clone(),
                name,
                role,
                agent,
                workdir,
                approval: approval_enum,
                routines: vec![],
                triggers: vec![],
                look,
                engine,
                project,
                setup_step,
                short_replies: None,
                created_at: now_s(),
            };
            hub.bots
                .store
                .save_bot(&bot)
                .map_err(|e| format!("save_bot: {e}"))?;
            let _ = hub.bots.store.write_memory(&id, "");
            warm_checkout(hub, &bot);
            Ok(serde_json::to_value(&bot).map_err(|e| e.to_string())?)
        }

        // ── BotUpdate ───────────────────────────────────────────────────────
        HostCmd::BotUpdate {
            id,
            name,
            role,
            agent,
            approval,
            look,
            engine,
            project,
            setup_step,
            short_replies,
        } => {
            let mut bot = hub
                .bots
                .store
                .load_bot(&id)
                .map_err(|_| format!("bot {id} not found"))?;
            if let Some(n) = name {
                let n = n.trim().to_string();
                if n.is_empty() || n.len() > 40 {
                    return Err("name must be 1–40 characters".into());
                }
                bot.name = n;
            }
            if let Some(r) = role {
                if r.len() > 2000 {
                    return Err("role must be ≤ 2000 characters".into());
                }
                bot.role = r;
            }
            if let Some(a) = agent {
                if atlas_core::agents::get(&a).is_none() {
                    return Err(format!("unknown agent '{a}'"));
                }
                bot.agent = a;
            }
            if let Some(ap) = approval {
                let ap_enum: Approval = serde_json::from_value(json!(ap))
                    .map_err(|_| format!("invalid approval: {ap}"))?;
                // Check updated engine kind too (engine may be set in this same call).
                let effective_kind = engine
                    .as_ref()
                    .map(|e| e.kind.as_str())
                    .or_else(|| bot.engine.as_ref().map(|e| e.kind.as_str()))
                    .unwrap_or("");
                if effective_kind == "opencode" && ap_enum == Approval::Ask {
                    return Err(
                        "OpenCode bots run without approval prompts (approval must be auto)".into(),
                    );
                }
                bot.approval = ap_enum;
            }
            if let Some(l) = look {
                l.validate()?;
                bot.look = Some(l);
            }
            if let Some(e) = engine {
                bot.engine = Some(e);
            }
            if let Some(p) = project {
                // A folder must exist on this computer now, not fail every later task ("Something else" was saved as
                // a folder by the setup once). The same check project::prepare runs before each task.
                if let crate::bots::store::ProjectRef::Path { path } = &p {
                    project::prepare_path(path)
                        .map_err(|e| format!("project folder {path}: {e}"))?;
                }
                bot.project = Some(p);
            }
            // Outer Some = caller wants to change setup_step; inner None = clear it.
            if let Some(step) = setup_step {
                bot.setup_step = step;
            }
            if let Some(sr) = short_replies {
                bot.short_replies = sr;
            }
            hub.bots
                .store
                .save_bot(&bot)
                .map_err(|e| format!("save_bot: {e}"))?;
            // Notify connected clients that the bot changed.
            hub.broadcast(json!({"ev": "bot_updated", "bot_id": bot.id}).to_string());
            warm_checkout(hub, &bot);
            Ok(serde_json::to_value(&bot).map_err(|e| e.to_string())?)
        }

        // ── BotDelete ───────────────────────────────────────────────────────
        HostCmd::BotDelete { id } => {
            // Cancel all queued/running tasks first.
            let tasks = hub.bots.store.read_tasks(&id, usize::MAX);
            for t in tasks {
                if matches!(
                    t.state,
                    TaskState::Queued | TaskState::Running | TaskState::WaitingAction
                ) {
                    let _ = runner::cancel(hub, &hub.bots.store, &id, &t.id);
                }
            }
            hub.bots
                .store
                .delete_bot_dir(&id)
                .map_err(|e| format!("delete: {e}"))?;
            Ok(json!({}))
        }

        // ── BotTask ─────────────────────────────────────────────────────────
        HostCmd::BotTask {
            id,
            prompt,
            source,
            tz,
        } => {
            let bot = hub
                .bots
                .store
                .load_bot(&id)
                .map_err(|_| format!("bot {id} not found"))?;
            if prompt.trim().is_empty() {
                return Err("prompt must not be empty".into());
            }
            let mut task = Task::new(source, prompt.trim());
            task.tz = tz;
            hub.bots
                .store
                .append_task(&id, &task)
                .map_err(|e| format!("append_task: {e}"))?;
            let task_val = serde_json::to_value(&task).map_err(|e| e.to_string())?;
            runner::enqueue(hub, &hub.bots.store, &bot, task);
            hub.broadcast(json!({"ev": "bot_task", "bot_id": id}).to_string());
            Ok(json!({ "task": task_val }))
        }

        // ── BotTasks ────────────────────────────────────────────────────────
        HostCmd::BotTasks { id, limit } => {
            let tasks = hub.bots.store.read_tasks(&id, limit);
            Ok(json!({ "tasks": tasks }))
        }

        // ── BotCancel ───────────────────────────────────────────────────────
        HostCmd::BotCancel { id, task } => {
            runner::cancel(hub, &hub.bots.store, &id, &task).map(|()| json!({}))?;
            Ok(json!({}))
        }

        // ── BotMemory ───────────────────────────────────────────────────────
        HostCmd::BotMemory { id } => {
            let text = hub.bots.store.read_memory(&id);
            Ok(json!({ "text": text }))
        }

        // ── BotMemorySet ────────────────────────────────────────────────────
        HostCmd::BotMemorySet { id, text } => {
            hub.bots
                .store
                .write_memory(&id, &text)
                .map_err(|e| format!("write_memory: {e}"))?;
            Ok(json!({}))
        }

        // ── BotRoutineSet ───────────────────────────────────────────────────
        HostCmd::BotRoutineSet { id, routines } => {
            let mut bot = hub
                .bots
                .store
                .load_bot(&id)
                .map_err(|_| format!("bot {id} not found"))?;
            // Parse and validate the routines.
            let parsed: Vec<Routine> = serde_json::from_value(json!(routines))
                .map_err(|e| format!("invalid routines: {e}"))?;
            for r in &parsed {
                schedule::validate_cron(&r.cron)?;
            }
            bot.routines = parsed;
            hub.bots
                .store
                .save_bot(&bot)
                .map_err(|e| format!("save_bot: {e}"))?;
            Ok(json!({}))
        }

        // ── PushRegister ────────────────────────────────────────────────────
        HostCmd::PushRegister { token, platform } => {
            // We need node_id; derive it from the mobile connection's node_id.
            // For now accept as a separate field; node_id is passed implicitly by the
            // mobile session layer.  We store an empty node_id when called from the desktop.
            // The node_id is injected by server.rs before routing here.
            // Use the token as a proxy for deduplication when node_id is unknown.
            let mut tokens = hub.bots.push_tokens.lock();
            // Replace existing entry with same token (any node_id).
            tokens.retain(|t| t.token != token);
            tokens.push(push::new_token("", &token, &platform));
            push::save_tokens(&hub.bots.dir, &tokens);
            Ok(json!({}))
        }

        // ── PushUnregister ──────────────────────────────────────────────────
        HostCmd::PushUnregister { token } => {
            let mut tokens = hub.bots.push_tokens.lock();
            tokens.retain(|t| t.token != token);
            push::save_tokens(&hub.bots.dir, &tokens);
            Ok(json!({}))
        }

        // ── BotTelegramSet ──────────────────────────────────────────────────
        HostCmd::BotTelegramSet { token } => {
            hub.bots
                .telegram
                .set_token(token)
                .map_err(|e| format!("set_token: {e}"))?;
            Ok(json!({}))
        }

        // ── BotTelegramLink ─────────────────────────────────────────────────
        HostCmd::BotTelegramLink {} => {
            let code = hub.bots.telegram.start_link();
            Ok(json!({ "code": code }))
        }

        // ── BotTriggersSet ──────────────────────────────────────────────────
        HostCmd::BotTriggersSet { id, triggers } => {
            let mut bot = hub
                .bots
                .store
                .load_bot(&id)
                .map_err(|_| format!("bot {id} not found"))?;
            let old_triggers = bot.triggers.clone();
            let baseline_now = now_s();
            let new_triggers = triggers
                .into_iter()
                .map(|v| {
                    let mut t: crate::bots::store::Trigger =
                        serde_json::from_value(v).map_err(|e| format!("invalid trigger: {e}"))?;
                    // Validate tz before anything else.
                    if let Some(tz_str) = &t.tz {
                        tz_str
                            .parse::<chrono_tz::Tz>()
                            .map_err(|_| format!("unknown timezone: {tz_str}"))?;
                    }
                    crate::bots::triggers::validate_trigger(&t)?;
                    // Preserve last_fired of an existing trigger with the same id;
                    // baseline new schedule/interval triggers so first fire is on-schedule.
                    if let Some(existing) = old_triggers.iter().find(|ot| ot.id == t.id) {
                        t.last_fired = existing.last_fired;
                    } else {
                        match &t.kind {
                            crate::bots::store::TriggerKind::Schedule { .. }
                            | crate::bots::store::TriggerKind::Interval { .. } => {
                                t.last_fired = Some(baseline_now);
                            }
                            _ => {}
                        }
                    }
                    Ok(t)
                })
                .collect::<Result<Vec<_>, String>>()?;
            bot.triggers = new_triggers;
            hub.bots
                .store
                .save_bot(&bot)
                .map_err(|e| format!("save_bot: {e}"))?;
            hub.broadcast(json!({"ev": "bot_updated", "bot_id": &id}).to_string());
            Ok(json!({}))
        }

        // ── BotSecretSet ────────────────────────────────────────────────────
        HostCmd::BotSecretSet { id, key, value } => {
            // Validate bot exists before storing secret.
            hub.bots
                .store
                .load_bot(&id)
                .map_err(|_| format!("bot {id} not found"))?;
            hub.bots
                .store
                .set_secret(&id, &key, &value)
                .map_err(|e| format!("set_secret: {e}"))?;
            Ok(json!({}))
        }

        // ── BotAnswer ───────────────────────────────────────────────────────
        HostCmd::BotAnswer {
            id,
            ask_id,
            key,
            text,
        } => {
            use crate::bots::store::AskAnswer;
            let answer = if let Some(k) = key {
                AskAnswer::Key { value: k }
            } else if let Some(t) = text {
                AskAnswer::Text { value: t }
            } else {
                return Err("key or text required".into());
            };
            let tasks = hub.bots.store.read_tasks(&id, usize::MAX);
            let mut updated = false;
            for mut task in tasks {
                if task.ask.as_ref().map(|a| a.id == ask_id).unwrap_or(false) {
                    if let Some(ask) = &mut task.ask {
                        ask.answered = Some(answer.clone());
                    }
                    hub.bots
                        .store
                        .append_task(&id, &task)
                        .map_err(|e| format!("append_task: {e}"))?;
                    updated = true;
                    break;
                }
            }
            if !updated {
                return Err(format!("ask {ask_id} not found for bot {id}"));
            }
            hub.bots.answer_ask(&ask_id, answer);
            Ok(json!({}))
        }

        // ── BotUsage ────────────────────────────────────────────────────────
        HostCmd::BotUsage { id, since } => {
            let records = hub.bots.store.read_usage(id.as_deref(), since.unwrap_or(0));
            Ok(json!({ "records": records }))
        }

        // ── EngineLimits ────────────────────────────────────────────────────
        HostCmd::EngineLimits {} => Ok(json!({
            "message": "no machine-readable quota or rate-limit information is exposed by any supported engine CLI",
            "limits": {}
        })),
        // ── BotGithubStatus ─────────────────────────────────────────────────
        HostCmd::BotGithubStatus {} => {
            let gh_ok = tokio::process::Command::new("gh")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if !gh_ok {
                return Ok(json!({"installed": false, "login": null}));
            }
            let out = tokio::process::Command::new("gh")
                .args(["auth", "status", "--hostname", "github.com"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .await
                .map_err(|e| format!("gh auth status: {e}"))?;
            let text = String::from_utf8_lossy(&out.stderr).to_string()
                + &String::from_utf8_lossy(&out.stdout);
            if !out.status.success() {
                return Ok(json!({"installed": true, "login": null}));
            }
            let login = parse_gh_login(&text);
            Ok(json!({"installed": true, "login": login}))
        }

        // ── BotGithubLoginStart ──────────────────────────────────────────────
        HostCmd::BotGithubLoginStart {} => {
            let home = dirs::home_dir().ok_or("no home dir")?;
            ensure_gh_installed(&home).await?;
            {
                let mut g = hub.bots.github_login.lock().await;
                *g = None;
            }
            let mut child = tokio::process::Command::new("gh")
                .args([
                    "auth",
                    "login",
                    "--hostname",
                    "github.com",
                    "--git-protocol",
                    "https",
                    "--web",
                ])
                .env("BROWSER", "false")
                .env("GH_BROWSER", "false")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("spawn gh: {e}"))?;
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let (user_code, verification_uri) =
                wait_for_gh_code(stdout, stderr, std::time::Duration::from_secs(30))
                    .await
                    .ok_or("gh auth login did not produce a device code within 30s")?;
            let state = Arc::new(parking_lot::Mutex::new(GithubLoginState::Pending));
            let state2 = Arc::clone(&state);
            tokio::spawn(async move {
                let result = child.wait().await;
                let mut s = state2.lock();
                if matches!(*s, GithubLoginState::Pending) {
                    match result {
                        Ok(status) if status.success() => *s = GithubLoginState::Done,
                        Ok(status) => {
                            *s = GithubLoginState::Error(format!(
                                "gh auth login exited with {status}"
                            ))
                        }
                        Err(e) => *s = GithubLoginState::Error(format!("wait: {e}")),
                    }
                }
            });
            let mut g = hub.bots.github_login.lock().await;
            *g = Some(GithubLoginHandle {
                user_code: user_code.clone(),
                verification_uri: verification_uri.clone(),
                state,
            });
            Ok(json!({"user_code": user_code, "verification_uri": verification_uri}))
        }

        // ── BotGithubLoginPoll ───────────────────────────────────────────────
        HostCmd::BotGithubLoginPoll {} => {
            let g = hub.bots.github_login.lock().await;
            let Some(handle) = g.as_ref() else {
                return Ok(json!({"state": "error", "error": "no login in progress"}));
            };
            let current = handle.state.lock().clone();
            match current {
                GithubLoginState::Pending => Ok(json!({"state": "pending"})),
                GithubLoginState::Error(e) => Ok(json!({"state": "error", "error": e})),
                GithubLoginState::Done => {
                    drop(g);
                    gh_auth_setup_git().await?;
                    let _ = tokio::process::Command::new("git")
                        .args([
                            "config",
                            "--global",
                            "url.https://github.com/.insteadOf",
                            "git@github.com:",
                        ])
                        .status()
                        .await;
                    let out = tokio::process::Command::new("gh")
                        .args(["auth", "status", "--hostname", "github.com"])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .output()
                        .await
                        .map_err(|e| format!("gh auth status: {e}"))?;
                    let text = String::from_utf8_lossy(&out.stderr).to_string()
                        + &String::from_utf8_lossy(&out.stdout);
                    let login = parse_gh_login(&text);
                    let mut g2 = hub.bots.github_login.lock().await;
                    *g2 = None;
                    hub.bots.notify_github_connected();
                    Ok(json!({"state": "done", "login": login}))
                }
            }
        }

        // ── BotGithubLogout ──────────────────────────────────────────────────
        HostCmd::BotGithubLogout {} => {
            {
                let mut g = hub.bots.github_login.lock().await;
                *g = None;
            }
            let _ = tokio::process::Command::new("gh")
                .args(["auth", "logout", "--hostname", "github.com"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
            let _ = tokio::process::Command::new("git")
                .args([
                    "config",
                    "--global",
                    "--unset",
                    "credential.https://github.com.helper",
                ])
                .status()
                .await;
            let _ = tokio::process::Command::new("git")
                .args([
                    "config",
                    "--global",
                    "--unset",
                    "url.https://github.com/.insteadOf",
                ])
                .status()
                .await;
            Ok(json!({}))
        }

        // ── BotActionAnswer ─────────────────────────────────────────────────
        HostCmd::BotActionAnswer {
            id,
            task: task_id,
            value,
        } => {
            use crate::bots::store::TaskAction;
            let tasks = hub.bots.store.read_tasks(&id, usize::MAX);
            let task = tasks
                .iter()
                .find(|t| t.id == task_id && t.state == TaskState::WaitingAction)
                .ok_or_else(|| format!("no waiting-action task {task_id} for bot {id}"))?
                .clone();
            match &task.action {
                Some(TaskAction::ProvideSecret { name, .. }) => {
                    let name = name.clone();
                    hub.bots
                        .store
                        .set_secret(&id, &name, &value)
                        .map_err(|e| format!("set_secret: {e}"))?;
                    hub.bots.answer_action(&task_id, name);
                }
                Some(TaskAction::ConnectGithub { .. }) => {
                    return Err("use bot_github_connect_resume for connect_github actions".into());
                }
                Some(TaskAction::AddTrigger {
                    trigger: trigger_kind,
                    prompt: trigger_prompt,
                    ..
                }) => {
                    if value == "add" {
                        // Load fresh bot to avoid overwriting concurrent changes.
                        let mut fresh = hub
                            .bots
                            .store
                            .load_bot(&id)
                            .map_err(|e| format!("load_bot: {e}"))?;
                        let new_trigger = crate::bots::store::Trigger {
                            id: paths::new_id(),
                            kind: trigger_kind.clone(),
                            enabled: true,
                            prompt: trigger_prompt.clone(),
                            last_fired: Some(now_s()), // baseline: next fire at real schedule time
                            tz: task.tz.clone(),       // stamp tz from task
                            next_fire: None,
                        };
                        crate::bots::triggers::validate_trigger(&new_trigger)?;
                        fresh.triggers.push(new_trigger);
                        hub.bots
                            .store
                            .save_bot(&fresh)
                            .map_err(|e| format!("save_bot: {e}"))?;
                        hub.broadcast(json!({"ev": "bot_updated", "bot_id": &id}).to_string());
                    }
                    hub.bots.answer_action(&task_id, value);
                }
                _ => {
                    hub.bots.answer_action(&task_id, value);
                }
            }
            Ok(json!({}))
        }

        // ── BotGithubConnectResume ───────────────────────────────────────────
        HostCmd::BotGithubConnectResume {} => {
            // Always fire the notification; the waiting runner calls is_github_logged_in()
            // after waking and returns Connected only if truly logged in. This also lets
            // tests drive the check via TEST_GITHUB_LOGGED_IN without spawning gh.
            hub.bots.notify_github_connected();
            Ok(json!({}))
        }

        // ── BotCrewGet ───────────────────────────────────────────────────────
        HostCmd::BotCrewGet {} => {
            let settings = hub.bots.store.load_crew_settings();
            Ok(json!({
                "short_replies": settings.short_replies,
                "short_replies_model": settings.short_replies_model,
            }))
        }

        // ── BotCrewSet ───────────────────────────────────────────────────────
        HostCmd::BotCrewSet {
            short_replies,
            short_replies_model,
        } => {
            let settings = crate::bots::store::CrewSettings {
                short_replies,
                short_replies_model,
            };
            hub.bots
                .store
                .save_crew_settings(&settings)
                .map_err(|e| format!("save_crew_settings: {e}"))?;
            Ok(json!({}))
        }

        _ => Err("not a bot command".into()),
    }
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── GitHub CLI helpers ───────────────────────────────────────────────────────

/// Returns the tarball asset name for this version and arch suffix (e.g. `"linux_amd64"`).
fn gh_asset_name(version: &str, arch_suffix: &str) -> String {
    format!("gh_{version}_{arch_suffix}.tar.gz")
}

/// Maps `std::env::consts::ARCH` values to the suffixes used in gh release asset names.
/// Returns `None` for unsupported architectures.
fn gh_arch_suffix(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("linux_amd64"),
        "aarch64" => Some("linux_arm64"),
        _ => None,
    }
}

/// Verify the SHA-256 of `data` against `checksums` (GitHub's `checksums.txt` format:
/// `<hex>  <filename>` lines, two spaces).  Rejects a mismatch and a missing entry.
fn verify_gh_checksum(data: &[u8], checksums: &str, filename: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let expected = checksums
        .lines()
        .find_map(|line| {
            let (hash, name) = line.split_once("  ")?;
            if name.trim() == filename {
                Some(hash.trim().to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!("checksum for {filename} not found in checksums file"))?;
    let actual = hex::encode(Sha256::digest(data));
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {filename}: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

/// Configure git to use `gh` as the HTTPS credential helper for github.com.
async fn gh_auth_setup_git() -> Result<(), String> {
    let out = tokio::process::Command::new("gh")
        .args(["auth", "setup-git"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("gh auth setup-git: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("gh auth setup-git failed: {stderr}"));
    }
    Ok(())
}

/// Ensure the `gh` CLI is available.  If not found on PATH, downloads the official release
/// binary for the current arch into `~/.local/bin` without requiring root.
/// Verifies the SHA-256 checksum before extracting.
async fn ensure_gh_installed(home: &std::path::Path) -> Result<(), String> {
    // Fast path: already on PATH.
    if tokio::process::Command::new("gh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let arch = std::env::consts::ARCH;
    let arch_suffix = gh_arch_suffix(arch)
        .ok_or_else(|| format!("unsupported architecture for gh auto-install: {arch}"))?;
    let local_bin = home.join(".local/bin");
    std::fs::create_dir_all(&local_bin).map_err(|e| format!("cannot create ~/.local/bin: {e}"))?;
    // Use ring crypto provider (may already be installed; ignore the error).
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent("atlas-host/1.0")
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    // Fetch latest release metadata from GitHub API.
    let release: serde_json::Value = client
        .get("https://api.github.com/repos/cli/cli/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("fetch gh release info: {e}"))?
        .json()
        .await
        .map_err(|e| format!("parse gh release metadata: {e}"))?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or("gh release has no tag_name")?;
    let version = tag.trim_start_matches('v');
    let asset_name = gh_asset_name(version, arch_suffix);
    let checksums_name = format!("gh_{version}_checksums.txt");
    let assets = release["assets"]
        .as_array()
        .ok_or("gh release has no assets array")?;
    let asset_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&asset_name))
        .and_then(|a| a["browser_download_url"].as_str().map(str::to_string))
        .ok_or_else(|| format!("gh release asset {asset_name} not found"))?;
    let checksums_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&checksums_name))
        .and_then(|a| a["browser_download_url"].as_str().map(str::to_string))
        .ok_or_else(|| format!("gh release checksums file {checksums_name} not found"))?;
    let checksums = client
        .get(&checksums_url)
        .send()
        .await
        .map_err(|e| format!("download gh checksums: {e}"))?
        .text()
        .await
        .map_err(|e| format!("read gh checksums: {e}"))?;
    let tarball_bytes = client
        .get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("download gh tarball: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("read gh tarball: {e}"))?;
    verify_gh_checksum(&tarball_bytes, &checksums, &asset_name)?;
    // Extract `bin/gh` from the tarball.
    let dir_name = asset_name.trim_end_matches(".tar.gz");
    let expected_inner = format!("{dir_name}/bin/gh");
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(&tarball_bytes[..]));
    let mut archive = tar::Archive::new(gz);
    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|e| format!("read gh tarball entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("gh tarball entry error: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("gh tarball path: {e}"))?
            .to_path_buf();
        if path.to_str() == Some(&expected_inner) {
            let dest = local_bin.join("gh");
            entry
                .unpack(&dest)
                .map_err(|e| format!("extract gh binary: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("chmod gh: {e}"))?;
            }
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!(
            "gh binary not found at {expected_inner} inside tarball {asset_name}"
        ));
    }
    Ok(())
}

/// Parse `Logged in to github.com account <login>` from `gh auth status` output.
fn parse_gh_login(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.split("account ").nth(1) {
            let login = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
            if !login.is_empty() {
                return Some(login.to_string());
            }
        }
    }
    None
}

/// Read lines from stdout + stderr of a `gh auth login` process until the device
/// user_code and verification_uri appear, or `timeout` elapses.
async fn wait_for_gh_code(
    stdout: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    stderr: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    timeout: std::time::Duration,
) -> Option<(String, String)> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::mpsc;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        let mut r = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx.send(line);
        }
    });
    tokio::spawn(async move {
        let mut r = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx2.send(line);
        }
    });
    let mut user_code: Option<String> = None;
    let verification_uri = "https://github.com/login/device".to_string();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(line)) => {
                if user_code.is_none()
                    && let Some(code) = extract_device_code(&line)
                {
                    user_code = Some(code);
                }
                if let Some(code) = user_code.take() {
                    return Some((code, verification_uri));
                }
            }
            Ok(None) | Err(_) => return None,
        }
    }
}

/// Extract the device user_code from a gh output line.
/// Matches tokens shaped XXXX-XXXX where X is A-Z or 0-9.
fn extract_device_code(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        let parts: Vec<&str> = t.split('-').collect();
        if parts.len() == 2 {
            let a = parts[0];
            let b = parts[1];
            if a.len() == 4
                && b.len() == 4
                && a.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && b.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                return Some(t.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod gh_tests {
    use super::*;

    #[test]
    fn gh_asset_name_for_known_arches() {
        assert_eq!(
            gh_asset_name("2.60.1", "linux_amd64"),
            "gh_2.60.1_linux_amd64.tar.gz"
        );
        assert_eq!(
            gh_asset_name("2.60.1", "linux_arm64"),
            "gh_2.60.1_linux_arm64.tar.gz"
        );
    }

    #[test]
    fn gh_arch_suffix_maps_rust_consts_to_asset_suffixes() {
        assert_eq!(gh_arch_suffix("x86_64"), Some("linux_amd64"));
        assert_eq!(gh_arch_suffix("aarch64"), Some("linux_arm64"));
        assert_eq!(gh_arch_suffix("arm"), None);
    }

    #[test]
    fn verify_gh_checksum_accepts_correct_hash() {
        use sha2::{Digest, Sha256};
        let data = b"hello gh";
        let hash = hex::encode(Sha256::digest(data));
        let checksums = format!("{hash}  gh_2.60.1_linux_amd64.tar.gz\n");
        verify_gh_checksum(data, &checksums, "gh_2.60.1_linux_amd64.tar.gz").unwrap();
    }

    #[test]
    fn verify_gh_checksum_rejects_wrong_hash() {
        let data = b"hello gh";
        let wrong = "0".repeat(64);
        let checksums = format!("{wrong}  gh_2.60.1_linux_amd64.tar.gz\n");
        let err = verify_gh_checksum(data, &checksums, "gh_2.60.1_linux_amd64.tar.gz").unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
    }

    #[test]
    fn verify_gh_checksum_rejects_missing_entry() {
        let data = b"hello gh";
        let checksums = "abc123  some_other_file.tar.gz\n";
        let err = verify_gh_checksum(data, checksums, "gh_2.60.1_linux_amd64.tar.gz").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn parse_gh_login_extracts_username() {
        let text = "github.com\n  x Logged in to github.com account alice (keyring)\n";
        assert_eq!(parse_gh_login(text), Some("alice".to_string()));
        assert_eq!(parse_gh_login("not logged in"), None);
    }

    #[test]
    fn extract_device_code_finds_code_in_gh_output() {
        assert_eq!(
            extract_device_code("! First copy your one-time code: ABCD-1234"),
            Some("ABCD-1234".to_string())
        );
        assert_eq!(extract_device_code("nothing here"), None);
        assert_eq!(
            extract_device_code("  code: WXYZ-9876  extra"),
            Some("WXYZ-9876".to_string())
        );
    }
}
