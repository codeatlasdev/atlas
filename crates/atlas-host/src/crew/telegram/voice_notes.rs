//! Voice notes on Telegram (#647): a note from the owner's chat is transcribed on the crew server, echoed back as
//! text with **Send as task / Edit / Cancel**, and only Send makes a task (marked `dictated`).
//!
//! - Only a linked chat gets here (`handle_message` returns for any other before it looks at the message).
//! - At most [`MAX_BYTES`] (20 MB, the Bot API's `getFile` limit) and [`MAX_SECONDS`]: a longer note is
//!   transcribed up to the cap and the echo says so, nothing is discarded silently.
//! - The audio is decoded in a scratch dir that is removed before the echo is sent, whatever the outcome.
//! - The engine is the crew's: a failed one is told to the chat, no other is tried.
//! - What waits for a tap lives in memory only, for [`WAIT`]: a restart drops it, and a tap on a note that is gone
//!   says so.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use atlas_i18n::t;
use serde_json::{Value, json};

use super::{TelegramBridge, api_url};
use crate::crew::voice::{self, audio};

/// What Telegram's `getFile` serves: 20 MB.
pub(super) const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// How long an echo waits for a tap.
const WAIT: Duration = Duration::from_secs(60 * 60);
/// Echoes waiting at once; a new one past this drops the oldest.
const MAX_WAITING: usize = 20;

/// A transcription waiting for Send, Edit or Cancel.
pub(super) struct Pending {
    chat: i64,
    crewmate: String,
    text: String,
    at: Instant,
}

#[derive(Default)]
pub(super) struct VoiceState {
    waiting: HashMap<String, Pending>,
    /// Chats whose next plain message is the corrected text of this pending id.
    editing: HashMap<i64, String>,
}

impl VoiceState {
    fn sweep(&mut self) {
        self.waiting.retain(|_, p| p.at.elapsed() < WAIT);
        let live = &self.waiting;
        self.editing.retain(|_, id| live.contains_key(id));
    }

    fn add(&mut self, id: String, p: Pending) {
        self.sweep();
        if self.waiting.len() >= MAX_WAITING
            && let Some(oldest) = self
                .waiting
                .iter()
                .min_by_key(|(_, p)| p.at)
                .map(|(k, _)| k.clone())
        {
            self.waiting.remove(&oldest);
        }
        self.waiting.insert(id, p);
    }
}

/// `v:<id>:<s|e|c>`: the data of a button of an echo.
fn data(id: &str, action: char) -> String {
    format!("v:{id}:{action}")
}

