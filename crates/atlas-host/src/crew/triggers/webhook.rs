//! Webhook trigger: route matching and token-validated task enqueue.
//!
//! The hook server calls `parse_webhook_path` to identify `(crewmate_id, token)`,
//! then `handle` to validate the token against the crewmate's triggers and fire.
//!
//! Route: `POST /crew/<crewmate_id>/hook/<token>`, and `POST /bots/<crewmate_id>/hook/<token>`: the
//! URL webhooks were configured with before the Bot -> Crew rename. Those URLs live in other
//! services (GitHub, CI), so the old one keeps working; only webhooks use it.
//! Body (≤ 64 KiB, text or JSON) becomes the `{{event}}` summary.

use std::sync::Arc;

use crate::crew::store::TriggerKind;
use crate::server::Hub;

/// The prefix of webhook URLs issued before the rename (see the module docs).
pub(crate) const LEGACY_PREFIX: &str = "/bots/";

/// Extract `(crewmate_id, token)` from `/crew/<crewmate_id>/hook/<token>` (or the legacy
/// `/bots/…`). Returns `None` for any malformed path.
pub(crate) fn parse_webhook_path(path: &str) -> Option<(String, String)> {
    let rest = path
        .strip_prefix("/crew/")
        .or_else(|| path.strip_prefix(LEGACY_PREFIX))?;
    let slash = rest.find('/').filter(|&i| i > 0)?;
    let (crewmate_id, after) = rest.split_at(slash);
    let token = after.strip_prefix("/hook/")?;
    if token.is_empty() || token.contains('/') {
        return None;
    }
    Some((crewmate_id.to_string(), token.to_string()))
}

/// Handle a webhook POST for `crewmate_id` / `token`.
///
/// Body is truncated to 200 chars for the `{{event}}` summary, and handed over as untrusted text
/// (quoted, behind a fixed preamble); the task runs as `ask` unless the trigger has `allow_auto`.
pub(crate) fn handle(hub: &Arc<Hub>, crewmate_id: &str, token: &str, body: &str) -> &'static str {
    let Ok(crewmate) = hub.crew.store.load_crewmate(crewmate_id) else {
        return "404 Not Found";
    };
    let trigger = crewmate.triggers.iter().find(|t| {
        t.enabled && matches!(&t.kind, TriggerKind::Webhook { token: tk, .. } if tk == token)
    });
    let Some(trigger) = trigger else {
        return "403 Forbidden";
    };
    // Trim body to a reasonable summary length (by characters: a byte cut can split a letter).
    let body = body.trim();
    let event: String = if body.chars().count() <= 200 {
        body.to_string()
    } else {
        format!("{}…", body.chars().take(200).collect::<String>().trim_end())
    };
    let prompt_template = trigger.prompt.clone();
    let trigger_id = trigger.id.clone();
    super::fire(
        hub,
        crewmate_id,
        &trigger_id,
        &prompt_template,
        super::Event::Stranger {
            source: "a webhook request body",
            text: &event,
        },
    );
    "204 No Content"
}
