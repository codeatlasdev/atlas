//! What a task runs with: the working directory, the instructions every agent gets, the env files
//! an agent is started with and the hook endpoint it reports to.

use std::collections::BTreeMap;
use std::path::Path;

use super::action::park_task;
use super::github::{WaitResult, needs_github_auth, wait_for_github_connect};
use super::outcome::{broadcast_task, finish_task, persist};
use crate::crew::store::{ProjectRef, Task, TaskAction};
use crate::crew::{Crewmate, project, triggers};
use crate::server::Hub;

/// What a task needs before its agent can start.
pub(super) struct Prepared {
    /// Where the agent works.
    pub work_dir: String,
    /// The full instructions, task included.
    pub prompt: String,
}

/// Everything both runners do before the agent starts: wait for GitHub when the project needs a
/// login the crew server lacks, check the project out, build the instructions.
///
/// `None`: the task ended here (cancelled while waiting, timed out, or the project could not be
/// prepared). It is recorded and announced already, and the crewmate's triggers were told.
pub(super) async fn prepare_task(
    hub: &Hub,
    crewmate: &Crewmate,
    task: &mut Task,
) -> Option<Prepared> {
    // Pre-task: if this is a GitHub project and gh is not logged in, park and wait.
    if let Some(reason) = needs_github_auth(crewmate).await
        && let Some(hub_arc) = hub.crew.hub_arc()
    {
        park_task(hub, crewmate, task, TaskAction::ConnectGithub { reason });
        match wait_for_github_connect(&hub_arc, &crewmate.id, &task.id).await {
            WaitResult::Connected => task.action = None,
            WaitResult::Cancelled => {
                triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
                return None;
            }
            WaitResult::Timeout => {
                finish_task(
                    hub,
                    crewmate,
                    task,
                    Err("timed out waiting for GitHub connection".into()),
                    vec![],
                );
                triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
                return None;
            }
        }
    }

    // After the GitHub wait: a private clone only works once gh is logged in.
    let work_dir = match prepare_work_dir(hub, crewmate, task).await {
        Ok(d) => d,
        Err(e) => {
            finish_task(hub, crewmate, task, Err(e), vec![]);
            triggers::on_task_finished(hub, &crewmate.id, &task.id, task.state);
            return None;
        }
    };
    let memory = hub.crew.store.memory_path(&crewmate.id);
    let prompt = task_prompt(
        crewmate,
        &work_dir,
        &memory,
        &task.prompt,
        task.tz.as_deref(),
    );
    Some(Prepared { work_dir, prompt })
}

/// Where the crewmate's agent works: its project (cloned or fetched now, so after any GitHub login it waited for), else its
/// own folder. Both runners use this; the terminal one used to skip it and start every agent in hostd's directory.
async fn prepare_work_dir(
    hub: &Hub,
    crewmate: &Crewmate,
    task: &mut Task,
) -> Result<String, String> {
    let dir = if let Some(proj) = &crewmate.project {
        // Not checked out yet (a first task, or one sent while the background clone runs): say so on the task.
        let work = project::work_dir(&hub.crew.dir, &crewmate.id);
        if let ProjectRef::Git { url, .. } = proj
            && !work.join(".git").exists()
        {
            task.preparing = Some(
                url.trim_end_matches(".git")
                    .trim_start_matches("https://github.com/")
                    .to_string(),
            );
            let _ = persist(hub, &crewmate.id, task);
            broadcast_task(hub, &crewmate.id, task);
        }
        let p = project::prepare(proj, &crewmate.id, &hub.crew.dir).await;
        if task.preparing.take().is_some() {
            let _ = persist(hub, &crewmate.id, task);
            broadcast_task(hub, &crewmate.id, task);
        }
        let p = p.map_err(|e| format!("project prepare: {e}"))?;
        let _ = project::fetch(&p).await; // non-fatal
        p.to_string_lossy().to_string()
    } else {
        crewmate.workdir.clone().unwrap_or_else(|| {
            project::work_dir(&hub.crew.dir, &crewmate.id)
                .to_string_lossy()
                .to_string()
        })
    };
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir)
}

