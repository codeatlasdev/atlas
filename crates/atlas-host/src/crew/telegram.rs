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

use crate::paths::Loaded;
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

/// Telegram Bot API endpoint for `method`. `/bot<token>/` is Telegram's own path ("bot" is their
/// product name, not our vocabulary: never rename it; `messages_go_to_telegrams_bot_api_path`).
fn api_url(base: &str, token: &str, method: &str) -> String {
    format!("{base}/bot{token}/{method}")
}

/// Where Telegram serves the files `getFile` names: the same `/bot<token>/` segment, behind `/file`.
fn file_url(base: &str, token: &str, file_path: &str) -> String {
    format!("{base}/file/bot{token}/{file_path}")
}

/// The most photos Telegram takes in one `sendMediaGroup`.
pub(crate) const MEDIA_GROUP_MAX: usize = 10;

/// A leading `/ui` marks the message as a UI task: `(true, rest)` (`rest` is empty for a bare `/ui`); anything
/// else is `(false, text)`. The rest is read as always (`/task <name> <text>`, a plain prompt), so
/// `/ui /task Ana fix the header` works. `/uikit` is not `/ui`.
fn strip_ui(text: &str) -> (bool, &str) {
    if text == "/ui" {
        return (true, "");
    }
    match text.strip_prefix("/ui ") {
        Some(rest) => (true, rest.trim()),
        None => (false, text),
    }
}

/// What a `/ui` with nothing to do says (one line).
const UI_USAGE: &str = "Usage: /ui <message>, or /ui /task <crewmate-name> <message>";

/// The agent's images of a task, `(name, bytes)`, up to [`MEDIA_GROUP_MAX`] of them: the newest, the ones
/// the agent took last being the "after". An image whose file cannot be read is left out and logged.
fn card_images_of(
    store: &crate::crew::store::CrewStore,
    crewmate_id: &str,
    task_id: &str,
) -> Vec<(String, Vec<u8>)> {
    let Some(task) = store.task(crewmate_id, task_id) else {
        return vec![];
    };
    let agent: Vec<_> = task
        .attachments
        .iter()
        .filter(|a| a.source == crate::crew::attachments::SOURCE_AGENT)
        .collect();
    let skip = agent.len().saturating_sub(MEDIA_GROUP_MAX);
    agent[skip..]
        .iter()
        .filter_map(
            |a| match crate::crew::attachments::read(store, crewmate_id, &task, &a.name) {
                Ok((a, bytes)) => Some((a.name, bytes)),
                Err(e) => {
                    // i18n-ignore: log
                    eprintln!(
                        "atlas telegram: image {} of task {task_id} not sent: {e}",
                        a.name
                    );
                    None
                }
            },
        )
        .collect()
}

