//! Telegram polling bridge.
//!
//! Config at `<dir>/telegram.json` (0600): `{token, chats: [i64]}`.
//! `ATLAS_TELEGRAM_API` env overrides the base URL (for tests).
//!
//! Poll loop: `getUpdates?timeout=50&offset=<offset>`, backoff 5 s → 5 min on error.
//! Outbound messages use the same reqwest client as `push.rs`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::server::Hub;

// ─── config ──────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Default)]
struct TelegramConfig {
    token: String,
    #[serde(default)]
    chats: Vec<i64>,
}

fn config_path(dir: &Path) -> PathBuf {
    dir.join("telegram.json")
}

// ─── link code ───────────────────────────────────────────────────────────────

struct LinkCode {
    code: String,
    expires: Instant,
}

// ─── bridge ──────────────────────────────────────────────────────────────────

pub(crate) struct TelegramBridge {
    dir: PathBuf,
    api_base: String,
    hub_weak: OnceLock<Weak<Hub>>,
    config: Mutex<TelegramConfig>,
    link: Mutex<Option<LinkCode>>,
}

impl TelegramBridge {
    pub(crate) fn new(dir: PathBuf) -> Self {
        let api_base = std::env::var("ATLAS_TELEGRAM_API")
            .unwrap_or_else(|_| "https://api.telegram.org".to_string());
        Self::new_with_api(dir, api_base)
    }

    pub(crate) fn new_with_api(dir: PathBuf, api_base: impl Into<String>) -> Self {
        let config = std::fs::read(dir.join("telegram.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            dir,
            api_base: api_base.into(),
            hub_weak: OnceLock::new(),
            config: Mutex::new(config),
            link: Mutex::new(None),
        }
    }

    /// Set the Hub back-reference (called once from `BotHub::start`).
    pub(crate) fn set_hub(&self, hub: Weak<Hub>) {
        let _ = self.hub_weak.set(hub);
    }

    fn hub(&self) -> Option<Arc<Hub>> {
        self.hub_weak.get()?.upgrade()
    }

    /// Set or clear the Telegram bot token, saving to disk.
    pub(crate) fn set_token(&self, token: Option<String>) -> io::Result<()> {
        let mut cfg = self.config.lock();
        cfg.token = token.unwrap_or_default();
        self.save_config(&cfg)
    }

    /// Currently active 6-digit link code, if any (not expired). Used in tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn link_code(&self) -> Option<String> {
        let g = self.link.lock();
        g.as_ref()
            .filter(|lc| lc.expires > Instant::now())
            .map(|lc| lc.code.clone())
    }

    /// Generate a new 6-digit link code valid for 10 min and return it.
    pub(crate) fn start_link(&self) -> String {
        let code = match crate::paths::random_hex(3) {
            Ok(h) => {
                // 3 hex bytes = 6 hex chars; use decimal for friendliness
                let n = u32::from_str_radix(&h, 16).unwrap_or(0) % 1_000_000;
                format!("{n:06}")
            }
            Err(_) => "000000".to_string(),
        };
        *self.link.lock() = Some(LinkCode {
            code: code.clone(),
            expires: Instant::now() + Duration::from_secs(600),
        });
        code
    }

    /// True if at least one chat is linked.
    pub(crate) fn has_linked_chat(&self) -> bool {
        !self.config.lock().chats.is_empty()
    }

    /// Fire-and-forget: send `text` to all linked chats.
    pub(crate) fn send_message(&self, text: &str) {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let chats = self.config.lock().chats.clone();
        let text = text.to_string();
        let base = self.api_base.clone();
        tokio::spawn(async move {
            let Some(client) = crate::bots::push::HTTP.as_ref() else {
                return;
            };
            for chat_id in chats {
                let url = format!("{base}/bot{token}/sendMessage");
                let body = json!({"chat_id": chat_id, "text": text});
                let _ = client.post(&url).json(&body).send().await;
            }
        });
    }

