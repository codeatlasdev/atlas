//! Commands on one crewmate's definition: list, create, update, delete.

use serde_json::{Value, json};

use super::super::store::{Approval, CrewmateLook, EngineConfig, ProjectRef, TaskState};
use super::super::{Crewmate, events, now_s, pane, project, runner, triggers};
use crate::paths;
use crate::server::Hub;

// ── what both create and update check ────────────────────────────────────────

/// A crewmate's name: 1–40 characters once trimmed.
fn valid_name(name: &str) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 40 {
        return Err("name must be 1–40 characters".into());
    }
    Ok(name)
}

fn check_role(role: &str) -> Result<(), String> {
    if role.len() > 2000 {
        return Err("role must be ≤ 2000 characters".into());
    }
    Ok(())
}

fn check_agent(agent: &str) -> Result<(), String> {
    if atlas_core::agents::get(agent).is_none() {
        return Err(format!("unknown agent '{agent}'"));
    }
    Ok(())
}

/// The approval mode named on the wire, refused for an opencode engine (it is forced `--auto`).
fn parse_approval(approval: &str, engine_kind: &str) -> Result<Approval, String> {
    let approval_enum: Approval = serde_json::from_value(json!(approval))
        .map_err(|_| format!("invalid approval: {approval}"))?;
    if engine_kind == "opencode" && approval_enum == Approval::Ask {
        return Err(
            "OpenCode crewmates run without approval prompts (approval must be auto)".into(),
        );
    }
    Ok(approval_enum)
}

/// `CrewmateList`
pub(crate) fn list(hub: &Hub) -> Result<Value, String> {
    let crewmates = hub.crew.store.list_crewmates();
    let list: Vec<Value> = crewmates
        .into_iter()
        .map(|b| {
            let running_task = hub.crew.runners.running_task(&b.id);
            let pending_approval = hub.agents.pending_approvals().into_iter().find(|v| {
                v["pane_key"]
                    .as_str()
                    .map(|pk| pane::is_pane_of(pk, &b.id))
                    .unwrap_or(false)
            });
            let state = if pending_approval.is_some() {
                "needs_you"
            } else if running_task.is_some() {
                "working"
            } else {
                "idle"
            };
            let queued = hub.crew.runners.queue_len(&b.id);
            let last: Option<Value> = {
                let tasks = hub.crew.store.read_tasks(&b.id, 1);
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
                let pane_key = pane::key(&b.id, &task_id);
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
                let now = now_s();
                let triggers: Vec<Value> = b
                    .triggers
                    .iter()
                    .map(|t| {
                        let next = triggers::compute_next_fire(
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
    Ok(json!({ "crewmates": list }))
}

/// `CrewmateCreate`
#[allow(clippy::too_many_arguments)]
pub(crate) fn create(
    hub: &Hub,
    name: String,
    role: String,
    agent: String,
    approval: String,
    workdir: Option<String>,
    look: Option<CrewmateLook>,
    engine: Option<EngineConfig>,
    project: Option<ProjectRef>,
    setup_step: Option<String>,
) -> Result<Value, String> {
    let name = valid_name(&name)?;
    check_role(&role)?;
    // opencode is forced --auto; reject ask at creation time.
    let engine_kind = engine.as_ref().map_or("", |e| e.kind.as_str());
    let approval_enum = parse_approval(&approval, engine_kind)?;
    check_agent(&agent)?;
    if let Some(l) = &look {
        l.validate()?;
    }
    let id = paths::new_id();
    if let Some(ref wd) = workdir {
        std::fs::create_dir_all(wd).map_err(|e| format!("cannot create workdir: {e}"))?;
    }
    let crewmate = Crewmate {
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
    hub.crew
        .store
        .save_crewmate(&crewmate)
        .map_err(|e| format!("save_crewmate: {e}"))?;
    let _ = hub.crew.store.write_memory(&id, "");
    warm_checkout(hub, &crewmate);
    serde_json::to_value(&crewmate).map_err(|e| e.to_string())
}

/// `CrewmateUpdate`
#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    hub: &Hub,
    id: String,
    name: Option<String>,
    role: Option<String>,
    agent: Option<String>,
    approval: Option<String>,
    look: Option<CrewmateLook>,
    engine: Option<EngineConfig>,
    project: Option<ProjectRef>,
    setup_step: Option<Option<String>>,
    short_replies: Option<Option<bool>>,
) -> Result<Value, String> {
    let mut crewmate = hub
        .crew
        .store
        .load_crewmate(&id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    if let Some(n) = name {
        crewmate.name = valid_name(&n)?;
    }
    if let Some(r) = role {
        check_role(&r)?;
        crewmate.role = r;
    }
    if let Some(a) = agent {
        check_agent(&a)?;
        crewmate.agent = a;
    }
    if let Some(ap) = approval {
        // Check updated engine kind too (engine may be set in this same call).
        let effective_kind = engine
            .as_ref()
            .map(|e| e.kind.as_str())
            .or_else(|| crewmate.engine.as_ref().map(|e| e.kind.as_str()))
            .unwrap_or("");
        crewmate.approval = parse_approval(&ap, effective_kind)?;
    }
    if let Some(l) = look {
        l.validate()?;
        crewmate.look = Some(l);
    }
    if let Some(e) = engine {
        crewmate.engine = Some(e);
    }
    if let Some(p) = project {
        // A folder must exist on this computer now, not fail every later task ("Something else" was saved as
        // a folder by the setup once). The same check project::prepare runs before each task.
        if let ProjectRef::Path { path } = &p {
            project::prepare_path(path).map_err(|e| format!("project folder {path}: {e}"))?;
        }
        crewmate.project = Some(p);
    }
    // Outer Some = caller wants to change setup_step; inner None = clear it.
    if let Some(step) = setup_step {
        crewmate.setup_step = step;
    }
    if let Some(sr) = short_replies {
        crewmate.short_replies = sr;
    }
    hub.crew
        .store
        .save_crewmate(&crewmate)
        .map_err(|e| format!("save_crewmate: {e}"))?;
    // Notify connected clients that the crewmate changed.
    hub.broadcast(events::crewmate_updated(&crewmate.id));
    warm_checkout(hub, &crewmate);
    serde_json::to_value(&crewmate).map_err(|e| e.to_string())
}

/// `CrewmateDelete`
pub(crate) fn delete(hub: &Hub, id: String) -> Result<Value, String> {
    // Cancel all queued/running tasks first.
    let tasks = hub.crew.store.read_tasks(&id, usize::MAX);
    for t in tasks {
        if matches!(
            t.state,
            TaskState::Queued | TaskState::Running | TaskState::WaitingAction
        ) {
            let _ = runner::cancel(hub, &hub.crew.store, &id, &t.id);
        }
    }
    hub.crew
        .store
        .delete_crewmate_dir(&id)
        .map_err(|e| format!("delete: {e}"))?;
    Ok(json!({}))
}

/// Start checking out a Git project in the background as soon as the crewmate has it, so the first task does not wait for
/// the clone (~90 s on atlas-test-ship before). Failures are left to the task, which reports them (a private repo also
/// needs GitHub connected first). `project::prepare` serializes with a task that asks while this runs.
fn warm_checkout(hub: &Hub, crewmate: &Crewmate) {
    let Some(project @ ProjectRef::Git { .. }) = crewmate.project.clone() else {
        return;
    };
    let (id, dir) = (crewmate.id.clone(), hub.crew.dir.clone());
    tokio::spawn(async move {
        if let Err(e) = project::prepare(&project, &id, &dir).await {
            eprintln!("atlas crew: checkout for {id} not ready yet: {e}");
        }
    });
}