impl TelegramBridge {
    /// A voice note (or an audio file) from a linked chat.
    pub(super) async fn handle_voice(&self, chat_id: i64, msg: &Value) {
        let Some(hub) = self.hub() else {
            return;
        };
        let note = if msg["voice"].is_object() {
            &msg["voice"]
        } else {
            &msg["audio"]
        };
        // The crewmate: named in the caption (`/task <name>`), else the only one.
        let caption = msg["caption"].as_str().unwrap_or("").trim();
        let name = match caption.strip_prefix("/task ") {
            Some(rest) => Some(rest.split_whitespace().next().unwrap_or("").to_string()),
            None => None,
        };
        let crewmates = hub.crew.store.list_crewmates();
        let crewmate = match &name {
            Some(n) => crewmates.iter().find(|c| c.name.eq_ignore_ascii_case(n)),
            None => (crewmates.len() == 1).then(|| &crewmates[0]),
        };
        let Some(crewmate) = crewmate else {
            let text = match &name {
                Some(n) => t!("cmd_hostd_telegram_mode_not_found", name = n.as_str()),
                None => t!("cmd_hostd_telegram_voice_which"),
            };
            self.reply(chat_id, &text).await;
            return;
        };
        // Refused before it is fetched: Telegram says how big it is.
        let size = note["file_size"].as_u64().unwrap_or(0);
        if size > MAX_BYTES {
            self.reply(
                chat_id,
                &t!(
                    "cmd_hostd_telegram_voice_too_big",
                    mb = (MAX_BYTES / (1024 * 1024)).to_string()
                ),
            )
            .await;
            return;
        }
        let Some(file_id) = note["file_id"].as_str() else {
            return;
        };
        let bytes = match self.download_limited(file_id, MAX_BYTES as usize).await {
            Ok(b) => b,
            Err(e) => {
                let text = if e.starts_with("over ") {
                    t!(
                        "cmd_hostd_telegram_voice_too_big",
                        mb = (MAX_BYTES / (1024 * 1024)).to_string()
                    )
                } else {
                    t!("cmd_hostd_telegram_voice_unreadable")
                };
                self.reply(chat_id, &text).await;
                return;
            }
        };
        let dir = voice::engine::voice_dir(&hub.crew.dir);
        let (pcm, longer) =
            match voice::engine::decode_to_pcm(&dir, &bytes, audio::MAX_SECONDS).await {
                Ok(v) => v,
                Err(_) => {
                    self.reply(chat_id, &t!("cmd_hostd_telegram_voice_unreadable"))
                        .await;
                    return;
                }
            };
        drop(bytes);
        let heard = voice::run(&hub, &crewmate.id, &pcm, None, false).await;
        drop(pcm);
        let transcript = match heard {
            Ok(t) => t,
            Err(f) => {
                self.reply(chat_id, &failure_text(&f)).await;
                return;
            }
        };
        if transcript.text.trim().is_empty() {
            self.reply(chat_id, &t!("cmd_hostd_telegram_voice_silent"))
                .await;
            return;
        }
        let mut echo = t!(
            "cmd_hostd_telegram_voice_echo",
            text = transcript.text.as_str(),
            engine = transcript.label.as_str(),
            crewmate = crewmate.name.as_str()
        );
        if longer {
            echo.push_str("\n");
            echo.push_str(&t!(
                "cmd_hostd_telegram_voice_truncated",
                seconds = audio::MAX_SECONDS.to_string()
            ));
        }
        self.offer(chat_id, &crewmate.id, transcript.text, echo)
            .await;
    }

    /// Echo `text` with the three buttons and remember it until one is tapped.
    async fn offer(&self, chat_id: i64, crewmate: &str, text: String, echo: String) {
        let Ok(id) = crate::paths::random_hex(6) else {
            return;
        };
        self.voice.lock().add(
            id.clone(),
            Pending {
                chat: chat_id,
                crewmate: crewmate.to_string(),
                text,
                at: Instant::now(),
            },
        );
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let body = json!({
            "chat_id": chat_id,
            "text": echo,
            "reply_markup": {"inline_keyboard": [[
                {"text": t!("cmd_hostd_telegram_voice_send"), "callback_data": data(&id, 's')},
                {"text": t!("common_action_edit"), "callback_data": data(&id, 'e')},
                {"text": t!("common_action_cancel"), "callback_data": data(&id, 'c')},
            ]]},
        });
        if let Some(client) = crate::crew::push::HTTP.as_ref() {
            let _ = client
                .post(api_url(&self.api_base, &token, "sendMessage"))
                .json(&body)
                .send()
                .await;
        }
    }

    /// A tap on a button of an echo (`v:<id>:<s|e|c>`).
    pub(super) async fn handle_voice_tap(&self, chat_id: i64, msg_id: i64, id: &str, action: &str) {
        // A tap from a chat that is not the owner's is ignored, like any other message from it.
        if !self.config.lock().chats.contains(&chat_id) {
            return;
        }
        // Decided under the lock; told to the chat after it is released.
        let taken: Option<Option<Pending>> = {
            let mut v = self.voice.lock();
            v.sweep();
            if v.waiting.get(id).is_some_and(|p| p.chat == chat_id) {
                if action == "e" {
                    // Edit keeps it waiting: the corrected text replaces it.
                    v.editing.insert(chat_id, id.to_string());
                    Some(None)
                } else {
                    Some(v.waiting.remove(id))
                }
            } else {
                None
            }
        };
        match (action, taken) {
            (_, None) => {
                self.edit_text(chat_id, msg_id, &t!("cmd_hostd_telegram_voice_gone"))
                    .await;
            }
            ("e", _) => {
                self.edit_text(chat_id, msg_id, &t!("cmd_hostd_telegram_voice_edit"))
                    .await;
            }
            ("s", Some(Some(p))) => {
                self.edit_text(chat_id, msg_id, &t!("cmd_hostd_telegram_voice_sent"))
                    .await;
                self.queue_dictated(chat_id, &p.crewmate, &p.text).await;
            }
            ("c", Some(Some(_))) => {
                self.edit_text(chat_id, msg_id, &t!("cmd_hostd_telegram_voice_cancelled"))
                    .await;
            }
            _ => {}
        }
    }

