//! Everything else a crewmate or the crew keeps: memory, routines, triggers, secrets, Telegram, push
//! tokens, the crew-wide reply settings and the engine limits.

use serde_json::{Value, json};

use super::super::store::{Mode, Routine, Trigger, TriggerKind};
use super::super::{events, now_s, schedule, triggers};
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
    // A routine kept as it is stays, even when its cron no longer parses (one saved under an older cron parser):
    // it must not block editing the others, and `crewmate_list` marks it `invalid` until the user fixes or
    // deletes it. Only what is new or changed is validated.
    for r in &parsed {
        if !crewmate
            .routines
            .iter()
            .any(|old| old.id == r.id && old.cron == r.cron)
        {
            schedule::validate_cron(&r.cron)?;
        }
    }
    let old = std::mem::replace(&mut crewmate.routines, parsed);
    hub.crew
        .store
        .save_crewmate(&crewmate)
        .map_err(|e| format!("save_crewmate: {e}"))?;
    hub.crew.audit.record(
        Some(&id),
        "config_changed",
        json!({"field": "routines", "from": old, "to": crewmate.routines}),
    );
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
            // Validate what is new or changed; a trigger kept as it is stays (see `set_routines`).
            let unchanged = old_triggers.iter().any(|ot| {
                ot.id == t.id
                    && ot.tz == t.tz
                    && serde_json::to_value(&ot.kind).ok() == serde_json::to_value(&t.kind).ok()
            });
            if !unchanged {
                // Validate tz before anything else.
                if let Some(tz_str) = &t.tz {
                    schedule::validate_tz(tz_str)?;
                }
                triggers::validate_trigger(&t)?;
            }
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
    // The whole list goes in: a trigger is what lets text from outside start a task, so what it allows
    // (authors, allow_auto) is exactly what an audit has to show. Webhook tokens are secrets of a kind:
    // only their presence is kept.
    hub.crew.audit.record(
        Some(&id),
        "config_changed",
        json!({
            "field": "triggers",
            "from": audit_triggers(&old_triggers),
            "to": audit_triggers(&crewmate.triggers),
        }),
    );
    hub.broadcast(events::crewmate_updated(&id));
    Ok(json!({}))
}

/// A trigger list for the audit: webhook tokens replaced by `"[token]"`.
fn audit_triggers(list: &[Trigger]) -> Value {
    let mut v = serde_json::to_value(list).unwrap_or_default();
    for t in v.as_array_mut().into_iter().flatten() {
        if t["kind"]["token"].is_string() {
            t["kind"]["token"] = json!("[token]");
        }
    }
    v
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
    // The name only, never the value.
    hub.crew
        .audit
        .record(Some(&id), "secret_set", json!({"name": key}));
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

/// `CrewSettingsGet`
pub(crate) fn get(hub: &Hub) -> Result<Value, String> {
    let settings = hub.crew.store.load_crew_settings();
    Ok(json!({
        "short_replies": settings.short_replies,
        "short_replies_model": settings.short_replies_model,
        "max_agents": settings.max_agents,
        "agent_memory_max": settings.agent_memory_max,
        "default_approval": settings.default_approval,
        "allow_bypass": settings.allow_bypass,
        "public_url": settings.public_url,
        // What agents run under now, or why there is no cap: asked of the helper, never assumed.
        "memory_cap": super::super::memory_cap::report(hub),
    }))
}

/// What `crew_settings_set` carries.
pub(crate) struct SettingsIn {
    pub short_replies: bool,
    pub short_replies_model: Option<String>,
    pub max_agents: Option<usize>,
    pub agent_memory_max: Option<u64>,
    pub default_approval: Option<String>,
    pub allow_bypass: Option<bool>,
    /// Absent leaves it, `""` clears it (webhooks off).
    pub public_url: Option<String>,
}

/// `CrewSettingsSet`
pub(crate) async fn set(hub: &Hub, input: SettingsIn) -> Result<Value, String> {
    let SettingsIn {
        short_replies,
        short_replies_model,
        max_agents,
        agent_memory_max,
        default_approval,
        allow_bypass,
        public_url,
    } = input;
    if max_agents == Some(0) {
        return Err("max_agents must be at least 1: 0 would stop every task".into());
    }
    let memory_max = agent_memory_max
        .map(super::super::memory_cap::parse_setting)
        .transpose()?;
    let public_url = public_url
        .as_deref()
        .map(super::super::store::normalize_public_url)
        .transpose()?;
    // Load, change, save: the file also holds `paused`, which this command must never reset.
    let mut settings = hub.crew.store.load_crew_settings();
    settings.short_replies = short_replies;
    settings.short_replies_model = short_replies_model;
    if let Some(n) = max_agents {
        settings.max_agents = n;
    }
    if let Some(m) = memory_max {
        settings.agent_memory_max = m;
    }
    if let Some(allow) = allow_bypass {
        settings.allow_bypass = allow;
    }
    let old_url = settings.public_url.clone();
    if let Some(url) = public_url {
        settings.public_url = url;
    }
    if let Some(m) = default_approval.as_deref().map(Mode::parse).transpose()? {
        if m == Mode::Bypass && !settings.allow_bypass {
            return Err("bypass is not allowed on this crew (allow_bypass is off)".into());
        }
        settings.default_approval = m;
    }
    // Taking Bypass off the crew also takes it off the default a new crewmate would start in.
    if !settings.allow_bypass && settings.default_approval == Mode::Bypass {
        settings.default_approval = Mode::Ask;
    }
    hub.crew
        .store
        .save_crew_settings(&settings)
        .map_err(|e| format!("save_crew_settings: {e}"))?;
    if !settings.allow_bypass {
        super::super::approval::revoke_bypass(hub, "owner");
    }
    hub.crew.agent_slots.set_limit(settings.max_agents);
    // The cap depends on both numbers: the helper rewrites memory.max of the agents that run.
    let cap = super::super::memory_cap::apply(hub);
    // The webhook follows the URL: what was verified under the old one is forgotten and GitHub is told the new one.
    // The setting is saved either way; a failure to tell GitHub is said in the reply and retried by the
    // reconciliation loop.
    let mut webhook_error = None;
    if settings.public_url != old_url {
        hub.crew.audit.record(
            None,
            "github_webhook_url_set",
            json!({"url": settings.public_url}),
        );
        webhook_error = hub
            .crew
            .github_app
            .webhook_url_changed(settings.public_url.as_deref())
            .await
            .err();
    }
    Ok(json!({
        "memory_cap": cap,
        "default_approval": settings.default_approval,
        "allow_bypass": settings.allow_bypass,
        "public_url": settings.public_url,
        "webhook_error": webhook_error,
    }))
}

/// `EngineLimits`
pub(crate) fn engine_limits() -> Result<Value, String> {
    Ok(json!({
        "message": "no machine-readable quota or rate-limit information is exposed by any supported engine CLI",
        "limits": {}
    }))
}