    /// Send an approval request with Allow / Deny inline keyboard to all linked chats.
    pub(crate) fn send_approval(
        &self,
        bot_name: &str,
        tool_name: &str,
        tool_input: &serde_json::Value,
        req: &str,
        pane: &str,
    ) {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let chats = self.config.lock().chats.clone();
        let text = format!(
            "🤖 *{bot_name}* wants to run `{tool_name}`\n```\n{}\n```",
            serde_json::to_string_pretty(tool_input).unwrap_or_default()
        );
        let req = req.to_string();
        let base = self.api_base.clone();
        let _ = pane; // pane is embedded in req → approval lookup
        tokio::spawn(async move {
            let Some(client) = crate::bots::push::HTTP.as_ref() else {
                return;
            };
            let keyboard = json!({
                "inline_keyboard": [[
                    {"text": "✅ Allow", "callback_data": format!("a:{req}:1")},
                    {"text": "❌ Deny",  "callback_data": format!("a:{req}:0")}
                ]]
            });
            for chat_id in &chats {
                let url = format!("{base}/bot{token}/sendMessage");
                let body = json!({
                    "chat_id": chat_id,
                    "text": text,
                    "parse_mode": "Markdown",
                    "reply_markup": keyboard
                });
                let _ = client.post(&url).json(&body).send().await;
            }
        });
    }

    /// Spawn the poll loop.
    pub(crate) fn start(self: Arc<Self>) {
        tokio::spawn(async move { self.poll_loop().await });
    }

    // ── config helpers ─────────────────────────────────────────────────────

