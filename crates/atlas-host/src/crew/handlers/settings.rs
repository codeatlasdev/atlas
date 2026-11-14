//! Everything else a crewmate or the crew keeps: memory, routines, triggers, secrets, Telegram, push
//! tokens, the crew-wide reply settings and the engine limits.

use serde_json::{Value, json};

use super::super::store::{Routine, Trigger, TriggerKind};
use super::super::{events, now_s, push, schedule, triggers};
use crate::server::Hub;

/// `CrewmateMemory`
pub(crate) fn memory(hub: &Hub, id: String) -> Result<Value, String> {
    let text = hub.crew.store.read_memory(&id);
    Ok(json!({ "text": text }))
}

/// `CrewmateMemorySet`
pub(crate) fn set_memory(hub: &Hub, id: String, text: String) -> Result<Value, String> {
    hub.crew
        .store
        .write_memory(&id, &text)
        .map_err(|e| format!("write_memory: {e}"))?;
    Ok(json!({}))
}

/// `CrewmateRoutineSet`
pub(crate) fn set_routines(hub: &Hub, id: String, routines: Vec<Value>) -> Result<Value, String> {
    let mut crewmate = hub
        .crew
        .store
        .load_crewmate(&id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    // Parse and validate the routines.
    let parsed: Vec<Routine> =
        serde_json::from_value(json!(routines)).map_err(|e| format!("invalid routines: {e}"))?;
    for r in &parsed {
        schedule::validate_cron(&r.cron)?;
    }
    crewmate.routines = parsed;
    hub.crew
        .store
        .save_crewmate(&crewmate)
        .map_err(|e| format!("save_crewmate: {e}"))?;
    Ok(json!({}))
}

/// `CrewmateTriggersSet`
pub(crate) fn set_triggers(hub: &Hub, id: String, triggers: Vec<Value>) -> Result<Value, String> {
    let mut crewmate = hub
        .crew
        .store
        .load_crewmate(&id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    let old_triggers = crewmate.triggers.clone();
    let baseline_now = now_s();
    let new_triggers = triggers
        .into_iter()
        .map(|v| {
            let mut t: Trigger =
                serde_json::from_value(v).map_err(|e| format!("invalid trigger: {e}"))?;
            // Validate tz before anything else.
            if let Some(tz_str) = &t.tz {
                tz_str
                    .parse::<chrono_tz::Tz>()
                    .map_err(|_| format!("unknown timezone: {tz_str}"))?;
            }
            triggers::validate_trigger(&t)?;
            // Preserve last_fired of an existing trigger with the same id;
            // baseline new schedule/interval triggers so first fire is on-schedule.
            if let Some(existing) = old_triggers.iter().find(|ot| ot.id == t.id) {
                t.last_fired = existing.last_fired;
            } else {
                match &t.kind {
                    TriggerKind::Schedule { .. } | TriggerKind::Interval { .. } => {
                        t.last_fired = Some(baseline_now);
                    }
                    _ => {}
                }
            }
            Ok(t)
        })
        .collect::<Result<Vec<_>, String>>()?;
    crewmate.triggers = new_triggers;
    hub.crew
        .store
        .save_crewmate(&crewmate)
        .map_err(|e| format!("save_crewmate: {e}"))?;
    hub.broadcast(events::crewmate_updated(&id));
    Ok(json!({}))
}

/// `CrewmateSecretSet`
pub(crate) fn set_secret(
    hub: &Hub,
    id: String,
    key: String,
    value: String,
) -> Result<Value, String> {
    // Validate crewmate exists before storing secret.
    hub.crew
        .store
        .load_crewmate(&id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    hub.crew
        .store
        .set_secret(&id, &key, &value)
        .map_err(|e| format!("set_secret: {e}"))?;
    Ok(json!({}))
}

/// `CrewTelegramSet`
pub(crate) fn telegram_set(hub: &Hub, token: Option<String>) -> Result<Value, String> {
    hub.crew
        .telegram
        .set_token(token)
        .map_err(|e| format!("set_token: {e}"))?;
    Ok(json!({}))
}

/// `CrewTelegramLink`
pub(crate) fn telegram_link(hub: &Hub) -> Result<Value, String> {
    let code = hub.crew.telegram.start_link();
    Ok(json!({ "code": code }))
}

/// `CrewTelegramStatus`: `{configured, chats}`; the token itself never leaves the server.
pub(crate) fn telegram_status(hub: &Hub) -> Result<Value, String> {
    Ok(hub.crew.telegram.status())
}

/// `PushRegister`
pub(crate) fn push_register(hub: &Hub, token: String, platform: String) -> Result<Value, String> {
    // We need node_id; derive it from the mobile connection's node_id.
    // For now accept as a separate field; node_id is passed implicitly by the
    // mobile session layer.  We store an empty node_id when called from the desktop.
    // The node_id is injected by server.rs before routing here.
    // Use the token as a proxy for deduplication when node_id is unknown.
    let mut tokens = hub.crew.push_tokens.lock();
    // Replace existing entry with same token (any node_id).
    tokens.retain(|t| t.token != token);
    tokens.push(push::new_token("", &token, &platform));
    push::save_tokens(&hub.crew.dir, &tokens);
    Ok(json!({}))
}

/// `PushUnregister`
pub(crate) fn push_unregister(hub: &Hub, token: String) -> Result<Value, String> {
    let mut tokens = hub.crew.push_tokens.lock();
    tokens.retain(|t| t.token != token);
    push::save_tokens(&hub.crew.dir, &tokens);
    Ok(json!({}))
}

/// `CrewSettingsGet`
pub(crate) fn get(hub: &Hub) -> Result<Value, String> {
    let settings = hub.crew.store.load_crew_settings();
    Ok(json!({
        "short_replies": settings.short_replies,
        "short_replies_model": settings.short_replies_model,
    }))
}

/// `CrewSettingsSet`
pub(crate) fn set(
    hub: &Hub,
    short_replies: bool,
    short_replies_model: Option<String>,
) -> Result<Value, String> {
    // Load, change, save: the file also holds `paused`, which this command must never reset.
    let mut settings = hub.crew.store.load_crew_settings();
    settings.short_replies = short_replies;
    settings.short_replies_model = short_replies_model;
    hub.crew
        .store
        .save_crew_settings(&settings)
        .map_err(|e| format!("save_crew_settings: {e}"))?;
    Ok(json!({}))
}

/// `EngineLimits`
pub(crate) fn engine_limits() -> Result<Value, String> {
    Ok(json!({
        "message": "no machine-readable quota or rate-limit information is exposed by any supported engine CLI",
        "limits": {}
    }))
}
