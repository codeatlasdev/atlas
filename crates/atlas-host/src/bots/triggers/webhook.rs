//! Webhook trigger: route matching and token-validated task enqueue.
//!
//! The hook server calls `parse_webhook_path` to identify `(bot_id, token)`,
//! then `handle` to validate the token against the bot's triggers and fire.
//!
//! Route: `POST /bots/<bot_id>/hook/<token>`
//! Body (≤ 64 KiB, text or JSON) becomes the `{{event}}` summary.

use std::sync::Arc;

use crate::bots::store::TriggerKind;
use crate::server::Hub;

/// Extract `(bot_id, token)` from `/bots/<bot_id>/hook/<token>`.
///
/// Returns `None` for any malformed path.
#[allow(dead_code)]
pub(crate) fn parse_webhook_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/bots/")?;
    let slash = rest.find('/').filter(|&i| i > 0)?;
    let (bot_id, after) = rest.split_at(slash);
    let token = after.strip_prefix("/hook/")?;
    if token.is_empty() || token.contains('/') {
        return None;
    }
    Some((bot_id.to_string(), token.to_string()))
}

/// Handle a webhook POST for `bot_id` / `token`.
///
/// Returns an HTTP status line suitable for a response header.
/// Body is truncated to 200 chars for the `{{event}}` summary.
pub(crate) fn handle(hub: &Arc<Hub>, bot_id: &str, token: &str, body: &str) -> &'static str {
    let Ok(bot) = hub.bots.store.load_bot(bot_id) else {
        return "404 Not Found";
    };
    let trigger = bot.triggers.iter().find(|t| {
        t.enabled && matches!(&t.kind, TriggerKind::Webhook { token: tk } if tk == token)
    });
    let Some(trigger) = trigger else {
        return "403 Forbidden";
    };
    // Trim body to a reasonable summary length.
    let event: String = if body.len() <= 200 {
        body.trim().to_string()
    } else {
        format!("{}…", body[..200].trim_end())
    };
    let prompt_template = trigger.prompt.clone();
    let trigger_id = trigger.id.clone();
    super::fire(hub, bot_id, &trigger_id, &prompt_template, &event);
    "204 No Content"
}