/// The `multipart/form-data` body of a `sendMediaGroup` for `chat_id`: a `media` JSON array whose items
/// point (`attach://f<i>`) at the file parts, then one part per image. Built by hand, so no `multipart`
/// feature of reqwest is needed for it. Returns the `content-type` (with the boundary) and the body.
pub(crate) fn media_group_body(chat_id: i64, images: &[(String, Vec<u8>)]) -> (String, Vec<u8>) {
    let boundary = format!("atlas-{}", crate::paths::new_id());
    let media: Vec<Value> = images
        .iter()
        .enumerate()
        .map(|(i, _)| json!({"type": "photo", "media": format!("attach://f{i}")}))
        .collect();
    let mut body = Vec::new();
    let text_part = |body: &mut Vec<u8>, name: &str, value: &str| {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    };
    text_part(&mut body, "chat_id", &chat_id.to_string());
    text_part(&mut body, "media", &Value::Array(media).to_string());
    for (i, (name, bytes)) in images.iter().enumerate() {
        // The name is ours (`attachments::safe_name`: `[A-Za-z0-9._-]`), so it needs no escaping here.
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"f{i}\"; filename=\"{name}\"\r\n\
                 Content-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

// ─── link code ───────────────────────────────────────────────────────────────

struct LinkCode {
    code: String,
    expires: Instant,
    /// Wrong `/start` codes since this code was issued, from any chat.
    wrong: u8,
}

/// Wrong codes a link code survives. The fifth kills it: six digits are guessable by brute force
/// within the ten minutes it lives, and a linked chat can queue tasks and answer every approval.
const MAX_WRONG_CODES: u8 = 5;

enum LinkAttempt {
    Linked,
    Wrong,
}

// ─── bridge ──────────────────────────────────────────────────────────────────

pub(crate) struct TelegramBridge {
    dir: PathBuf,
    /// Where an unreadable `telegram.json` was kept at start, if there was one.
    corrupt_aside: Option<PathBuf>,
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
        // A telegram.json that cannot be parsed is set aside (never overwritten by the next `set_token`) and
        // named in `status`, so the client can say why Telegram went quiet.
        let (config, corrupt_aside) = match crate::paths::read_json_or_aside(&config_path(&dir)) {
            Ok(Loaded::Ok(c)) => (c, None),
            Ok(Loaded::Missing) => (TelegramConfig::default(), None),
            Ok(Loaded::Corrupt(aside)) => (TelegramConfig::default(), Some(aside)),
            Err(e) => {
                // i18n-ignore: log
                eprintln!("atlas crew: cannot read telegram.json: {e}");
                (TelegramConfig::default(), None)
            }
        };
        Self {
            corrupt_aside,
            dir,
            api_base: api_base.into(),
            hub_weak: OnceLock::new(),
            config: Mutex::new(config),
            link: Mutex::new(None),
        }
    }

    /// Set the Hub back-reference (called once from `CrewHub::start`).
    pub(crate) fn set_hub(&self, hub: Weak<Hub>) {
        let _ = self.hub_weak.set(hub);
    }

    fn hub(&self) -> Option<Arc<Hub>> {
        self.hub_weak.get()?.upgrade()
    }

    /// Set or clear the Telegram bot token (from @BotFather), saving to disk.
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
            wrong: 0,
        });
        code
    }

    /// `{configured, chats}` for `crew_telegram_status`: whether a token is set and how many chats
    /// are linked. The token is never part of it.
    pub(crate) fn status(&self) -> Value {
        let cfg = self.config.lock();
        let mut status = json!({"configured": !cfg.token.is_empty(), "chats": cfg.chats.len()});
        if let Some(aside) = &self.corrupt_aside {
            status["corrupt_aside"] = json!(aside.display().to_string());
        }
        status
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
            let Some(client) = crate::crew::push::HTTP.as_ref() else {
                return;
            };
            for chat_id in chats {
                let url = api_url(&base, &token, "sendMessage");
                let body = json!({"chat_id": chat_id, "text": text});
                let _ = client.post(&url).json(&body).send().await;
            }
        });
    }

    /// Send an approval request with Allow / Deny inline keyboard to all linked chats.
    pub(crate) fn send_approval(
        &self,
        crewmate_name: &str,
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
            "*{crewmate_name}* wants to run `{tool_name}`\n```\n{}\n```",
            serde_json::to_string_pretty(tool_input).unwrap_or_default()
        );
        let req = req.to_string();
        let base = self.api_base.clone();
        let _ = pane; // pane is embedded in req → approval lookup
        tokio::spawn(async move {
            let Some(client) = crate::crew::push::HTTP.as_ref() else {
                return;
            };
            let keyboard = json!({
                "inline_keyboard": [[
                    {"text": "Allow", "callback_data": format!("a:{req}:1")},
                    {"text": "Deny",  "callback_data": format!("a:{req}:0")}
                ]]
            });
            for chat_id in &chats {
                let url = api_url(&base, &token, "sendMessage");
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

    /// Tell every linked chat that a task waits on a card (`Refused` by the guards, `ProveUi` without
    /// captures): the agent's images of the task first, as one media group (at most
    /// [`MEDIA_GROUP_MAX`]), then the message with the reasons and the Override / Retry buttons
    /// (`r:<task>:<o|r>`, answered in `handle_callback` like the card on the apps). Sent in this order
    /// from one task, so the buttons never arrive before the pictures they are about.
    pub(crate) fn send_card(
        &self,
        crewmate_name: &str,
        crewmate_id: &str,
        task_id: &str,
        reasons: &[String],
    ) {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let chats = self.config.lock().chats.clone();
        let images = self
            .hub()
            .map(|hub| card_images_of(&hub.crew.store, crewmate_id, task_id));
        let text = format!(
            "{crewmate_name}: this work needs you\n\n{}",
            reasons
                .iter()
                .map(|r| format!("- {r}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
        .chars()
        .take(4000)
        .collect::<String>();
        let keyboard = json!({"inline_keyboard": [[
            {"text": "Override", "callback_data": format!("r:{task_id}:o")},
            {"text": "Retry with reasons", "callback_data": format!("r:{task_id}:r")}
        ]]});
        let base = self.api_base.clone();
        tokio::spawn(async move {
            let Some(client) = crate::crew::push::HTTP.as_ref() else {
                return;
            };
            for chat_id in chats {
                if let Some(images) = images.as_ref().filter(|i| !i.is_empty()) {
                    let (content_type, body) = media_group_body(chat_id, images);
                    let _ = client
                        .post(api_url(&base, &token, "sendMediaGroup"))
                        .header("content-type", content_type)
                        .body(body)
                        .send()
                        .await;
                }
                let url = api_url(&base, &token, "sendMessage");
                let body = json!({"chat_id": chat_id, "text": text, "reply_markup": keyboard});
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
                "{}?timeout=50&offset={offset}&allowed_updates=message,callback_query",
                api_url(&base, &token, "getUpdates")
            );
            let Some(client) = crate::crew::push::HTTP.as_ref() else {
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
                            // i18n-ignore: log
                            eprintln!(
                                "atlas telegram: API error: {}",
                                v["description"].as_str().unwrap_or("?")
                            );
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(MAX_BACKOFF);
                        }
                        Err(e) => {
                            // i18n-ignore: log
                            eprintln!("atlas telegram: parse error: {e}");
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(MAX_BACKOFF);
                        }
                    }
                }
                Ok(Err(e)) => {
                    // i18n-ignore: log
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
        // Format: "r:{task}:{o|r}", the answer to a refusal (`send_refusal`).
        if parts.len() == 3 && parts[0] == "r" {
            self.answer_refusal(chat_id, msg_id, parts[1], parts[2] == "o")
                .await;
            return;
        }
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
                    let _ =
                        hub.agents
                            .approve_as(&pane, req, allow, &format!("telegram:{chat_id}"));
                }
            }

            // Edit the original message to show the decision.
            let token = self.config.lock().token.clone();
            if !token.is_empty() {
                let base = self.api_base.clone();
                let url = api_url(&base, &token, "editMessageText");
                let body = json!({
                    "chat_id": chat_id,
                    "message_id": msg_id,
                    "text": answer_text
                });
                let Some(client) = crate::crew::push::HTTP.as_ref() else {
                    return;
                };
                let _ = client.post(&url).json(&body).send().await;
            }
        }
    }

    /// A tap on a refusal's button: answer the parked task `override` or `retry`, and edit the
    /// message to say what was chosen (or that the task no longer waits).
    async fn answer_refusal(&self, chat_id: i64, msg_id: i64, task_id: &str, override_it: bool) {
        let answer = if override_it {
            crate::crew::runner::REFUSAL_OVERRIDE
        } else {
            crate::crew::runner::REFUSAL_RETRY
        };
        let delivered = self
            .hub()
            .is_some_and(|hub| hub.crew.answer_action(task_id, answer.to_string()));
        let text = match (delivered, override_it) {
            (false, _) => "This task is no longer waiting.",
            (true, true) => "Overridden: the work is accepted.",
            (true, false) => "Retrying with the reasons.",
        };
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let url = api_url(&self.api_base, &token, "editMessageText");
        let body = json!({"chat_id": chat_id, "message_id": msg_id, "text": text});
        if let Some(client) = crate::crew::push::HTTP.as_ref() {
            let _ = client.post(&url).json(&body).send().await;
        }
    }

    pub(crate) async fn handle_message(&self, msg: &Value) {
        let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);
        let text = msg["text"].as_str().unwrap_or("").trim();

        // /start <code>
        if let Some(rest) = text.strip_prefix("/start") {
            let code = rest.trim();
            let outcome = {
                let mut g = self.link.lock();
                match g.as_mut() {
                    Some(lc) if lc.expires > Instant::now() && lc.code == code => {
                        *g = None;
                        LinkAttempt::Linked
                    }
                    Some(lc) if !code.is_empty() => {
                        lc.wrong += 1;
                        if lc.wrong >= MAX_WRONG_CODES {
                            // i18n-ignore: log
                            eprintln!(
                                "atlas telegram: {MAX_WRONG_CODES} wrong link codes; the code is invalidated, start a new link"
                            );
                            *g = None;
                        }
                        LinkAttempt::Wrong
                    }
                    _ => LinkAttempt::Wrong,
                }
            };
            if let LinkAttempt::Linked = outcome {
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

        // A photo: its caption is the prompt.
        if msg["photo"].is_array() {
            self.handle_photo(chat_id, msg).await;
            return;
        }

        // `/ui <message>`: the same message, for a task that changes what people see (`Task.ui`).
        let (ui, text) = strip_ui(text);
        // A UI task needs its message: a task with an empty prompt is never useful, so say how and queue nothing.
        if ui && text.is_empty() {
            self.reply(chat_id, UI_USAGE).await;
            return;
        }

        // /mode: every crewmate's permission mode; `/mode [<crewmate>] <ask|edits|bypass>` changes one. Only a linked
        // chat gets here (the one above returns for any other), and that chat is the owner's.
        if text == "/mode" || text.starts_with("/mode ") {
            self.handle_mode(chat_id, text["/mode".len()..].trim())
                .await;
            return;
        }

        // /crewmates
        if text == "/crew" {
            if let Some(hub) = self.hub() {
                let crewmates = hub.crew.store.list_crewmates();
                if crewmates.is_empty() {
                    self.reply(chat_id, "No crewmates configured.").await;
                } else {
                    let lines: Vec<String> = crewmates
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
                self.reply(chat_id, "Usage: /task <crewmate-name> <prompt>")
                    .await;
                return;
            }
            self.create_task(chat_id, name, prompt, &[], ui).await;
            return;
        }

        // Plain text → single crewmate, or prompt disambiguation.
        if let Some(hub) = self.hub() {
            let crewmates = hub.crew.store.list_crewmates();
            if crewmates.len() == 1 {
                let name = crewmates[0].name.clone();
                self.create_task(chat_id, &name, text, &[], ui).await;
            } else {
                self.reply(chat_id, "Which crewmate? Use /task <name> <text>")
                    .await;
            }
        }
    }

    /// `/mode` (no arguments): each crewmate and its permission mode. `/mode [<crewmate>] <ask|edits|bypass>`:
    /// change one (the only crewmate when none is named), through the same command the apps use, so it is audited
    /// with this chat as the device, broadcast, and a call that waits is decided again at once.
    async fn handle_mode(&self, chat_id: i64, args: &str) {
        use crate::crew::store::Mode;
        use atlas_i18n::t;
        let Some(hub) = self.hub() else {
            return;
        };
        let label = |m: Mode| match m {
            Mode::Ask => t!("cmd_hostd_mode_ask"),
            Mode::Edits => t!("cmd_hostd_mode_edits"),
            Mode::Bypass => t!("cmd_hostd_mode_bypass"),
        };
        let crewmates = hub.crew.store.list_crewmates();
        if args.is_empty() {
            let text = if crewmates.is_empty() {
                t!("cmd_hostd_telegram_mode_none")
            } else {
                crewmates
                    .iter()
                    .map(|c| {
                        t!(
                            "cmd_hostd_telegram_mode_line",
                            name = c.name.as_str(),
                            mode = label(c.mode)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            self.reply(chat_id, &text).await;
            return;
        }
        // The last word is the mode; what comes before it names the crewmate.
        let (name, word) = match args.rsplit_once(char::is_whitespace) {
            Some((n, w)) => (n.trim(), w),
            None => ("", args),
        };
        let Ok(mode) = Mode::parse(word) else {
            self.reply(chat_id, &t!("cmd_hostd_telegram_mode_usage"))
                .await;
            return;
        };
        let target = if name.is_empty() {
            (crewmates.len() == 1).then(|| &crewmates[0])
        } else {
            crewmates.iter().find(|c| c.name.eq_ignore_ascii_case(name))
        };
        let Some(c) = target else {
            let text = if name.is_empty() {
                t!("cmd_hostd_telegram_mode_usage")
            } else {
                t!("cmd_hostd_telegram_mode_not_found", name = name)
            };
            self.reply(chat_id, &text).await;
            return;
        };
        let device = format!("telegram:{chat_id}");
        let text = match crate::crew::approval::set_approval(&hub, &c.id, mode.as_str(), &device) {
            Ok(_) => t!(
                "cmd_hostd_telegram_mode_set",
                name = c.name.as_str(),
                mode = label(mode)
            ),
            Err(why) => t!("cmd_hostd_telegram_mode_refused", why = String::from(why)),
        };
        self.reply(chat_id, &text).await;
    }

    /// A photo message: the largest size Telegram made of it becomes the image of a task whose prompt is the
    /// caption (`/task <name> <text>` picks the crewmate; without it a single crewmate takes it). Telegram sends
    /// every size of one photo in one message, smallest first; an album is several messages.
    async fn handle_photo(&self, chat_id: i64, msg: &Value) {
        let Some(hub) = self.hub() else {
            return;
        };
        let (ui, caption) = strip_ui(msg["caption"].as_str().unwrap_or("").trim());
        let (name, prompt) = match caption.strip_prefix("/task ") {
            Some(rest) => match rest.split_once(' ') {
                Some((n, p)) => (Some(n.trim().to_string()), p.trim()),
                None => (Some(rest.trim().to_string()), ""),
            },
            None => (None, caption),
        };
        let prompt = if prompt.is_empty() {
            "Look at the attached image."
        } else {
            prompt
        };
        let name = match name {
            Some(n) => n,
            None => {
                let crewmates = hub.crew.store.list_crewmates();
                if crewmates.len() != 1 {
                    self.reply(
                        chat_id,
                        "Which crewmate? Send the photo with the caption /task <name> <text>",
                    )
                    .await;
                    return;
                }
                crewmates[0].name.clone()
            }
        };
        let Some(file_id) = msg["photo"]
            .as_array()
            .and_then(|sizes| {
                sizes.iter().max_by_key(|s| {
                    s["width"].as_u64().unwrap_or(0) * s["height"].as_u64().unwrap_or(0)
                })
            })
            .and_then(|s| s["file_id"].as_str())
        else {
            return;
        };
        match self.download_file(file_id).await {
            Ok(bytes) => {
                let image = crate::crew::attachments::Incoming {
                    name: format!("telegram-{}.jpg", msg["message_id"].as_i64().unwrap_or(0)),
                    bytes,
                };
                self.create_task(chat_id, &name, prompt, &[image], ui).await;
            }
            Err(e) => {
                self.reply(
                    chat_id,
                    &format!("Not queued: the photo could not be fetched ({e})"),
                )
                .await;
            }
        }
    }

    /// The bytes of a Telegram file: `getFile` for its path, then the file endpoint. Through the same base URL
    /// and `/bot<token>/` segment as every other call (`api_url`, `file_url`), never more than the image cap.
    async fn download_file(&self, file_id: &str) -> Result<Vec<u8>, String> {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return Err("no token".into());
        }
        let client = crate::crew::push::HTTP
            .as_ref()
            .ok_or_else(|| "no http client".to_string())?;
        let info: Value = client
            .post(api_url(&self.api_base, &token, "getFile"))
            .json(&json!({"file_id": file_id}))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let path = info["result"]["file_path"]
            .as_str()
            .filter(|_| info["ok"] == true)
            .ok_or_else(|| "getFile refused".to_string())?;
        // Telegram says how big it is before it is downloaded: refuse an oversize file without fetching it.
        if info["result"]["file_size"].as_u64().unwrap_or(0)
            > crate::crew::attachments::MAX_BYTES as u64
        {
            return Err(format!(
                "over {} MiB",
                crate::crew::attachments::MAX_BYTES / (1024 * 1024)
            ));
        }
        let mut resp = client
            .get(file_url(&self.api_base, &token, path))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > crate::crew::attachments::MAX_BYTES {
                return Err(format!(
                    "over {} MiB",
                    crate::crew::attachments::MAX_BYTES / (1024 * 1024)
                ));
            }
        }
        Ok(bytes)
    }

    async fn create_task(
        &self,
        chat_id: i64,
        crewmate_name: &str,
        prompt: &str,
        images: &[crate::crew::attachments::Incoming],
        ui: bool,
    ) {
        let Some(hub) = self.hub() else {
            return;
        };
        let crewmates = hub.crew.store.list_crewmates();
        let Some(crewmate) = crewmates
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(crewmate_name))
        else {
            self.reply(chat_id, &format!("Crewmate '{}' not found.", crewmate_name))
                .await;
            return;
        };
        let max = crate::crew::handlers::tasks::MAX_PROMPT_CHARS;
        if prompt.chars().count() > max {
            self.reply(chat_id, &format!("The prompt is over {max} characters."))
                .await;
            return;
        }
        let mut task = crate::crew::store::Task::new("telegram", prompt);
        task.ui = ui;
        let task_id = task.id.clone();
        if let Err(e) =
            crate::crew::attachments::attach(&hub.crew.store, &crewmate.id, &mut task, images)
        {
            self.reply(chat_id, &format!("Not queued: {e}")).await;
            return;
        }
        if let Err(e) = hub.crew.store.append_task(&crewmate.id, &task) {
            // tree-access: attachments of a task hostd just made, under the crew store (task.id is hostd's own)
            let _ = std::fs::remove_dir_all(crate::crew::attachments::task_dir(
                &hub.crew.store,
                &crewmate.id,
                &task.id,
            ));
            self.reply(chat_id, &format!("Error: {e}")).await;
            return;
        }
        crate::crew::runner::enqueue(&hub, &hub.crew.store, crewmate, task);
        hub.broadcast(super::events::task(&crewmate.id, None));
        self.reply(chat_id, &format!("Task {task_id} queued."))
            .await;
    }

    async fn reply(&self, chat_id: i64, text: &str) {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let base = self.api_base.clone();
        let url = api_url(&base, &token, "sendMessage");
        let body = json!({"chat_id": chat_id, "text": text});
        let Some(client) = crate::crew::push::HTTP.as_ref() else {
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

    /// A `/start <text>` from `chat` as the poll loop would hand it over.
    async fn start_from(b: &TelegramBridge, chat: i64, text: &str) {
        b.handle_update(&serde_json::json!({
            "update_id": 1,
            "message": {"message_id": 1, "chat": {"id": chat}, "text": format!("/start {text}")}
        }))
        .await;
    }

    fn bridge(tmp: &tempfile::TempDir) -> TelegramBridge {
        let b = TelegramBridge::new_with_api(tmp.path().to_path_buf(), "http://127.0.0.1:1");
        b.config.lock().token = "tok".to_string();
        b
    }

    // Crew review P0, PR #106 comment 6874910355: the 6-digit code (10 min) had no attempt limit, so
    // a stranger could try a million codes and link their chat, which can queue tasks and approve
    // every tool call. After 5 wrong codes, from any chats, the live code dies: even the right one
    // is refused until the owner starts a new link.
    #[tokio::test(flavor = "multi_thread")]
    async fn five_wrong_codes_kill_the_link_code_even_for_the_right_one() {
        let tmp = tempfile::tempdir().unwrap();
        let b = bridge(&tmp);
        let code = b.start_link();
        let wrong = if code == "000000" { "000001" } else { "000000" };

        // Spread over several chats: the limit is on the code, not on a chat.
        for chat in 1..=5 {
            start_from(&b, 100 + chat, wrong).await;
        }
        assert_eq!(b.link_code(), None, "the code must be invalidated");

        start_from(&b, 7, &code).await;
        assert!(
            !b.has_linked_chat(),
            "the right code after the lockout must not link"
        );

        // A new link starts clean: its code works, and the earlier failures do not carry over.
        let fresh = b.start_link();
        start_from(&b, 7, &fresh).await;
        assert!(b.has_linked_chat(), "a new link must work again");
    }

    // The edge of the same rule: four wrong codes still leave the code alive (a typo is not an attack),
    // and a right one links.
    #[tokio::test(flavor = "multi_thread")]
    async fn four_wrong_codes_leave_the_code_usable() {
        let tmp = tempfile::tempdir().unwrap();
        let b = bridge(&tmp);
        let code = b.start_link();
        let wrong = if code == "000000" { "000001" } else { "000000" };
        for _ in 0..4 {
            start_from(&b, 5, wrong).await;
        }
        assert_eq!(b.link_code().as_deref(), Some(code.as_str()));
        start_from(&b, 5, &code).await;
        assert!(b.has_linked_chat());
    }

    // Without any link in progress there is nothing to guess: /start with a code is refused, and
    // those attempts must not poison the next link.
    #[tokio::test(flavor = "multi_thread")]
    async fn start_without_a_link_in_progress_never_links_and_does_not_poison_the_next_one() {
        let tmp = tempfile::tempdir().unwrap();
        let b = bridge(&tmp);
        for _ in 0..20 {
            start_from(&b, 9, "123456").await;
        }
        assert!(!b.has_linked_chat());
        let code = b.start_link();
        start_from(&b, 9, &code).await;
        assert!(b.has_linked_chat());
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

    // Incident (Bot -> Crew rename): a codemod rewrote the API path to `/crewmate<token>/…`, which
    // Telegram answers with 404, so no message would ever leave. `/bot<token>/<method>` is Telegram's
    // Bot API, not our vocabulary. A fake Telegram records the path the bridge really requests.
    #[tokio::test(flavor = "multi_thread")]
    async fn messages_go_to_telegrams_bot_api_path() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", api.local_addr().unwrap());
        let tmp = tempfile::tempdir().unwrap();
        let b = TelegramBridge::new_with_api(tmp.path().to_path_buf(), base);
        {
            let mut cfg = b.config.lock();
            cfg.token = "123:abc".to_string();
            cfg.chats = vec![42];
        }
        b.send_message("done");
        let (mut s, _) = tokio::time::timeout(atlas_core::scaling::HANG, api.accept())
            .await
            .expect("the bridge never called the Telegram API")
            .unwrap();
        let mut buf = vec![0u8; 4096];
        let n = s.read(&mut buf).await.unwrap();
        let _ = s
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\n\r\n{\"ok\":true}")
            .await;
        let request_line = String::from_utf8_lossy(&buf[..n])
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        assert_eq!(request_line, "POST /bot123:abc/sendMessage HTTP/1.1");
    }

    // `/ui` marks a UI task and is not part of the prompt; only a leading `/ui ` counts.
    #[test]
    fn a_leading_ui_command_marks_the_message_and_is_dropped() {
        use super::strip_ui;
        assert_eq!(
            strip_ui("/ui restyle the header"),
            (true, "restyle the header")
        );
        assert_eq!(
            strip_ui("/ui /task Ana fix the header"),
            (true, "/task Ana fix the header")
        );
        assert_eq!(
            strip_ui("restyle the /ui header"),
            (false, "restyle the /ui header")
        );
        assert_eq!(strip_ui("/uikit"), (false, "/uikit"));
        assert_eq!(
            strip_ui("/ui"),
            (true, ""),
            "bare: the handler replies with usage"
        );
    }

    // The body Telegram parses: the chat id first, a `media` array whose items point at the file parts, each
    // part named as pointed, the closing boundary last. The bytes are copied untouched.
    #[test]
    fn the_media_group_body_is_the_multipart_telegram_reads() {
        let imgs = vec![
            ("a.png".to_string(), b"\x89PNG\r\n\x1a\nAAAA".to_vec()),
            ("b.png".to_string(), b"\x89PNG\r\n\x1a\nBBBB".to_vec()),
        ];
        let (ctype, body) = super::media_group_body(42, &imgs);
        let boundary = ctype
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();
        let text = String::from_utf8_lossy(&body).to_string();
        assert!(text.starts_with(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"chat_id\"\r\n\r\n42\r\n"
        )));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
        // The `media` field is JSON: compared as a value, since its key order depends on whether
        // serde_json's `preserve_order` is on in the build (CI failed on the raw text).
        let media = text
            .split("name=\"media\"\r\n\r\n")
            .nth(1)
            .and_then(|rest| rest.split("\r\n--").next())
            .expect("a media part");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(media).unwrap(),
            serde_json::json!([
                {"type": "photo", "media": "attach://f0"},
                {"type": "photo", "media": "attach://f1"}
            ])
        );
        assert!(text.contains("name=\"f0\"; filename=\"a.png\""));
        assert!(text.contains("name=\"f1\"; filename=\"b.png\""));
        assert!(body.windows(12).any(|w| w == b"\x89PNG\r\n\x1a\nAAAA"));
        assert!(body.windows(12).any(|w| w == b"\x89PNG\r\n\x1a\nBBBB"));
    }

    // The bridge never sends more than ten in a group (the Bot API limit) and keeps the newest: the last
    // captures are the "after".
    #[test]
    fn a_card_sends_at_most_ten_images_and_the_newest() {
        use crate::crew::attachments::{self, Incoming, SOURCE_AGENT};
        use crate::crew::store::{CrewStore, Task};
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        let mut task = Task::new("test", "look");
        task.id = "t1".into();
        let png = |i: usize| Incoming {
            name: format!("{i:02}.png"),
            bytes: b"\x89PNG\r\n\x1a\nx".to_vec(),
        };
        let all: Vec<_> = (0..14).map(png).collect();
        attachments::add(&store, "c", &mut task, &all, SOURCE_AGENT).unwrap();
        store.append_task("c", &task).unwrap();
        let picked = super::card_images_of(&store, "c", "t1");
        assert_eq!(picked.len(), super::MEDIA_GROUP_MAX);
        assert_eq!(picked[0].0, "04.png");
        assert_eq!(picked[9].0, "13.png");
    }
}
