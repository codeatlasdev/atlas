//! Preparing a task and starting its agent: a cancel during a stalled checkout never starts the agent
//! (#292), a hung git is killed at its deadline and blocks no other crewmate, repos check out
//! concurrently, and the server runs at most N agents at once (#296). The "git" here is a fake
//! script (`project::TEST_GIT_PROGRAM`), the agents are fake binaries, the marker file is what
//! proves an agent was (not) started.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::fixture::{
    Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin, wait_task_state,
};
use crate::HostClient;
use crate::crew::proc::TEST_CALL_DEADLINE;
use crate::crew::project::TEST_GIT_PROGRAM;

/// Holds the process-wide overrides of one test and clears them when the test ends.
struct Overrides {
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Overrides {
    async fn new() -> Self {
        Self {
            _lock: lock_agent_bin().await,
        }
    }
}

impl Drop for Overrides {
    fn drop(&mut self) {
        *TEST_GIT_PROGRAM.lock() = None;
        *TEST_CALL_DEADLINE.lock() = None;
        crate::crew::runner::set_test_agent_bin(None);
        crate::crew::engine::set_test_engine_bin(None);
        *crate::crew::runner::TEST_MAX_AGENTS.lock() = None;
    }
}

/// A fake `git` that records its pid in `<dir>/git.pid` and hangs, with a child of its own
/// (`sleep`), so only a kill of the whole tree leaves nothing behind.
fn hanging_git(dir: &Path) -> PathBuf {
    fake_agent(
        dir,
        "git-hang",
        &format!(
            "echo $$ >> \"{p}\"\nsleep 300 &\necho $! >> \"{p}.child\"\nwait\nexit 0",
            p = dir.join("git.pid").display()
        ),
    )
}

fn pids(file: &Path) -> Vec<u32> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

async fn until(what: &str, within: Duration, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < within, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn alive(pid: u32) -> bool {
    // A killed process may stay a zombie until its parent reaps it: that is gone for our purpose.
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat
            .rsplit(')')
            .next()
            .is_some_and(|rest| rest.trim_start().starts_with('Z')),
        Err(_) => atlas_core::sys::pid_alive(pid),
    }
}

async fn git_project_crewmate(
    client: &HostClient,
    name: &str,
    url: &str,
    engine: Option<&str>,
) -> String {
    let mut args = json!({"name": name, "role": "r", "agent": "claude", "approval": "auto",
                          "project": {"kind": "git", "url": url}});
    if let Some(kind) = engine {
        args["engine"] = json!({"kind": kind});
    }
    let v = client.call("crewmate_create", args).await.unwrap();
    v["id"].as_str().unwrap().to_string()
}

async fn send(client: &HostClient, crewmate: &str, prompt: &str) -> String {
    let v = client
        .call(
            "crewmate_task",
            json!({"id": crewmate, "prompt": prompt, "source": "test"}),
        )
        .await
        .unwrap();
    v["task"]["id"].as_str().unwrap().to_string()
}

async fn task_of(client: &HostClient, crewmate: &str, task: &str) -> Value {
    let v = client
        .call("crewmate_tasks", json!({"id": crewmate, "limit": 50}))
        .await
        .unwrap();
    v["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task)
        .cloned()
        .unwrap_or(Value::Null)
}

async fn runner_idle(client: &HostClient, crewmate: &str) {
    let start = Instant::now();
    loop {
        let list = client.call("crewmate_list", json!({})).await.unwrap();
        let me = list["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == crewmate)
            .cloned()
            .unwrap_or(Value::Null);
        if me["state"] == "idle" {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "the runner never went idle: {me}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// An agent that leaves `marker` behind when it starts, then ends the task.
fn marker_agent(dir: &Path, marker: &Path) -> PathBuf {
    fake_agent(
        dir,
        "claude",
        &Hooks::new()
            .done("ok")
            .script_after(&format!("echo started > \"{}\"", marker.display())),
    )
}

// #292: cancel while the checkout is stalled: the task is cancelled, and when the checkout is
// finally over the agent must never be started (it used to be, and persisted `running` over the
// cancel). Here the stalled git is killed by the cancel itself, the clone fails, and nothing starts.
async fn cancel_during_a_stalled_prepare_never_starts_the_agent(engine: Option<&str>) {
    let _o = Overrides::new().await;
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("agent-started");
    *TEST_GIT_PROGRAM.lock() = Some(hanging_git(tmp.path()).to_string_lossy().into_owned());
    let agent = marker_agent(tmp.path(), &marker);
    use_agent_bin(&agent);
    crate::crew::engine::set_test_engine_bin(Some(&agent.to_string_lossy()));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id =
        git_project_crewmate(&client, "Stalled", "https://example.invalid/r.git", engine).await;

    let task = send(&client, &id, "go").await;
    // Prepare is stalled in git: the task says it is downloading.
    until("git to start", Duration::from_secs(15), || {
        !pids(&tmp.path().join("git.pid")).is_empty()
    })
    .await;
    client
        .call("crewmate_cancel", json!({"id": id, "task": task}))
        .await
        .unwrap();
    runner_idle(&client, &id).await;

    let t = task_of(&client, &id, &task).await;
    assert_eq!(t["state"], "cancelled", "{t}");
    // Give a wrongly started agent time to leave its marker.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !marker.exists(),
        "the agent was started for a task cancelled during prepare"
    );
    let t = task_of(&client, &id, &task).await;
    assert_eq!(t["state"], "cancelled", "still cancelled: {t}");
    client.call("shutdown", json!({})).await.ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_during_a_stalled_prepare_never_starts_the_pty_agent() {
    cancel_during_a_stalled_prepare_never_starts_the_agent(None).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_during_a_stalled_prepare_never_starts_the_conversational_agent() {
    cancel_during_a_stalled_prepare_never_starts_the_agent(Some("claude")).await;
}

// #292: the same when the checkout finishes AFTER the cancel (git does not die on cancel, e.g.
// a slow but healthy clone): the store says cancelled, so the agent must not start.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_cancelled_while_its_checkout_finishes_never_starts_the_agent() {
    for engine in [None, Some("claude")] {
        let _o = Overrides::new().await;
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("agent-started");
        // A git that takes 3 s and then does what the real one would for a missing repo: it
        // creates the checkout dirs so prepare succeeds.
        let slow_git = fake_agent(
            tmp.path(),
            "git-slow",
            "sleep 3\nfor a in \"$@\"; do last=\"$a\"; done\n\
             case \"$1\" in\n  clone) mkdir -p \"$last/.git\";;\n  worktree) mkdir -p \"$7/.git\" 2>/dev/null; for a in \"$@\"; do w=\"$a\"; done; \
             mkdir -p \"$w/.git\" ;;\n  rev-parse) echo origin/main;;\nesac\nexit 0",
        );
        *TEST_GIT_PROGRAM.lock() = Some(slow_git.to_string_lossy().into_owned());
        let agent = marker_agent(tmp.path(), &marker);
        use_agent_bin(&agent);
        crate::crew::engine::set_test_engine_bin(Some(&agent.to_string_lossy()));
        let client = daemon(&tmp.path().join("hostd")).await;
        let id =
            git_project_crewmate(&client, "Slow", "https://example.invalid/r.git", engine).await;

        let task = send(&client, &id, "go").await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        client
            .call("crewmate_cancel", json!({"id": id, "task": task}))
            .await
            .unwrap();
        runner_idle(&client, &id).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(
            !marker.exists(),
            "engine {engine:?}: the agent started for a task cancelled during prepare"
        );
        let t = task_of(&client, &id, &task).await;
        assert_eq!(t["state"], "cancelled", "engine {engine:?}: {t}");
        client.call("shutdown", json!({})).await.ok();
    }
}

// #296: a git that never returns is killed (with its children) at the deadline, the task fails with
// the reason, and a crewmate of another project is not held up by it.
#[tokio::test(flavor = "multi_thread")]
async fn a_hung_git_is_killed_at_the_deadline_and_blocks_no_other_crewmate() {
    let _o = Overrides::new().await;
    let tmp = tempfile::tempdir().unwrap();
    *TEST_GIT_PROGRAM.lock() = Some(hanging_git(tmp.path()).to_string_lossy().into_owned());
    *TEST_CALL_DEADLINE.lock() = Some(Duration::from_secs(2));
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done("ok").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let hung = git_project_crewmate(&client, "Hung", "https://example.invalid/a.git", None).await;
    let hung_task = send(&client, &hung, "go").await;
    until("git to start", Duration::from_secs(15), || {
        !pids(&tmp.path().join("git.pid")).is_empty()
    })
    .await;

    // Another crewmate (no project) runs to the end while the clone hangs.
    let other = create_crewmate(&client, "Other").await;
    let other_task = send(&client, &other, "go").await;
    wait_task_state(
        &client,
        &other,
        &other_task,
        "done",
        Duration::from_secs(15),
    )
    .await;

    // The hung one fails at the deadline, naming git.
    let start = Instant::now();
    let failed = loop {
        let t = task_of(&client, &hung, &hung_task).await;
        if t["state"] == "failed" {
            break t;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the hung git was never given up on: {t}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let err = failed["error"].as_str().unwrap();
    assert!(
        err.contains("timed out"),
        "the error should say git timed out: {err}"
    );
    // Nothing of that git is left, children included.
    let (git, child) = (
        pids(&tmp.path().join("git.pid")),
        pids(&tmp.path().join("git.pid.child")),
    );
    until(
        "the killed git tree to be gone",
        Duration::from_secs(5),
        || git.iter().chain(&child).all(|p| !alive(*p)),
    )
    .await;
    client.call("shutdown", json!({})).await.ok();
}

// #296: the checkout lock is per repo: a clone of repo A that hangs does not hold up the checkout of
// repo B (it used to be one global mutex for all clones).
#[tokio::test(flavor = "multi_thread")]
async fn two_projects_on_different_repos_prepare_concurrently() {
    let _o = Overrides::new().await;
    let tmp = tempfile::tempdir().unwrap();
    // Hangs for repo `a`, completes at once for anything else.
    let git = fake_agent(
        tmp.path(),
        "git-by-repo",
        &format!(
            "case \"$*\" in\n  *repo-a*) echo $$ >> \"{p}\"; sleep 300 & wait;;\nesac\n\
             for a in \"$@\"; do last=\"$a\"; done\n\
             case \"$1\" in\n  clone) mkdir -p \"$last/.git\";;\n  worktree) for a in \"$@\"; do w=\"$a\"; done; mkdir -p \"$w/.git\";;\n  rev-parse) echo origin/main;;\nesac\nexit 0",
            p = tmp.path().join("git.pid").display()
        ),
    );
    *TEST_GIT_PROGRAM.lock() = Some(git.to_string_lossy().into_owned());
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().done("ok").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let a = git_project_crewmate(&client, "A", "https://example.invalid/repo-a.git", None).await;
    send(&client, &a, "go").await;
    until("repo a's git to hang", Duration::from_secs(15), || {
        !pids(&tmp.path().join("git.pid")).is_empty()
    })
    .await;

    let b = git_project_crewmate(&client, "B", "https://example.invalid/repo-b.git", None).await;
    let b_task = send(&client, &b, "go").await;
    wait_task_state(&client, &b, &b_task, "done", Duration::from_secs(20)).await;
    client.call("shutdown", json!({})).await.ok();
}

/// An agent that records, while it runs, how many agents run: `<dir>/live/<pid>` exists for its
/// whole life, and the highest count seen is appended to `<dir>/peak`.
fn counting_agent(dir: &Path, hold_secs: u32) -> PathBuf {
    let live = dir.join("live");
    std::fs::create_dir_all(&live).unwrap();
    fake_agent(
        dir,
        "claude",
        &Hooks::new().done("ok").script_after(&format!(
            "touch \"{live}/$$\"\nn=$(ls \"{live}\" | wc -l)\necho $n >> \"{peak}\"\nsleep {hold_secs}\nrm -f \"{live}/$$\"",
            live = live.display(),
            peak = dir.join("peak").display(),
        )),
    )
}

fn peak(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join("peak"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .max()
        .unwrap_or(0)
}

// #296: limit 2 with 4 crewmates queued: never more than 2 live agents, the other two wait with a
// visible "server busy" and then run.
#[tokio::test(flavor = "multi_thread")]
async fn a_limit_of_two_runs_at_most_two_agents_and_the_rest_show_server_busy() {
    let _o = Overrides::new().await;
    let tmp = tempfile::tempdir().unwrap();
    *crate::crew::runner::TEST_MAX_AGENTS.lock() = Some(2);
    use_agent_bin(&counting_agent(tmp.path(), 3));
    let client = daemon(&tmp.path().join("hostd")).await;
    let mut tasks = vec![];
    for i in 0..4 {
        let id = create_crewmate(&client, &format!("C{i}")).await;
        let t = send(&client, &id, "go").await;
        tasks.push((id, t));
    }

    // While two run, the other two are queued and say why.
    let start = Instant::now();
    loop {
        let mut busy = 0;
        for (id, t) in &tasks {
            let task = task_of(&client, id, t).await;
            if task["state"] == "queued" && task["waiting_for"] == "server busy" {
                busy += 1;
            }
        }
        if busy == 2 {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "two tasks should wait as 'server busy' while two agents run (saw {busy})"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for (id, t) in &tasks {
        wait_task_state(&client, id, t, "done", Duration::from_secs(40)).await;
    }
    assert_eq!(peak(tmp.path()), 2, "more than 2 agents ran at once");
    client.call("shutdown", json!({})).await.ok();
}

// #296: the limit is a crew setting (`crew_settings_set.max_agents`), read back by
// `crew_settings_get`, and saving it keeps what else is in crew.json.
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_limit_is_a_crew_setting() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let before = client.call("crew_settings_get", json!({})).await.unwrap();
    assert!(
        before["max_agents"].as_u64().unwrap() >= 1,
        "a default limit: {before}"
    );
    client
        .call(
            "crew_settings_set",
            json!({"short_replies": true, "max_agents": 3}),
        )
        .await
        .unwrap();
    let after = client.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(after["max_agents"], 3, "{after}");
    // An old client that sends no limit leaves it alone.
    client
        .call("crew_settings_set", json!({"short_replies": false}))
        .await
        .unwrap();
    let kept = client.call("crew_settings_get", json!({})).await.unwrap();
    assert_eq!(kept["max_agents"], 3, "{kept}");
    // Zero would stop every task: refused.
    let zero = client
        .call(
            "crew_settings_set",
            json!({"short_replies": true, "max_agents": 0}),
        )
        .await;
    assert!(zero.is_err(), "a limit of 0 must be refused");
    client.call("shutdown", json!({})).await.ok();
}

// #296: a task that waits for the user (an action card) holds no agent slot: with a limit of 1, a
// second crewmate runs while the first waits for an answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_waiting_for_the_user_does_not_hold_an_agent_slot() {
    let _o = Overrides::new().await;
    let tmp = tempfile::tempdir().unwrap();
    *crate::crew::runner::TEST_MAX_AGENTS.lock() = Some(1);
    let ask = "which?\nATLAS_ACTION: {\"kind\":\"choose\",\"question\":\"which?\",\"options\":[\"a\",\"b\"]}";
    // The first agent asks, the second ends at once: a file tells them apart.
    let first = tmp.path().join("first");
    let asker = Hooks::new()
        .done(ask)
        .script_after(&format!("touch \"{}\"", first.display()));
    let quick = Hooks::new().done("ok").script();
    let bin = fake_agent(
        tmp.path(),
        "claude",
        &format!(
            "if [ ! -f \"{first}\" ]; then\n{asker}\nfi\n{quick}",
            first = first.display()
        ),
    );
    use_agent_bin(&bin);
    let client = daemon(&tmp.path().join("hostd")).await;
    let a = create_crewmate(&client, "Asker").await;
    let a_task = send(&client, &a, "go").await;
    wait_task_state(
        &client,
        &a,
        &a_task,
        "waiting_action",
        Duration::from_secs(20),
    )
    .await;

    let b = create_crewmate(&client, "Runner").await;
    let b_task = send(&client, &b, "go").await;
    wait_task_state(&client, &b, &b_task, "done", Duration::from_secs(20)).await;
    client.call("shutdown", json!({})).await.ok();
}
