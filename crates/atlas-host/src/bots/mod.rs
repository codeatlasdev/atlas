//! Bot subsystem: hub, public API, and HostCmd routing.

pub(crate) mod push;
pub(crate) mod runner;
pub(crate) mod schedule;
pub(crate) mod store;
pub(crate) mod telegram;
#[cfg(test)]
mod tests;

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
        })
    }

    pub(crate) fn start(self: &Arc<Self>, hub: std::sync::Weak<Hub>) {
        let _ = self.hub_ref.set(hub.clone());
        self.telegram.set_hub(hub.clone());
        self.telegram.clone().start();
        schedule::start(hub.clone());

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
                        TaskState::Running => {
                            let mut t2 = t;
                            t2.state = TaskState::Failed;
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
                    if let Some(l) = last {
                        v["last"] = l;
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
            if agent == "omp" && approval_enum == Approval::Ask {
                return Err("omp bots run with approval: auto".into());
            }
            if atlas_core::agents::get(&agent).is_none() {
                return Err(format!("unknown agent '{agent}'"));
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
                created_at: now_s(),
            };
            hub.bots
                .store
                .save_bot(&bot)
                .map_err(|e| format!("save_bot: {e}"))?;
            let _ = hub.bots.store.write_memory(&id, "");
            Ok(serde_json::to_value(&bot).map_err(|e| e.to_string())?)
        }

        // ── BotUpdate ───────────────────────────────────────────────────────
        HostCmd::BotUpdate {
            id,
            name,
            role,
            agent,
            approval,
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
                if bot.agent == "omp" && ap_enum == Approval::Ask {
                    return Err("omp bots run with approval: auto".into());
                }
                bot.approval = ap_enum;
            }
            hub.bots
                .store
                .save_bot(&bot)
                .map_err(|e| format!("save_bot: {e}"))?;
            Ok(serde_json::to_value(&bot).map_err(|e| e.to_string())?)
        }

        // ── BotDelete ───────────────────────────────────────────────────────
        HostCmd::BotDelete { id } => {
            // Cancel all queued/running tasks first.
            let tasks = hub.bots.store.read_tasks(&id, usize::MAX);
            for t in tasks {
                if matches!(t.state, TaskState::Queued | TaskState::Running) {
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
        HostCmd::BotTask { id, prompt, source } => {
            let bot = hub
                .bots
                .store
                .load_bot(&id)
                .map_err(|_| format!("bot {id} not found"))?;
            if prompt.trim().is_empty() {
                return Err("prompt must not be empty".into());
            }
            let task = Task::new(source, prompt.trim());
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

        _ => Err("not a bot command".into()),
    }
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
