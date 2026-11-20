//! Permission modes per crewmate (#642), through a real hostd: hostd decides every tool call from the rails, the
//! crewmate's current mode and the task's grants, and a person decides the rest.
//!
//! claude's calls cross the real MCP `approve` tool (`atlas:approve`, `mcp::call_for_test`) and the hook route;
//! omp's cross the REAL generated approval extension run by a fake omp (`fake_omp.mjs`). What a person answers
//! goes in as `agent_approve`, a switch as `crewmate_set_approval`, exactly as the clients send them.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use atlas_core::progress::Coded;
use serde_json::{Value, json};

use super::fixture::{daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::HostClient;

const HANG: Duration = atlas_core::scaling::HANG;

fn have_node() -> bool {
    let ok = Command::new("node").arg("--version").output().is_ok();
    if !ok {
        eprintln!("SKIP: node is not installed, the fake omp cannot run the real extension");
    }
    ok
}

/// A hostd with one crewmate of `agent` (the engine of the same name) in `mode`, working in `work`.
struct Rig {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
    work: PathBuf,
    client: HostClient,
    id: String,
}

async fn rig(agent: &str, mode: &str) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let client = daemon(&dir).await;
    let v = client
        .call(
            "crewmate_create",
            json!({"name": "Pat", "role": "r", "agent": agent, "mode": mode,
                   "engine": {"kind": agent}, "workdir": work}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    Rig {
        _tmp: tmp,
        dir,
        work,
        client,
        id,
    }
}

impl Rig {
    fn pane(&self, task: &str) -> String {
        format!("crew:{}:{task}", self.id)
    }

    /// A path inside the crewmate's work tree.
    fn tree(&self, rel: &str) -> String {
        self.work.join(rel).to_string_lossy().into_owned()
    }

    /// What claude's `atlas:approve` tool says for this call: `{"behavior": "allow"}` or a deny with its message.
    fn approve(&self, task: &str, tool: &str, input: Value) -> tokio::task::JoinHandle<Value> {
        let (dir, id, pane) = (self.dir.clone(), self.id.clone(), self.pane(task));
        let args = json!({"tool_name": tool, "tool_input": input});
        tokio::spawn(async move {
            let r = crate::crew::mcp::call_for_test(&dir, &id, &pane, "approve", args)
                .await
                .unwrap();
            serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
        })
    }

    /// The call is decided at once (allowed by the mode or a grant), not held.
    async fn decided(&self, task: &str, tool: &str, input: Value) -> Value {
        tokio::time::timeout(Duration::from_secs(10), self.approve(task, tool, input))
            .await
            .expect("the call was held for a person")
            .unwrap()
    }

    /// The call is held: it is still waiting after a moment, and is listed as a pending approval.
    async fn held(
        &self,
        task: &str,
        tool: &str,
        input: Value,
    ) -> (tokio::task::JoinHandle<Value>, Value) {
        let h = self.approve(task, tool, input.clone());
        // This call's own prompt: other calls may be waiting too.
        let start = std::time::Instant::now();
        let ev = loop {
            let mine = self.pending().await.into_iter().find(|p| {
                p["tool_name"] == tool
                    && p["tool_input"] == input
                    && p["pane_key"] == self.pane(task)
            });
            if let Some(p) = mine {
                break p;
            }
            assert!(
                start.elapsed() < HANG,
                "{tool} {input} never asked for an approval"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(
            !h.is_finished(),
            "the call was decided without a person: {ev}"
        );
        (h, ev)
    }

    async fn pending(&self) -> Vec<Value> {
        let st = self
            .client
            .call("agent_status_all", json!({}))
            .await
            .unwrap();
        st["pending_approvals"].as_array().unwrap().clone()
    }

    async fn wait_pending(&self) -> Value {
        let start = std::time::Instant::now();
        loop {
            if let Some(p) = self.pending().await.into_iter().next() {
                return p;
            }
            assert!(start.elapsed() < HANG, "nothing asked for an approval");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Answer a prompt as a person does (`agent_approve`).
    async fn answer(&self, ev: &Value, allow: bool, scope: Option<&str>, comment: Option<&str>) {
        let mut args = json!({"pane": ev["pane_key"], "req": ev["req"], "allow": allow});
        if let Some(s) = scope {
            args["scope"] = json!(s);
        }
        if let Some(c) = comment {
            args["comment"] = json!(c);
        }
        self.client.call("agent_approve", args).await.unwrap();
    }

    async fn set_mode(&self, mode: &str) -> anyhow::Result<Value> {
        self.client
            .call(
                "crewmate_set_approval",
                json!({"id": self.id, "mode": mode}),
            )
            .await
    }

    /// The crewmate's audit entries of `kind`, newest first.
    async fn audit(&self, kind: &str) -> Vec<Value> {
        let v = self
            .client
            .call(
                "crewmate_audit",
                json!({"id": self.id, "kind": kind, "limit": 100}),
            )
            .await
            .unwrap();
        v["entries"].as_array().unwrap().clone()
    }

    async fn listed(&self) -> Value {
        let v = self.client.call("crewmate_list", json!({})).await.unwrap();
        v["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == self.id)
            .unwrap()
            .clone()
    }
}

fn code_of(e: &anyhow::Error) -> String {
    e.chain()
        .find_map(|c| c.downcast_ref::<Coded>())
        .map(|c| c.code.to_string())
        .unwrap_or_else(|| format!("not coded: {e:#}"))
}

// ── the three modes, on claude ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn in_ask_mode_every_call_waits_for_a_person_and_the_answer_is_audited() {
    let rig = rig("claude", "ask").await;
    let (call, ev) = rig
        .held("t1", "Bash", json!({"command": "cargo test"}))
        .await;
    assert_eq!(ev["tool_name"], "Bash");
    assert!(ev.get("rail").is_none(), "no rail made it wait: {ev}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    rig.answer(&ev, true, None, None).await;
    assert_eq!(call.await.unwrap(), json!({"behavior": "allow"}));

    let audit = rig.audit("approval_decision").await;
    assert_eq!(audit.len(), 1, "{audit:?}");
    let d = &audit[0];
    assert_eq!(d["tool"], "Bash");
    assert_eq!(d["decision"], "allow");
    assert_eq!(d["source"], "person");
    assert_eq!(d["device"], "desktop");
    assert_eq!(d["tool_input"]["command"], "cargo test");
    assert!(
        d["wait_ms"].as_u64().unwrap() >= 250,
        "the wait is measured: {d}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn in_accept_edits_mode_an_edit_in_the_tree_runs_and_a_command_or_an_edit_elsewhere_waits() {
    let rig = rig("claude", "edits").await;
    let ok = rig
        .decided("t1", "Edit", json!({"file_path": rig.tree("src/a.rs")}))
        .await;
    assert_eq!(ok, json!({"behavior": "allow"}));
    let d = &rig.audit("approval_decision").await[0];
    assert_eq!(
        (
            d["decision"].as_str(),
            d["source"].as_str(),
            d.get("device")
        ),
        (Some("allow"), Some("mode"), None),
        "{d}"
    );

    let (cmd, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
    rig.answer(&ev, false, None, None).await;
    assert_eq!(cmd.await.unwrap()["behavior"], "deny");

    let (outside, ev) = rig
        .held("t1", "Write", json!({"file_path": "/etc/hosts"}))
        .await;
    rig.answer(&ev, false, None, None).await;
    assert_eq!(outside.await.unwrap()["behavior"], "deny");
    let (up, ev) = rig
        .held(
            "t1",
            "Write",
            json!({"file_path": rig.tree("../escape.txt")}),
        )
        .await;
    rig.answer(&ev, false, None, None).await;
    assert_eq!(
        up.await.unwrap()["behavior"],
        "deny",
        "`..` is not inside the tree"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn in_bypass_mode_a_command_runs_but_git_config_and_dotenv_still_wait() {
    let rig = rig("claude", "bypass").await;
    assert_eq!(
        rig.decided("t1", "Bash", json!({"command": "cargo build"}))
            .await,
        json!({"behavior": "allow"})
    );
    for (tool, input) in [
        ("Write", json!({"file_path": rig.tree(".git/config")})),
        ("Edit", json!({"file_path": rig.tree("app/.env")})),
        ("Bash", json!({"command": "echo KEY=1 >> .env"})),
    ] {
        let (call, ev) = rig.held("t1", tool, input).await;
        assert_eq!(ev["rail"], "protected_path", "{ev}");
        rig.answer(&ev, false, None, None).await;
        assert_eq!(call.await.unwrap()["behavior"], "deny", "{tool}");
    }
    let audit = rig.audit("approval_decision").await;
    let sources: Vec<_> = audit
        .iter()
        .rev()
        .map(|d| d["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["mode", "person", "person", "person"], "{audit:?}");
    assert!(audit[0]["rail"] == "protected_path", "{audit:?}");
}

// ── switching ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn switching_while_a_call_waits_decides_it_at_once_and_the_audit_says_why() {
    let rig = rig("claude", "ask").await;
    let (call, _) = rig.held("t1", "Bash", json!({"command": "make"})).await;
    // A protected call waiting too: bypass does not let it through.
    let (env_call, env_ev) = rig
        .held("t1", "Write", json!({"file_path": rig.tree(".env")}))
        .await;

    let v = rig.set_mode("bypass").await.unwrap();
    assert_eq!(
        (v["mode"].as_str(), v["approval"].as_str()),
        (Some("bypass"), Some("auto"))
    );
    let allowed = tokio::time::timeout(Duration::from_secs(10), call)
        .await
        .expect("the waiting call was not decided by the switch")
        .unwrap();
    assert_eq!(allowed, json!({"behavior": "allow"}));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!env_call.is_finished(), "a rail holds in every mode");

    let decisions = rig.audit("approval_decision").await;
    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert_eq!(
        (
            decisions[0]["source"].as_str(),
            decisions[0]["decision"].as_str(),
            decisions[0]["device"].as_str()
        ),
        (Some("mode"), Some("allow"), Some("desktop"))
    );
    let changed = rig.audit("approval_mode_changed").await;
    assert_eq!(changed.len(), 1);
    assert_eq!(
        (
            changed[0]["from"].as_str(),
            changed[0]["to"].as_str(),
            changed[0]["device"].as_str()
        ),
        (Some("ask"), Some("bypass"), Some("desktop"))
    );
    rig.answer(&env_ev, false, None, None).await;
    env_call.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_from_a_task_of_github_or_a_webhook_still_asks_in_bypass() {
    let rig = rig("claude", "bypass").await;
    let mut t = crate::crew::store::Task::new("github", "fix the bug");
    t.id = "t-gh".into();
    t.no_auto = true;
    crate::crew::store::CrewStore::new(&rig.dir)
        .append_task(&rig.id, &t)
        .unwrap();
    // The same call, from an ordinary task, runs.
    assert_eq!(
        rig.decided("t-plain", "Bash", json!({"command": "ls"}))
            .await,
        json!({"behavior": "allow"})
    );
    let (call, ev) = rig.held("t-gh", "Bash", json!({"command": "ls"})).await;
    assert_eq!(ev["rail"], "no_auto", "{ev}");
    rig.answer(&ev, true, None, None).await;
    assert_eq!(call.await.unwrap(), json!({"behavior": "allow"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_bypass_on_the_crew_bypass_is_refused_and_crewmates_in_it_go_back_to_ask() {
    let rig = rig("claude", "bypass").await;
    rig.client
        .call(
            "crew_settings_set",
            json!({"short_replies": true, "allow_bypass": false}),
        )
        .await
        .unwrap();
    let settings = rig
        .client
        .call("crew_settings_get", json!({}))
        .await
        .unwrap();
    assert_eq!(settings["allow_bypass"], false);

    let e = rig.set_mode("bypass").await.unwrap_err();
    assert_eq!(code_of(&e), "crew.approval.bypass_disabled");
    let e = rig
        .client
        .call(
            "crewmate_create",
            json!({"name": "B", "role": "r", "agent": "claude", "approval": "auto", "engine": {"kind": "claude"}}),
        )
        .await
        .unwrap_err();
    assert!(format!("{e:#}").contains("bypass is not allowed"), "{e:#}");
    assert_eq!(rig.set_mode("edits").await.unwrap()["mode"], "edits");

    // The crewmate that was in bypass is in ask now (audited), and its calls wait.
    let before = rig.audit("approval_mode_changed").await;
    assert_eq!(
        (before[1]["from"].as_str(), before[1]["to"].as_str()),
        (Some("bypass"), Some("ask")),
        "{before:?}"
    );
    rig.set_mode("ask").await.unwrap();
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
    rig.answer(&ev, false, None, None).await;
    call.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_crewmate_starts_in_the_crews_default_mode() {
    let rig = rig("claude", "ask").await;
    let made = |extra: Value| {
        let mut args =
            json!({"name": "N", "role": "r", "agent": "claude", "engine": {"kind": "claude"}});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        rig.client.call("crewmate_create", args)
    };
    assert_eq!(
        made(json!({})).await.unwrap()["mode"],
        "ask",
        "the default default"
    );
    rig.client
        .call(
            "crew_settings_set",
            json!({"short_replies": true, "default_approval": "edits"}),
        )
        .await
        .unwrap();
    let v = made(json!({})).await.unwrap();
    assert_eq!(
        (v["mode"].as_str(), v["approval"].as_str()),
        (Some("edits"), Some("ask"))
    );
    // Named, `mode` and the legacy `approval` both work.
    assert_eq!(
        made(json!({"approval": "auto"})).await.unwrap()["mode"],
        "bypass"
    );
    assert_eq!(
        made(json!({"mode": "ask", "approval": "auto"}))
            .await
            .unwrap()["mode"],
        "ask"
    );
    // An engine that cannot ask starts in bypass: that is what it does.
    let c = rig
        .client
        .call(
            "crewmate_create",
            json!({"name": "Cx", "role": "r", "agent": "codex", "engine": {"kind": "codex"}}),
        )
        .await
        .unwrap();
    assert_eq!(c["mode"], "bypass");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_owner_switches_a_crewmate_never_a_crewmates_own_process() {
    let rig = rig("claude", "ask").await;
    // An atlas process inside the crewmate's agent names itself `crewmate:<id>` (HostClient, from its env).
    let crewmate = HostClient::connect_dir(&rig.dir, &format!("crewmate:{}", rig.id))
        .await
        .unwrap();
    let e = crewmate
        .call(
            "crewmate_set_approval",
            json!({"id": rig.id, "mode": "bypass"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code_of(&e), "crew.approval.forbidden", "{e:#}");
    assert_eq!(rig.listed().await["mode"], "ask", "nothing changed");
    assert!(rig.audit("approval_mode_changed").await.is_empty());
    // The owner's client can.
    assert_eq!(rig.set_mode("bypass").await.unwrap()["mode"], "bypass");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_that_cannot_ask_has_no_modes_to_switch_to() {
    let rig = rig("claude", "ask").await;
    let c = rig
        .client
        .call(
            "crewmate_create",
            json!({"name": "Oc", "role": "r", "agent": "opencode", "engine": {"kind": "opencode"}}),
        )
        .await
        .unwrap();
    let id = c["id"].as_str().unwrap();
    let e = rig
        .client
        .call("crewmate_set_approval", json!({"id": id, "mode": "ask"}))
        .await
        .unwrap_err();
    assert_eq!(code_of(&e), "crew.approval.unsupported_engine");
    let listed = rig.client.call("crewmate_list", json!({})).await.unwrap();
    let oc = listed["crewmates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    assert_eq!(oc["approval_supported"], false, "{oc}");
    // The ones that can ask say so.
    assert_eq!(rig.listed().await["approval_supported"], true);
}

// ── scope and comments ───────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn allow_for_this_task_covers_the_same_call_for_that_task_only() {
    let rig = rig("claude", "ask").await;
    let input = json!({"command": "cargo test"});
    let (call, ev) = rig.held("t1", "Bash", input.clone()).await;
    rig.answer(&ev, true, Some("task"), None).await;
    assert_eq!(call.await.unwrap(), json!({"behavior": "allow"}));

    // The rest of that task: the same command runs, by the grant.
    assert_eq!(
        rig.decided("t1", "Bash", input.clone()).await,
        json!({"behavior": "allow"})
    );
    let d = &rig.audit("approval_decision").await[0];
    assert_eq!(d["source"], "grant", "{d}");
    // Another command is not covered.
    let (other, ev) = rig
        .held("t1", "Bash", json!({"command": "rm -rf target"}))
        .await;
    rig.answer(&ev, false, None, None).await;
    other.await.unwrap();
    // The next task asks again.
    let (next, ev) = rig.held("t2", "Bash", input.clone()).await;
    assert_eq!(ev["pane_key"], rig.pane("t2"));
    rig.answer(&ev, false, None, None).await;
    next.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_allow_never_grants_standing_access() {
    let rig = rig("claude", "ask").await;
    let input = json!({"command": "ls"});
    let (call, ev) = rig.held("t1", "Bash", input.clone()).await;
    rig.answer(&ev, true, None, None).await;
    call.await.unwrap();
    let (again, ev) = rig.held("t1", "Bash", input).await;
    rig.answer(&ev, false, None, None).await;
    again.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denys_comment_reaches_the_agent_as_the_reason() {
    let rig = rig("claude", "ask").await;
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "rm -rf /"})).await;
    rig.answer(&ev, false, None, Some("use `cargo clean` instead"))
        .await;
    let r = call.await.unwrap();
    assert_eq!(r["behavior"], "deny");
    let m = r["message"].as_str().unwrap();
    assert!(m.contains("use `cargo clean` instead"), "{m}");
    assert!(m.contains("safer path"), "tells the agent what to do: {m}");
    let d = &rig.audit("approval_decision").await[0];
    assert_eq!(d["comment"], "use `cargo clean` instead");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deny_without_a_comment_still_says_who_denied_and_what_to_do() {
    let rig = rig("claude", "ask").await;
    let (call, ev) = rig.held("t1", "Bash", json!({"command": "ls"})).await;
    rig.answer(&ev, false, None, None).await;
    let r = call.await.unwrap();
    let m = r["message"].as_str().unwrap();
    assert!(
        m.contains("owner denied") && m.contains("safer path"),
        "{m}"
    );
}

// ── nobody answers ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn a_call_nobody_can_answer_is_denied_audited_and_never_allowed() {
    let rig = rig("claude", "ask").await;
    // The only client goes away: nobody can answer. (The daemon notices within moments.)
    let (dir, id, pane) = (rig.dir.clone(), rig.id.clone(), rig.pane("t1"));
    drop(rig.client);
    let start = std::time::Instant::now();
    let r = loop {
        let r = tokio::time::timeout(
            Duration::from_millis(1500),
            crate::crew::mcp::call_for_test(
                &dir,
                &id,
                &pane,
                "approve",
                json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
            ),
        )
        .await;
        match r {
            Ok(r) => break r.unwrap(),
            Err(_) if start.elapsed() < HANG => continue,
            Err(_) => panic!("the call was held for an answer nobody can give"),
        }
    };
    let r: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(r["behavior"], "deny", "{r}");
    assert!(r["message"].as_str().unwrap().contains("nobody"), "{r}");
    let client = HostClient::connect_dir(&dir, "test").await.unwrap();
    let audit = client
        .call(
            "crewmate_audit",
            json!({"id": id, "kind": "approval_decision"}),
        )
        .await
        .unwrap();
    let d = &audit["entries"][0];
    assert_eq!(
        (d["decision"].as_str(), d["source"].as_str()),
        (Some("deny"), Some("timeout")),
        "{d}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── the launch: ask mode, explicitly, never a bypass flag ───────────────────

/// A fake claude: records its argv, one argument per `---ARG---`, then answers like `claude -p` would.
fn fake_claude(dir: &Path, result: &str) -> PathBuf {
    let reply = json!({"type": "result", "result": result, "session_id": "s1", "is_error": false});
    fake_agent(
        dir,
        "claude",
        &format!(
            "n=$(cat '{d}/count' 2>/dev/null || echo 0); n=$((n+1)); echo $n > '{d}/count'\n\
             for a in \"$@\"; do printf '%s\\n---ARG---\\n' \"$a\"; done > \"{d}/args$n.txt\"\n\
             printf '%s\\n' '{reply}'",
            d = dir.display(),
            reply = reply.to_string().replace('\'', "'\\''"),
        ),
    )
}

fn args_of(dir: &Path, n: u32) -> Vec<String> {
    std::fs::read_to_string(dir.join(format!("args{n}.txt")))
        .unwrap()
        .split("\n---ARG---\n")
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect()
}

async fn run_task(rig: &Rig, prompt: &str) -> (String, Value) {
    let v = rig
        .client
        .call(
            "crewmate_task",
            json!({"id": rig.id, "prompt": prompt, "source": "test"}),
        )
        .await
        .unwrap();
    let task = v["task"]["id"].as_str().unwrap().to_string();
    let done = wait_task_state(&rig.client, &rig.id, &task, "done", HANG).await;
    (task, done)
}

#[tokio::test(flavor = "multi_thread")]
async fn claude_is_always_launched_in_ask_mode_even_for_a_bypass_crewmate_and_is_told_its_mode() {
    let _lock = lock_agent_bin().await;
    for (mode, name, runs) in [
        ("ask", "Ask", "every tool call waits"),
        (
            "edits",
            "Accept edits",
            "file edits inside your working tree",
        ),
        ("bypass", "Bypass", "every tool call runs without asking"),
    ] {
        let rig = rig("claude", mode).await;
        let bin = fake_claude(rig._tmp.path(), "done");
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
            Some(("claude".into(), bin.to_string_lossy().into_owned()));
        run_task(&rig, "do it").await;
        *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;

        let args = args_of(rig._tmp.path(), 1);
        let pos = |a: &str| {
            args.iter()
                .position(|x| x == a)
                .unwrap_or_else(|| panic!("{mode}: no {a} in {args:?}"))
        };
        assert_eq!(
            args[pos("--permission-mode") + 1],
            "default",
            "{mode}: said explicitly"
        );
        assert_eq!(
            args[pos("--permission-prompt-tool") + 1],
            "atlas:approve",
            "{mode}"
        );
        assert!(
            !args.iter().any(|a| a.contains("dangerously")
                || a == "bypassPermissions"
                || a == "acceptEdits"),
            "{mode}: a bypass flag in {args:?}"
        );
        // The prompt, the last argument, starts with the fragment: the mode, what runs, the rails, what to do.
        let prompt = args.last().unwrap();
        assert!(
            prompt.contains(&format!("Your permission mode is {name}")),
            "{mode}: {prompt}"
        );
        assert!(prompt.contains(runs), "{mode}: {prompt}");
        assert!(
            prompt.contains(".git") && prompt.contains(".env"),
            "{mode}: {prompt}"
        );
        assert!(
            prompt.contains("find a safer path, or ask the user"),
            "{mode}: {prompt}"
        );
        assert!(
            prompt.contains("Task: do it"),
            "{mode}: the task follows: {prompt}"
        );
        rig.client.call("shutdown", json!({})).await.ok();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fragment_is_sent_again_when_the_mode_changed_before_the_next_turn() {
    let _lock = lock_agent_bin().await;
    let rig = rig("claude", "ask").await;
    // The first turn asks the owner for something (a parked card); the second turn carries the answer.
    let first = json!({"type": "result", "session_id": "s1", "is_error": false,
        "result": "Need it.\nATLAS_ACTION: {\"kind\":\"input\",\"label\":\"name\",\"reason\":\"to go on\"}"});
    let dir = rig._tmp.path();
    let bin = fake_agent(
        dir,
        "claude",
        &format!(
            "n=$(cat '{d}/count' 2>/dev/null || echo 0); n=$((n+1)); echo $n > '{d}/count'\n\
             for a in \"$@\"; do printf '%s\\n---ARG---\\n' \"$a\"; done > \"{d}/args$n.txt\"\n\
             if [ $n = 1 ]; then printf '%s\\n' '{first}'; else printf '%s\\n' '{second}'; fi",
            d = dir.display(),
            first = first.to_string().replace('\'', "'\\''"),
            second = r#"{"type":"result","result":"done","session_id":"s1","is_error":false}"#,
        ),
    );
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
        Some(("claude".into(), bin.to_string_lossy().into_owned()));
    let v = rig
        .client
        .call(
            "crewmate_task",
            json!({"id": rig.id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    let task = v["task"]["id"].as_str().unwrap().to_string();
    wait_task_state(&rig.client, &rig.id, &task, "waiting_action", HANG).await;
    assert!(
        args_of(dir, 1)
            .last()
            .unwrap()
            .contains("Your permission mode is Ask")
    );

    rig.set_mode("edits").await.unwrap();
    rig.client
        .call(
            "crewmate_action_answer",
            json!({"id": rig.id, "task": task, "value": "Pat"}),
        )
        .await
        .unwrap();
    wait_task_state(&rig.client, &rig.id, &task, "done", HANG).await;
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    let second = args_of(dir, 2);
    let prompt = second.last().unwrap();
    assert!(
        prompt.contains("Your permission mode is Accept edits"),
        "{prompt}"
    );

    // A turn with the mode unchanged does not repeat it.
    rig.client.call("shutdown", json!({})).await.ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denied_claude_call_is_told_the_mode_once_and_again_when_it_changes() {
    let rig = rig("claude", "ask").await;
    let deny = |what: &'static str| {
        let rig = &rig;
        async move {
            let (call, ev) = rig.held("t1", "Bash", json!({"command": what})).await;
            rig.answer(&ev, false, None, None).await;
            call.await.unwrap()["message"].as_str().unwrap().to_string()
        }
    };
    // A pane the agent was never told about hears its mode with the first deny (a task start tells it earlier).
    let first = deny("a").await;
    assert!(first.contains("Your permission mode is Ask"), "{first}");
    // The mode did not change: it is not repeated.
    let second = deny("b").await;
    assert!(!second.contains("Your permission mode"), "{second}");
    // It did: the next deny carries the new one.
    rig.set_mode("edits").await.unwrap();
    let third = deny("c").await;
    assert!(
        third.contains("Your permission mode is Accept edits"),
        "{third}"
    );
}

// ── omp, through the real extension ─────────────────────────────────────────

/// A fake omp for `calls` (`FAKE_OMP_CALLS`), which writes the prompt it got to `<dir>/prompt.txt`.
fn omp_with(dir: &Path, calls: Value) -> PathBuf {
    let mjs = dir.join("fake_omp.mjs");
    std::fs::write(&mjs, include_str!("fake_omp.mjs")).unwrap();
    fake_agent(
        dir,
        "omp",
        &format!(
            "FAKE_OMP_TIME_SCALE=100 FAKE_OMP_PROMPT_FILE='{d}/prompt.txt' FAKE_OMP_CALLS='{calls}' exec node '{mjs}' \"$@\"",
            d = dir.display(),
            calls = calls.to_string().replace('\'', "'\\''"),
            mjs = mjs.display(),
        ),
    )
}

/// Start a task on the rig's omp crewmate with the fake omp running `calls`; returns the task id.
async fn start_omp_task(rig: &Rig, calls: Value) -> String {
    let bin = omp_with(rig._tmp.path(), calls);
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
        Some(("omp".into(), bin.to_string_lossy().into_owned()));
    let v = rig
        .client
        .call(
            "crewmate_task",
            json!({"id": rig.id, "prompt": "go", "source": "test"}),
        )
        .await
        .unwrap();
    v["task"]["id"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn omp_in_accept_edits_mode_runs_an_edit_in_the_tree_and_holds_a_command() {
    if !have_node() {
        return;
    }
    let _lock = lock_agent_bin().await;
    let rig = rig("omp", "edits").await;
    let calls = json!([
        {"toolName": "edit", "args": {"input": format!("[{}#1A2B]\nPUT 1.=1:\n+let a = 1;\n", rig.tree("src/a.rs"))}},
        {"toolName": "bash", "args": {"command": "cargo test"}},
    ]);
    let task = start_omp_task(&rig, calls).await;
    // The edit ran without asking; the command is what waits.
    let ev = rig.wait_pending().await;
    assert_eq!(ev["tool_name"], "bash", "{ev}");
    rig.answer(&ev, false, None, Some("run it in the sandbox"))
        .await;
    let done = wait_task_state(&rig.client, &rig.id, &task, "done", HANG).await;
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    let reply = done["result"].as_str().unwrap();
    assert!(reply.contains("ran: edit"), "{reply}");
    // The deny's reason, with the person's comment, is what omp's block says.
    assert!(
        reply.contains("blocked: the owner denied this tool call: run it in the sandbox"),
        "{reply}"
    );
    let audit = rig.audit("approval_decision").await;
    assert_eq!(audit.len(), 2, "{audit:?}");
    assert_eq!(audit[1]["source"], "mode");
    assert_eq!(audit[0]["source"], "person");
}

#[tokio::test(flavor = "multi_thread")]
async fn omp_in_bypass_mode_runs_a_command_and_still_holds_a_write_to_dotenv() {
    if !have_node() {
        return;
    }
    let _lock = lock_agent_bin().await;
    let rig = rig("omp", "bypass").await;
    let calls = json!([
        {"toolName": "bash", "args": {"command": "make"}},
        {"toolName": "write", "args": {"path": rig.tree(".env"), "content": "K=1"}},
    ]);
    let task = start_omp_task(&rig, calls).await;
    let ev = rig.wait_pending().await;
    assert_eq!(
        (ev["tool_name"].as_str(), ev["rail"].as_str()),
        (Some("write"), Some("protected_path")),
        "{ev}"
    );
    rig.answer(&ev, true, None, Some("fine, only this key"))
        .await;
    let done = wait_task_state(&rig.client, &rig.id, &task, "done", HANG).await;
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    let reply = done["result"].as_str().unwrap();
    assert!(
        reply.contains("ran: make") && reply.contains("ran: write"),
        "{reply}"
    );
    // An allow's comment reaches omp as a steering message.
    assert!(
        reply.contains(
            "steer: [Atlas] The owner allowed write and said: fine, only this key (steer)"
        ),
        "{reply}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn omp_in_ask_mode_holds_and_a_switch_runs_it_and_tells_omp_the_new_mode() {
    if !have_node() {
        return;
    }
    let _lock = lock_agent_bin().await;
    let rig = rig("omp", "ask").await;
    let task = start_omp_task(
        &rig,
        json!([{"toolName": "bash", "args": {"command": "echo hi"}}]),
    )
    .await;
    rig.wait_pending().await;
    // The task start carried the fragment of the mode omp was in.
    let prompt = std::fs::read_to_string(rig._tmp.path().join("prompt.txt")).unwrap();
    assert!(prompt.contains("Your permission mode is Ask"), "{prompt}");
    assert!(prompt.contains("Task: go"), "{prompt}");

    rig.set_mode("bypass").await.unwrap();
    let done = wait_task_state(&rig.client, &rig.id, &task, "done", HANG).await;
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    let reply = done["result"].as_str().unwrap();
    assert!(reply.contains("ran: echo hi"), "{reply}");
    // omp hears about the change at once, as a steering message, with the very decision.
    assert!(
        reply.contains("steer: [Atlas permissions] Your permission mode is Bypass"),
        "{reply}"
    );
    assert_eq!(rig.audit("approval_decision").await[0]["source"], "mode");
}

#[tokio::test(flavor = "multi_thread")]
async fn omp_is_never_launched_with_auto_approve_and_always_with_the_extension() {
    if !have_node() {
        return;
    }
    let _lock = lock_agent_bin().await;
    let rig = rig("omp", "bypass").await;
    let dir = rig._tmp.path().to_path_buf();
    let mjs = dir.join("fake_omp.mjs");
    std::fs::write(&mjs, include_str!("fake_omp.mjs")).unwrap();
    let bin = fake_agent(
        &dir,
        "omp",
        &format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{d}/argv.txt'\n\
             FAKE_OMP_TIME_SCALE=100 FAKE_OMP_CALLS='[]' exec node '{mjs}' \"$@\"",
            d = dir.display(),
            mjs = mjs.display(),
        ),
    );
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() =
        Some(("omp".into(), bin.to_string_lossy().into_owned()));
    run_task(&rig, "go").await;
    *crate::crew::engine::TEST_ENGINE_PROGRAM.lock() = None;
    let argv = std::fs::read_to_string(dir.join("argv.txt")).unwrap();
    let argv: Vec<&str> = argv.lines().collect();
    assert!(!argv.contains(&"--auto-approve"), "{argv:?}");
    let ext = argv
        .iter()
        .position(|a| *a == "--extension")
        .unwrap_or_else(|| panic!("{argv:?}"));
    assert!(argv[ext + 1].ends_with("omp-approval.ts"), "{argv:?}");
}

// ── Telegram ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn telegram_mode_shows_and_switches_for_the_owners_chat_only() {
    atlas_i18n::set_lang("en");
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let first = daemon(&dir).await;
    let v = first
        .call(
            "crewmate_create",
            json!({"name": "Revisor", "role": "r", "agent": "claude", "mode": "ask", "engine": {"kind": "claude"}}),
        )
        .await
        .unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    first.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let msg = |update_id: i64, chat: i64, text: &str| json!({"update_id": update_id, "message": {"message_id": update_id, "chat": {"id": chat}, "text": text}});
    let updates = json!([
        msg(1, 42, "/mode"),
        msg(2, 42, "/mode revisor bypass"),
        // Not the owner's chat: ignored, and no reply goes to it.
        msg(3, 99, "/mode Revisor ask"),
        msg(4, 42, "/mode"),
        msg(5, 42, "/mode Nobody edits"),
    ]);
    let (base, _paths, sent) = super::attachments::fake_telegram(updates, vec![], 0).await;
    let client = super::attachments::daemon_with_telegram(&dir, &base).await;

    super::attachments::wait_for("the replies", || (sent.lock().len() >= 4).then_some(())).await;
    let sent = sent.lock().clone();
    assert_eq!(sent[0], "Revisor: Ask", "{sent:?}");
    assert_eq!(
        sent[1], "Revisor is now in Bypass mode. It applies from the next tool call.",
        "{sent:?}"
    );
    assert_eq!(
        sent[2], "Revisor: Bypass",
        "chat 99 changed nothing: {sent:?}"
    );
    assert_eq!(sent[3], "Crewmate 'Nobody' not found.", "{sent:?}");
    assert_eq!(sent.len(), 4, "{sent:?}");

    let audit = client
        .call(
            "crewmate_audit",
            json!({"id": id, "kind": "approval_mode_changed"}),
        )
        .await
        .unwrap();
    let e = &audit["entries"][0];
    assert_eq!(
        (e["from"].as_str(), e["to"].as_str(), e["device"].as_str()),
        (Some("ask"), Some("bypass"), Some("telegram:42")),
        "{e}"
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── the contract: Rust → Desktop and Mobile, from the real output ───────────

fn write_fixture(rel: &str, name: &str, live: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel).join(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{name} missing: run UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::permissions")
    }))
    .unwrap();
    assert_eq!(
        &stored, live,
        "{name} changed: rerun with UPDATE_FIXTURES=1"
    );
}

/// The same bytes to both clients.
fn fixture(name: &str, live: &Value) {
    write_fixture("../../apps/desktop/src/__fixtures__", name, live);
    write_fixture("../../apps/mobile/test/fixtures", name, live);
}

#[tokio::test(flavor = "multi_thread")]
async fn permission_mode_fixtures_are_real_payloads() {
    let rig = rig("claude", "ask").await;
    let codex = rig
        .client
        .call(
            "crewmate_create",
            json!({"name": "Cx", "role": "r", "agent": "codex", "engine": {"kind": "codex"}}),
        )
        .await
        .unwrap();
    let rig_ref = &rig;
    let listed = |id: String| async move {
        let list = rig_ref
            .client
            .call("crewmate_list", json!({}))
            .await
            .unwrap();
        let mut c = list["crewmates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id.as_str())
            .unwrap()
            .clone();
        c["id"] = json!("crewmate-fixture");
        c
    };

    // A crewmate as `crewmate_list` shows it in each mode (`approval` is the legacy field: `auto` only for
    // bypass), one whose engine cannot ask, and the reply of a switch, which is the same shape.
    let mut switched = rig.set_mode("edits").await.unwrap();
    switched["id"] = json!("crewmate-fixture");
    let edits = listed(rig.id.clone()).await;
    let unsupported = listed(codex["id"].as_str().unwrap().to_string()).await;
    rig.set_mode("bypass").await.unwrap();
    let bypass = listed(rig.id.clone()).await;
    assert_eq!(
        (edits["mode"].as_str(), edits["approval"].as_str()),
        (Some("edits"), Some("ask"))
    );
    assert_eq!(
        (bypass["mode"].as_str(), bypass["approval"].as_str()),
        (Some("bypass"), Some("auto"))
    );
    assert_eq!(
        (
            unsupported["mode"].as_str(),
            unsupported["approval_supported"].as_bool()
        ),
        (Some("bypass"), Some(false))
    );
    assert_eq!(
        switched, edits,
        "the reply of a switch is the crewmate as the list shows it"
    );

    // The prompt a protected call raises while the mode would allow it, as the clients get it, and the
    // person's answer (allow for the task, with a comment).
    let (call, mut pending) = rig.held("t1", "Write", json!({"file_path": ".env"})).await;
    rig.answer(&pending, true, Some("task"), Some("only this once"))
        .await;
    call.await.unwrap();
    pending["req"] = json!("req-fixture");
    pending["pane_key"] = json!("crew:crewmate-fixture:task-fixture");

    // The audit entries a decision and a switch leave.
    let mut decision = rig.audit("approval_decision").await[0].clone();
    let mut changed = rig.audit("approval_mode_changed").await[0].clone();
    for e in [&mut decision, &mut changed] {
        e["ts"] = json!(1_700_000_000);
        e["seq"] = json!(1);
        e["crewmate_id"] = json!("crewmate-fixture");
    }
    decision["task_id"] = json!("task-fixture");
    decision["req"] = json!("req-fixture");
    decision["wait_ms"] = json!(1200);

    // The crew's settings, and its refusal of Bypass.
    let settings = settings_with(
        &rig.client
            .call("crew_settings_get", json!({}))
            .await
            .unwrap(),
    );
    rig.client
        .call(
            "crew_settings_set",
            json!({"short_replies": true, "allow_bypass": false}),
        )
        .await
        .unwrap();
    let no_bypass = settings_with(
        &rig.client
            .call("crew_settings_get", json!({}))
            .await
            .unwrap(),
    );
    let refused = rig.set_mode("bypass").await.unwrap_err();
    let coded = refused
        .chain()
        .find_map(|c| c.downcast_ref::<Coded>())
        .unwrap();
    let error = json!({"code": coded.code, "args": coded.args, "message": coded.message});

    fixture(
        "crew_permission_modes.json",
        &json!({
            "crewmate_edits": edits,
            "crewmate_bypass": bypass,
            "crewmate_unsupported": unsupported,
            "set_approval": switched,
            "pending_approval": pending,
            "audit_approval_decision": decision,
            "audit_approval_mode_changed": changed,
            "settings": settings,
            "settings_no_bypass": no_bypass,
            "refusal": error,
        }),
    );
    rig.client.call("shutdown", json!({})).await.ok();
}

/// The two keys of `crew_settings_get` this feature added, the rest being the other fixtures' business.
fn settings_with(s: &Value) -> Value {
    json!({"default_approval": s["default_approval"], "allow_bypass": s["allow_bypass"]})
}

// ── the launch of a terminal agent (no engine) ──────────────────────────────

// #642: a terminal claude crewmate used to start with no permission flag at all in ask/edits, so a `defaultMode`
// of bypass in the user's own settings could widen it behind hostd's back. These are the arguments the PTY runner
// really starts each agent with (`runner::launch_argv`, what `run_one_pty` calls).
#[test]
fn a_terminal_claude_is_launched_in_ask_mode_said_explicitly_in_every_mode() {
    use crate::crew::store::{Mode, Task};
    let claude = atlas_core::agents::get("claude").unwrap();
    for mode in [Mode::Ask, Mode::Edits, Mode::Bypass] {
        let argv = crate::crew::runner::launch_argv(claude, mode, &Task::new("test", "p"), "do it");
        assert_eq!(
            argv,
            ["claude", "--permission-mode", "default", "do it"],
            "{mode:?}: ask mode, explicit, no bypass flag, the prompt last"
        );
    }
    // omp has no flag to name (its extension asks hostd), and still never its bypass flag.
    let omp = atlas_core::agents::get("omp").unwrap();
    for mode in [Mode::Ask, Mode::Bypass] {
        let argv = crate::crew::runner::launch_argv(omp, mode, &Task::new("test", "p"), "go");
        assert!(
            !argv.iter().any(|a| a == "--auto-approve"),
            "{mode:?}: {argv:?}"
        );
    }
}

#[test]
fn an_agent_that_cannot_ask_gets_its_bypass_flag_only_in_bypass_for_a_task_that_is_not_a_strangers()
{
    use crate::crew::store::{Mode, Task};
    let codex = atlas_core::agents::get("codex").unwrap();
    let argv = |mode, no_auto: bool| {
        let mut t = Task::new("test", "p");
        t.no_auto = no_auto;
        crate::crew::runner::launch_argv(codex, mode, &t, "go")
    };
    assert!(
        argv(Mode::Bypass, false)
            .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string())
    );
    assert!(
        !argv(Mode::Bypass, true)
            .iter()
            .any(|a| a.starts_with("--dangerously"))
    );
    assert!(
        !argv(Mode::Ask, false)
            .iter()
            .any(|a| a.starts_with("--dangerously"))
    );
}
