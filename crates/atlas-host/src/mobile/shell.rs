//! `pty_open`: a new shell for a phone, in a folder hostd itself resolves.
//!
//! A phone never sends a path or a program (`pty_create` stays refused: it takes any cwd and argv). It names
//! what the registry (a computer) or the crew store (a crew server) already knows, and hostd starts the user's
//! login shell there with an empty `argv`: the same default every desktop tab gets from `Session::spawn`.
//! The terminal is an ordinary hostd pty with no pane key (a person's shell, not an agent's), so the desktop
//! lists it, and the phone attaches with `pty_attach` and types behind the `term_write` scope and Face ID.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::protocol::{Mode, PtyCreate, Scope};
use crate::pty::Session;
use crate::server::Hub;

/// Where the shell starts. `Err` names why the request is refused; nothing is started.
async fn resolve_cwd(
    hub: &Hub,
    project: Option<&str>,
    worktree: Option<&str>,
    crewmate: Option<&str>,
) -> Result<PathBuf, String> {
    let dir = hub.data_dir();
    if hub.crew_host {
        // A crew server has no project registry: its folders are the crew home and its crewmates' work trees.
        if project.is_some() || worktree.is_some() {
            return Err(
                "a crew server has no projects: open the crew home or a crewmate's work tree"
                    .into(),
            );
        }
        return match crewmate {
            // Ids come from the crew store, so one that is not a stored crewmate (an escape, a path) is unknown.
            Some(id) => {
                let c = hub
                    .crew
                    .store
                    .list_crewmates()
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or_else(|| format!("unknown crewmate {id}"))?;
                crate::crew::project::current_work_dir(&hub.crew.dir, &c)
            }
            None => Ok(atlas_core::paths::home_dir()),
        };
    }
    if crewmate.is_some() {
        return Err("crewmates live on a crew server".into());
    }
    let project = project
        .filter(|p| !p.is_empty())
        .ok_or("name a project to open a terminal in")?;
    match worktree.filter(|w| !w.is_empty()) {
        Some(w) => {
            // A worktree of THIS project's repo, by id (as `agent_launch`): another repo's, or a path, is refused.
            let listed = crate::worktrees::local::list(dir, project).await?;
            let ok = listed["worktrees"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"] == w));
            if !ok {
                return Err(format!("unknown worktree {w} in this project"));
            }
            crate::worktrees::local::dir_of(dir, w)
        }
        None => crate::projects::root_of(dir, project),
    }
}

/// `{pty}` of a new shell in the folder the ids name; audited as `pty_created {node, project, cwd}`.
pub(crate) async fn open(
    hub: &Hub,
    conn_id: u64,
    project: Option<String>,
    worktree: Option<String>,
    crewmate: Option<String>,
) -> Result<Value, String> {
    let cwd = resolve_cwd(
        hub,
        project.as_deref(),
        worktree.as_deref(),
        crewmate.as_deref(),
    )
    .await?;
    let cwd = cwd
        .canonicalize()
        .map_err(|_| format!("{} no longer exists", cwd.display()))?;
    if !cwd.is_dir() {
        return Err(format!("{} no longer exists", cwd.display()));
    }
    // No pane key: no hook reports this terminal as an agent. An empty argv is the user's login shell.
    let req = PtyCreate {
        cwd: cwd.to_string_lossy().into_owned(),
        argv: Vec::new(),
        env: Default::default(),
        cols: 80,
        rows: 24,
        scope: Scope::default(),
        title: None,
        mode: Mode::Pty,
    };
    let scroll = crate::paths::scrollback_dir(&hub.dir);
    let session: Arc<Session> = tokio::task::spawn_blocking(move || Session::spawn(req, &scroll))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    let pty = session.id.clone();
    hub.sessions.lock().insert(pty.clone(), session);
    // Who asked, for which project and where: never what is typed afterwards (`pty_write` is logged as a size).
    if let Some(node) = hub.origin_of(conn_id).strip_prefix("phone:") {
        hub.mobile.audit(
            node,
            "pty_created",
            &json!({"project": project, "worktree": worktree, "crewmate": crewmate, "cwd": cwd}),
        );
    }
    Ok(json!({"pty": pty}))
}
