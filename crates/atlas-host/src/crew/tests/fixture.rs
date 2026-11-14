//! What every crew test shares: a real hostd, fake agent binaries, and a builder for the shell a
//! fake agent runs to talk back to hostd through the hook endpoint (what `claude` does for real).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::HostClient;
use crate::server::ServeOpts;

/// A crew-server hostd on `dir`, ready to take commands. The daemon dies with the test runtime.
pub(super) async fn daemon(dir: &Path) -> HostClient {
    let d = dir.to_path_buf();
    tokio::spawn(async move {
        let _ = crate::server::serve_with(
            &d,
            ServeOpts {
                crew_host: true,
                ..Default::default()
            },
        )
        .await;
    });
    for _ in 0..200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start in 10 s");
}

/// Write `script` as an executable `<dir>/bin/<name>` (a `#!/bin/sh` is prepended) and return its path.
pub(super) fn fake_agent(dir: &Path, name: &str, script: &str) -> PathBuf {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let path = bin_dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Hold for a whole test that overrides the agent binary (`runner::set_test_agent_bin`): the
/// override is process-wide, so tests that set it must not overlap.
pub(super) async fn lock_agent_bin() -> tokio::sync::MutexGuard<'static, ()> {
    crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// Make the next tasks run `bin` instead of the real agent. Pair with [`lock_agent_bin`] and clear
/// with `set_test_agent_bin(None)` at the end.
pub(super) fn use_agent_bin(bin: &Path) {
    crate::crew::runner::set_test_agent_bin(Some(&bin.to_string_lossy()));
}

/// Create a crewmate that runs `claude` without approvals and return its id.
pub(super) async fn create_crewmate(client: &HostClient, name: &str) -> String {
    let v = client
        .call(
            "crewmate_create",
            json!({"name": name, "role": "r", "agent": "claude", "approval": "auto"}),
        )
        .await
        .expect("crewmate_create failed");
    v["id"].as_str().expect("no id").to_string()
}

/// Wait up to `timeout` for a task on `crewmate_id` to reach `state`. Polls every 250 ms.
/// A task that ends in another final state, or never gets there, fails with the task as hostd
/// last reported it (state, result, error): a timeout alone said nothing about the one CI
/// failure of `crewmate_list_last_and_queued` (PR #106, macOS runner).
pub(super) async fn wait_task_state(
    client: &HostClient,
    crewmate_id: &str,
    task_id: &str,
    state: &str,
    timeout: Duration,
) -> Value {
    let start = std::time::Instant::now();
    let mut last = Value::Null;
    loop {
        let v = client
            .call("crewmate_tasks", json!({"id": crewmate_id, "limit": 20}))
            .await
            .expect("crewmate_tasks failed");
        if let Some(t) = v["tasks"]
            .as_array()
            .and_then(|tasks| tasks.iter().find(|t| t["id"] == task_id))
        {
            let now = t["state"].as_str().unwrap_or("");
            if now == state {
                return t.clone();
            }
            if matches!(now, "done" | "failed" | "cancelled") {
                panic!("task {task_id} ended '{now}', expected '{state}': {t}");
            }
            last = t.clone();
        }
        if start.elapsed() > timeout {
            panic!(
                "task {task_id} did not reach state '{state}' within {timeout:?}; last seen: {last}"
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Wait up to `timeout` for `crewmate_list` to return the crewmate with at least one trigger.
pub(super) async fn wait_crewmate_triggers(
    client: &HostClient,
    crewmate_id: &str,
    timeout: Duration,
) -> Value {
    let start = std::time::Instant::now();
    loop {
        let v = client.call("crewmate_list", json!({})).await.unwrap();
        if let Some(c) = v["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == crewmate_id)
            && c["triggers"].as_array().is_some_and(|t| !t.is_empty())
        {
            return c.clone();
        }
        if start.elapsed() > timeout {
            panic!("crewmate triggers never appeared");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The shell of a fake `claude`: a sequence of hook posts to hostd, then exit. Replaces the curl
/// scripts every test used to copy. Payloads are built with serde, so no quoting by hand.
///
/// ```ignore
/// let bin = fake_agent(tmp.path(), "claude", &Hooks::new().tool("bash", json!({"command": "ls"})).done("ok").script());
/// ```
#[derive(Default)]
pub(super) struct Hooks {
    sh: String,
}

impl Hooks {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Wait `secs` before the next step.
    pub(super) fn sleep(mut self, secs: u32) -> Self {
        self.sh.push_str(&format!("sleep {secs}\n"));
        self
    }

    /// A `pre_tool_use` hook: the agent is about to run `tool`.
    pub(super) fn tool(self, tool: &str, input: Value) -> Self {
        self.post(json!({"event": "pre_tool_use", "tool_name": tool, "tool_input": input}))
    }

    /// The agent's last message: the task ends `done` with `text` as its result.
    pub(super) fn done(self, text: &str) -> Self {
        self.post(json!({"event": "agent_end", "will_continue": false, "text": text}))
    }

    /// One hook post with `payload` for this task's pane.
    pub(super) fn post(mut self, payload: Value) -> Self {
        // Single-quoted in the shell (`'` closed, escaped and reopened): the payload is never expanded.
        let quoted = payload.to_string().replace('\'', "'\\''");
        self.sh.push_str(&format!(
            "PAYLOAD='{quoted}'\n\
             curl -s -X POST \"http://127.0.0.1:${{ATLAS_HOOK_PORT}}/hook/claude\" \\\n  \
             -H \"X-Atlas-Hook-Token: ${{ATLAS_HOOK_TOKEN}}\" -H \"Content-Type: application/json\" \\\n  \
             -d \"{{\\\"pane_key\\\":\\\"${{ATLAS_PANE_KEY}}\\\",\\\"payload\\\":$PAYLOAD}}\" || true\n"
        ));
        self
    }

    /// The whole script: waits for the hook endpoint file, runs the steps, exits 0.
    pub(super) fn script(&self) -> String {
        format!(
            "if [ -f \"$ATLAS_HOOK_ENDPOINT\" ]; then\n. \"$ATLAS_HOOK_ENDPOINT\"\n{}fi\nexit 0",
            self.sh
        )
    }
}