    /// The next plain message of a chat that tapped Edit: the corrected text, echoed again with the buttons. It
    /// is never sent by itself. `true` when the message was taken.
    pub(super) async fn take_correction(&self, chat_id: i64, text: &str) -> bool {
        if text.is_empty() || text.starts_with('/') {
            return false;
        }
        let taken = {
            let mut v = self.voice.lock();
            v.sweep();
            let id = v.editing.remove(&chat_id);
            id.and_then(|id| v.waiting.remove(&id))
        };
        let Some(p) = taken else {
            return false;
        };
        let name = self
            .hub()
            .and_then(|h| h.crew.store.load_crewmate(&p.crewmate).ok())
            .map(|c| c.name)
            .unwrap_or_default();
        let echo = t!(
            "cmd_hostd_telegram_voice_echo_edited",
            text = text,
            crewmate = name.as_str()
        );
        self.offer(chat_id, &p.crewmate, text.to_string(), echo)
            .await;
        true
    }

    async fn queue_dictated(&self, chat_id: i64, crewmate_id: &str, text: &str) {
        let name = self
            .hub()
            .and_then(|h| h.crew.store.load_crewmate(crewmate_id).ok())
            .map(|c| c.name);
        let Some(name) = name else {
            self.reply(chat_id, &t!("cmd_hostd_telegram_voice_gone"))
                .await;
            return;
        };
        self.create_task(chat_id, &name, text, &[], false, true)
            .await;
    }

    /// Replace the text of a message of ours, which also takes its buttons away.
    async fn edit_text(&self, chat_id: i64, msg_id: i64, text: &str) {
        let token = self.config.lock().token.clone();
        if token.is_empty() {
            return;
        }
        let body = json!({"chat_id": chat_id, "message_id": msg_id, "text": text});
        if let Some(client) = crate::crew::push::HTTP.as_ref() {
            let _ = client
                .post(api_url(&self.api_base, &token, "editMessageText"))
                .json(&body)
                .send()
                .await;
        }
    }
}

/// What the chat is told when a transcription failed.
fn failure_text(f: &crate::protocol::Failure) -> String {
    use crate::protocol::Failure;
    let Failure::Coded(c) = f else {
        return t!(
            "cmd_hostd_telegram_voice_failed_plain",
            why = String::from(f.clone()).as_str()
        );
    };
    match c.code {
        voice::OFF => t!("cmd_hostd_telegram_voice_off"),
        voice::NOT_INSTALLED => t!("cmd_hostd_telegram_voice_not_installed"),
        voice::ENGINE_FAILED => t!(
            "cmd_hostd_telegram_voice_failed",
            engine = c.args.get("engine").and_then(Value::as_str).unwrap_or(""),
            detail = c.args.get("detail").and_then(Value::as_str).unwrap_or("")
        ),
        _ => c.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(chat: i64, age: Duration) -> Pending {
        Pending {
            chat,
            crewmate: "c".into(),
            text: "t".into(),
            at: Instant::now() - age,
        }
    }

    #[test]
    fn what_waits_for_a_tap_expires_and_is_bounded() {
        let mut v = VoiceState::default();
        v.add("old".into(), pending(1, WAIT + Duration::from_secs(1)));
        v.editing.insert(1, "old".into());
        v.add("new".into(), pending(1, Duration::ZERO));
        assert!(
            !v.waiting.contains_key("old"),
            "a note past its hour is gone"
        );
        assert!(v.editing.is_empty(), "so is the edit that waited for it");
        for i in 0..MAX_WAITING + 5 {
            v.add(format!("n{i}"), pending(1, Duration::ZERO));
        }
        assert_eq!(v.waiting.len(), MAX_WAITING);
    }

    #[test]
    fn a_button_names_its_note_and_fits_telegrams_limit() {
        for a in ['s', 'e', 'c'] {
            let d = data("0123456789ab", a);
            assert!(d.len() <= 64, "callback_data is at most 64 bytes");
            let parts: Vec<&str> = d.splitn(3, ':').collect();
            assert_eq!(parts, ["v", "0123456789ab", &a.to_string()]);
        }
    }
}