/// The instructions every agent gets. It says where it runs: the crewmate lives on its own server, so the user's computer
/// (its paths, mounts, branches) is out of reach; without this an agent kept asking to "mount the Mac folder".
fn task_prompt(
    crewmate: &Crewmate,
    work_dir: &str,
    memory: &Path,
    prompt: &str,
    tz: Option<&str>,
) -> String {
    let project = match &crewmate.project {
        Some(ProjectRef::Git { url, branch }) => format!(
            "Your project is {url}{b}, already checked out in {work_dir}, your working directory. Work there.\n",
            b = branch
                .as_deref()
                .map(|b| format!(" (branch {b})"))
                .unwrap_or_default(),
        ),
        Some(ProjectRef::Path { .. }) => {
            format!("Your project is in {work_dir}, your working directory. Work there.\n")
        }
        None => format!("Your working directory is {work_dir}.\n"),
    };
    let tz_line = if let Some(tz_str) = tz {
        if let Ok(zone) = tz_str.parse::<chrono_tz::Tz>() {
            let local = chrono::Utc::now().with_timezone(&zone);
            format!(
                "The user's timezone is {tz_str}. Current local time there: {}.\n",
                local.format("%Y-%m-%d %H:%M %Z")
            )
        } else {
            format!("The user's timezone is {tz_str}.\n")
        }
    } else {
        String::new()
    };
    format!(
        "You are {name}. {role}\n\
         You run on your own server, not on the user's computer. You cannot see the user's machine, its files or \
         what is checked out there, and you never ask for it to be mounted or shared.\n\
         {project}\
         Your memory file is {mem}. Read it first. When you learn something durable \
         about this work or this user, append it there.\n\
         {tz_line}\
         Never use emojis in replies.\n\
         If you need the user to do something (connect to GitHub, provide a secret, answer a \
         question, make a choice, sign in to a service, or set up an automation), do not refuse \
         and do not ask for keys or tokens in prose. Instead, end your turn with exactly one line:\n\
         ATLAS_ACTION: {{\"kind\":\"<kind>\", ...}}\n\
         Supported kinds: connect_github {{reason}}, provide_secret {{name, label, reason}}, \
         input {{label, reason, placeholder?}}, choose {{question, options:[...]}}, \
         sign_in_engine {{engine}}, \
         add_trigger {{trigger: {{type, ...}}, prompt, reason}} \
         (trigger types: schedule {{cron}}, interval {{every_secs}}, github {{repo, events}}, \
         file {{path, glob?}}, crewmate {{from_crewmate, on}}, service {{project_dir, service, on}}; \
         cron is in the user's local time when tz is known).\n\
         Task: {prompt}",
        name = crewmate.name,
        role = crewmate.role,
        mem = memory.display(),
    )
}

/// The per-agent env overrides of `<dir>/agent-env/<agent>.env` (`KEY=VALUE` lines; `#` comments and
/// blank lines skipped, surrounding quotes of a value dropped). The file the provider setup in the
/// desktop writes: a custom provider key reaches every engine and the terminal agent through it.
pub(super) fn load_agent_env(dir: &Path, agent: &str) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let Ok(data) = std::fs::read_to_string(dir.join("agent-env").join(format!("{agent}.env")))
    else {
        return env;
    };
    for line in data.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            env.insert(k.trim().to_string(), v.trim_matches('"').to_string());
        }
    }
    env
}

/// The hook endpoint the desktop wrote (`ATLAS_HOOK_PORT`, `ATLAS_HOOK_TOKEN`), `(0, "")` when absent.
pub(super) fn load_hook_endpoint(dir: &Path) -> (u16, String) {
    let env_path = dir.join("desktop").join("hook-endpoint.env");
    let content = std::fs::read_to_string(env_path).unwrap_or_default();
    let mut port = 0u16;
    let mut token = String::new();
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("ATLAS_HOOK_PORT=") {
            port = v.parse().unwrap_or(0);
        }
        if let Some(v) = line.strip_prefix("ATLAS_HOOK_TOKEN=") {
            token = v.to_string();
        }
    }
    (port, token)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The provider setup writes `KEY=VALUE` lines; a custom provider key must reach the agent.
    #[test]
    fn agent_env_file_is_parsed_into_the_agents_env() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("agent-env")).unwrap();
        std::fs::write(
            dir.path().join("agent-env/omp.env"),
            "OPENROUTER_API_KEY=sk-test\n# comment\nFOO=bar\n  QUOTED=\"v\"\n\n",
        )
        .unwrap();

        let env = load_agent_env(dir.path(), "omp");

        assert_eq!(env["OPENROUTER_API_KEY"], "sk-test");
        assert_eq!(env["FOO"], "bar");
        assert_eq!(env["QUOTED"], "v", "surrounding quotes are dropped");
        assert!(!env.contains_key("# comment"));
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn a_missing_agent_env_file_is_an_empty_env() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_agent_env(dir.path(), "claude").is_empty());
    }

    #[test]
    fn the_hook_endpoint_is_read_and_defaults_to_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_hook_endpoint(dir.path()), (0, String::new()));
        std::fs::create_dir_all(dir.path().join("desktop")).unwrap();
        std::fs::write(
            dir.path().join("desktop/hook-endpoint.env"),
            "ATLAS_HOOK_PORT=4242\nATLAS_HOOK_TOKEN=tok\n",
        )
        .unwrap();
        assert_eq!(load_hook_endpoint(dir.path()), (4242, "tok".to_string()));
    }
}