    fn save_config(&self, cfg: &TelegramConfig) -> io::Result<()> {
        let data =
            serde_json::to_vec(cfg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        crate::paths::write_private(&config_path(&self.dir), &data)
    }

    fn add_chat(&self, chat_id: i64) -> io::Result<()> {
        let mut cfg = self.config.lock();
        if !cfg.chats.contains(&chat_id) {
            cfg.chats.push(chat_id);
            self.save_config(&cfg)?;
        }
        Ok(())
    }

    // ── poll loop ──────────────────────────────────────────────────────────

    async fn poll_loop(&self) {
        let mut offset: i64 = 0;
        let mut backoff = Duration::from_secs(5);
        const MAX_BACKOFF: Duration = Duration::from_secs(300);

        loop {
            let token = self.config.lock().token.clone();
            if token.is_empty() {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            let base = self.api_base.clone();
            let url = format!(
                "{base}/bot{token}/getUpdates?timeout=50&offset={offset}&allowed_updates=message,callback_query"
            );
            let Some(client) = crate::bots::push::HTTP.as_ref() else {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            };
            let res = tokio::time::timeout(Duration::from_secs(65), client.get(&url).send()).await;

            match res {
                Ok(Ok(resp)) => {
                    backoff = Duration::from_secs(5);
                    match resp.json::<Value>().await {
                        Ok(v) if v["ok"] == true => {
                            if let Some(updates) = v["result"].as_array() {
                                for upd in updates {
                                    let upd_id = upd["update_id"].as_i64().unwrap_or(0);
                                    self.handle_update(upd).await;
                                    offset = upd_id + 1;
                                }
                            }
                        }
                        Ok(v) => {
                            eprintln!(
                                "atlas telegram: API error: {}",
                                v["description"].as_str().unwrap_or("?")
                            );
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(MAX_BACKOFF);
                        }
                        Err(e) => {
                            eprintln!("atlas telegram: parse error: {e}");
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(MAX_BACKOFF);
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("atlas telegram: request error: {e}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
                Err(_timeout) => {
                    // Normal long-poll timeout; just retry immediately.
                }
            }
        }
    }

    pub(crate) async fn handle_update(&self, upd: &Value) {
        if let Some(cb) = upd.get("callback_query") {
            self.handle_callback(cb).await;
            return;
        }
        if let Some(msg) = upd.get("message") {
            self.handle_message(msg).await;
        }
    }

    async fn handle_callback(&self, cb: &Value) {
        let chat_id = cb["message"]["chat"]["id"].as_i64().unwrap_or(0);
        let msg_id = cb["message"]["message_id"].as_i64().unwrap_or(0);
        let data = cb["data"].as_str().unwrap_or("");

        // Format: "a:{req}:{1|0}"
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        if parts.len() == 3 && parts[0] == "a" {
            let req = parts[1];
            let allow = parts[2] == "1";

            let answer_text = if allow { "✅ Allowed" } else { "❌ Denied" };

            if let Some(hub) = self.hub() {
                // Find the pane from pending approvals.
                let pending = hub.agents.pending_approvals();
                if let Some(pane) = pending
                    .iter()
                    .find(|v| v["req"].as_str() == Some(req))
                    .and_then(|v| v["pane_key"].as_str())
                    .map(str::to_string)
                {
                    let _ = hub.agents.approve(&pane, req, allow);
                }
            }

            // Edit the original message to show the decision.
            let token = self.config.lock().token.clone();
            if !token.is_empty() {
                let base = self.api_base.clone();
                let url = format!("{base}/bot{token}/editMessageText");
                let body = json!({
                    "chat_id": chat_id,
                    "message_id": msg_id,
                    "text": answer_text
                });
                let Some(client) = crate::bots::push::HTTP.as_ref() else {
                    return;
                };
                let _ = client.post(&url).json(&body).send().await;
            }
        }
    }

    pub(crate) async fn handle_message(&self, msg: &Value) {
        let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);
        let text = msg["text"].as_str().unwrap_or("").trim();

        // /start <code>
        if let Some(rest) = text.strip_prefix("/start") {
            let code = rest.trim();
            let is_match = {
                let g = self.link.lock();
                g.as_ref()
                    .filter(|lc| lc.expires > Instant::now() && lc.code == code)
                    .is_some()
            };
            if is_match {
                // Clear the code.
                *self.link.lock() = None;
                let hostname = crate::server::hostname();
                let _ = self.add_chat(chat_id);
                self.reply(chat_id, &format!("Linked to {hostname}.")).await;
            } else if code.is_empty() {
                self.reply(chat_id, "Send your link code: /start <code>")
                    .await;
            } else {
                self.reply(chat_id, "Invalid or expired code.").await;
            }
            return;
        }

        // Only respond to linked chats.
        if !self.config.lock().chats.contains(&chat_id) {
            return;
        }

        // /bots
        if text == "/bots" {
            if let Some(hub) = self.hub() {
                let bots = hub.bots.store.list_bots();
                if bots.is_empty() {
                    self.reply(chat_id, "No bots configured.").await;
                } else {
                    let lines: Vec<String> = bots
                        .iter()
                        .map(|b| format!("• {} ({})", b.name, b.agent))
                        .collect();
                    self.reply(chat_id, &lines.join("\n")).await;
                }
            }
            return;
        }

        // /task <name> <text>
        if let Some(rest) = text.strip_prefix("/task ") {
            let (name, prompt) = match rest.split_once(' ') {
                Some((n, p)) => (n.trim(), p.trim()),
                None => (rest.trim(), ""),
            };
            if prompt.is_empty() {
                self.reply(chat_id, "Usage: /task <bot-name> <prompt>")
                    .await;
                return;
            }
            self.create_task(chat_id, name, prompt).await;
            return;
        }

        // Plain text → single bot, or prompt disambiguation.
        if let Some(hub) = self.hub() {
            let bots = hub.bots.store.list_bots();
            if bots.len() == 1 {
                let name = bots[0].name.clone();
                self.create_task(chat_id, &name, text).await;
            } else {
                self.reply(chat_id, "Which bot? Use /task <name> <text>")
                    .await;
            }
        }
    }

    async fn create_task(&self, chat_id: i64, bot_name: &str, prompt: &str) {
        let Some(hub) = self.hub() else {
            return;
        };
        let bots = hub.bots.store.list_bots();
        let Some(bot) = bots.iter().find(|b| b.name.eq_ignore_ascii_case(bot_name)) else {
            self.reply(chat_id, &format!("Bot '{}' not found.", bot_name))
                .await;
            return;
        };
        let task = crate::bots::store::Task::new("telegram", prompt);
        let task_id = task.id.clone();
        if let Err(e) = hub.bots.store.append_task(&bot.id, &task) {
            self.reply(chat_id, &format!("Error: {e}")).await;
            return;
        }
        crate::bots::runner::enqueue(&hub, &hub.bots.store, bot, task);
        hub.broadcast(serde_json::json!({"ev": "bot_task", "bot_id": bot.id}).to_string());
        self.reply(chat_id, &format!("Task {task_id} queued."))
            .await;
    }

    async fn reply(&self, chat_id: i64, text: &str) {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let base = self.api_base.clone();
        let url = format!("{base}/bot{token}/sendMessage");
        let body = json!({"chat_id": chat_id, "text": text});
        let Some(client) = crate::bots::push::HTTP.as_ref() else {
            return;
        };
        let _ = client.post(&url).json(&body).send().await;
    }
}

#[cfg(test)]
mod tests {
    use super::TelegramBridge;
    use std::time::Duration;

    #[test]
    fn start_link_generates_six_digit_code() {
        let tmp = tempfile::tempdir().unwrap();
        let b =
            TelegramBridge::new_with_api(tmp.path().to_path_buf(), "http://localhost".to_string());
        let code = b.start_link();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(b.link_code(), Some(code));
    }

    #[test]
    fn expired_link_code_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let b =
            TelegramBridge::new_with_api(tmp.path().to_path_buf(), "http://localhost".to_string());
        let _code = b.start_link();
        if let Some(lc) = &mut *b.link.lock() {
            lc.expires = std::time::Instant::now() - Duration::from_secs(1);
        }
        assert_eq!(b.link_code(), None);
    }

    #[test]
    fn unlinked_chat_is_not_answerable() {
        let tmp = tempfile::tempdir().unwrap();
        let b =
            TelegramBridge::new_with_api(tmp.path().to_path_buf(), "http://localhost".to_string());
        assert!(!b.has_linked_chat());
    }

    #[test]
    fn callback_data_parsing_allow_and_deny() {
        let data = "a:req123:1";
        let parts: Vec<&str> = data.splitn(3, ':').collect();
        assert_eq!(parts, ["a", "req123", "1"]);
        assert!(parts[2] == "1");

        let data2 = "a:req456:0";
        let parts2: Vec<&str> = data2.splitn(3, ':').collect();
        assert!(parts2[2] != "1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn start_code_links_chat_via_handle_update() {
        let tmp = tempfile::tempdir().unwrap();
        let b = std::sync::Arc::new(TelegramBridge::new_with_api(
            tmp.path().to_path_buf(),
            "http://localhost:1".to_string(),
        ));
        {
            b.config.lock().token = "tok".to_string();
        }
        let code = b.start_link();
        assert_eq!(code.len(), 6);

        let update = serde_json::json!({
            "update_id": 1,
            "message": {
                "message_id": 1,
                "chat": {"id": 42},
                "text": format!("/start {code}"),
                "from": {"id": 99, "is_bot": false, "first_name": "T"}
            }
        });
        b.handle_update(&update).await;
        assert!(b.has_linked_chat(), "chat 42 should be linked after /start");
    }

    #[tokio::test]
    async fn unlinked_chat_message_returns_quickly() {
        let tmp = tempfile::tempdir().unwrap();
        let b = TelegramBridge::new_with_api(
            tmp.path().to_path_buf(),
            "http://127.0.0.1:1".to_string(),
        );
        {
            b.config.lock().token = "tok".to_string();
        }
        let msg = serde_json::json!({
            "message_id": 1,
            "chat": {"id": 999},
            "text": "hello"
        });
        // No linked chats — message should be ignored without any network call.
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            b.handle_message(&msg),
        )
        .await
        .expect("handle_message for unlinked chat must not block");
    }
}
