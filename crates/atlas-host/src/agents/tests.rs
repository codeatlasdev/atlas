use super::*;
use serde_json::json;

fn ev(e: &str) -> Value {
    json!({"event": e})
}

#[test]
fn transitions() {
    use State::*;
    assert_eq!(normalize(Some(Idle), &ev("agent_start")), Some(Working));
    assert_eq!(
        normalize(
            Some(Working),
            &json!({"event":"tool_call","tool_name":"ask"})
        ),
        Some(Waiting)
    );
    assert_eq!(
        normalize(
            Some(Working),
            &json!({"event":"tool_call","tool_name":"bash"})
        ),
        None
    );
    assert_eq!(
        normalize(Some(Working), &ev("tool_approval_requested")),
        Some(Blocked)
    );
    assert_eq!(normalize(Some(Blocked), &ev("tool_execution_end")), None);
    assert_eq!(
        normalize(Some(Blocked), &ev("tool_approval_resolved")),
        Some(Working)
    );
    assert_eq!(
        normalize(
            Some(Working),
            &json!({"event":"agent_end","will_continue":true})
        ),
        None
    );
    assert_eq!(
        normalize(
            Some(Working),
            &json!({"event":"agent_end","will_continue":false})
        ),
        Some(Done)
    );
    assert_eq!(normalize(Some(Done), &ev("session_shutdown")), Some(Idle));
}

#[test]
fn apply_keeps_session_fields_across_events() {
    let hub = AgentHub::default();
    let body = |p: Value| json!({"pane_key": "t1", "worktree_id": "w", "payload": p});
    hub.apply(
        "omp",
        &body(json!({"event":"session_start","session_id":"s1","session_file":"/f"})),
    );
    let (_, st) = hub
        .apply("omp", &body(json!({"event":"agent_start","prompt":"hi"})))
        .unwrap();
    assert_eq!(st.state, State::Working);
    assert_eq!(st.session_file.as_deref(), Some("/f"));
    assert_eq!(st.prompt.as_deref(), Some("hi"));
    assert!(hub.apply("omp", &json!({"payload":{}})).is_none());
}

#[test]
fn events_without_pane_key_resolve_by_agent_and_cwd() {
    let hub = AgentHub::default();
    hub.bind("tab-1", "codex", "/tmp/wt");
    let ev = |agent: &str, cwd: &str| {
        json!({"payload": {"event": "user_prompt_submit", "cwd": cwd}})
            .pipe(|b| hub.apply(agent, &b))
    };
    assert_eq!(
        ev("codex", "/private/tmp/wt/").map(|x| x.0).as_deref(),
        Some("tab-1")
    );
    assert!(ev("codex", "/tmp/other").is_none());
    assert!(ev("claude", "/tmp/wt").is_none());
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl Pipe for Value {}

/// A stale tap on the phone (prompt already answered, or another pane's) must never
/// answer a newer prompt.
#[tokio::test]
async fn approvals_resolve_only_their_own_pending_request() {
    let hub = std::sync::Arc::new(AgentHub::default());
    let mut ev = hub.subscribe();
    // No phone connected: no wait, no prompt pushed.
    assert_eq!(
        hub.ask("p1", "claude", &json!({}), Duration::from_secs(5), false)
            .await,
        None
    );
    let h = hub.clone();
    let asked = tokio::spawn(async move {
        h.ask(
            "p1",
            "claude",
            &json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
            Duration::from_secs(5),
            true,
        )
        .await
    });
    let pushed = ev.recv().await.unwrap();
    assert_eq!(pushed["ev"], "agent_approval");
    assert_eq!(pushed["tool_name"], "Bash");
    let req = pushed["req"].as_str().unwrap().to_string();
    assert!(hub.approve("p2", &req, true).is_err(), "other pane");
    assert!(hub.approve("p1", "nope", true).is_err(), "unknown req");
    hub.approve("p1", &req, false).unwrap();
    assert_eq!(asked.await.unwrap(), Some(false));
    assert_eq!(ev.recv().await.unwrap()["ev"], "agent_approval_done");
    assert!(hub.approve("p1", &req, true).is_err(), "already answered");
}

#[tokio::test]
async fn unanswered_prompt_times_out_to_no_decision() {
    let hub = AgentHub::default();
    assert_eq!(
        hub.ask("p1", "codex", &json!({}), Duration::from_millis(50), true)
            .await,
        None
    );
}

/// A phone that opens an agent after its prompt was pushed (home → conversation) must
/// still get the prompt: the push happened once, so pending prompts are listed too.
#[tokio::test]
async fn pending_prompts_can_be_listed_after_their_push() {
    let hub = std::sync::Arc::new(AgentHub::default());
    assert!(hub.pending_approvals().is_empty());
    let h = hub.clone();
    let asked = tokio::spawn(async move {
        h.ask(
            "p1",
            "claude",
            &json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
            Duration::from_secs(5),
            true,
        )
        .await
    });
    let listed = loop {
        let l = hub.pending_approvals();
        if !l.is_empty() {
            break l;
        }
        tokio::task::yield_now().await;
    };
    // Same shape as the `agent_approval` push, so the phone handles both alike.
    let p = &listed[0];
    assert_eq!(p["ev"], "agent_approval");
    assert_eq!(p["pane_key"], "p1");
    assert_eq!(p["tool_input"]["command"], "ls");
    hub.approve("p1", p["req"].as_str().unwrap(), true).unwrap();
    assert_eq!(asked.await.unwrap(), Some(true));
    assert!(
        hub.pending_approvals().is_empty(),
        "answered prompts are not listed"
    );
}

/// Regression (2026-10-25, phone screenshots): omp loads the status extension in its subagents
/// too, in the same terminal, so their hooks arrive with the parent's pane key. Each subagent's
/// `before_agent_start` replaced the pane's prompt ("Complete assignment thoroughly: …" became
/// the agent's name on the phone), its `session_file` pointed the chat at the subagent's
/// transcript, and its `agent_end` marked the parent done while it was still working.
#[test]
fn subagent_hooks_never_take_over_the_parent_pane() {
    let hub = AgentHub::default();
    let send = |p: Value| hub.apply("omp", &json!({"pane_key": "t1", "payload": p}));
    let parent = "/s/2026-10-25_abc.jsonl";
    send(json!({"event": "session_start", "session_file": parent, "model": "opus"}));
    send(json!({"event": "before_agent_start", "prompt": "fix the sheet", "session_file": parent}));
    let sub = "/s/2026-10-25_abc/HostScout.jsonl";
    send(
        json!({"event": "before_agent_start", "prompt": "Complete assignment thoroughly: x", "session_file": sub, "model": "haiku"}),
    );
    send(json!({"event": "message_end", "text": "scout notes", "session_file": sub}));
    let (_, st) = send(json!({"event": "agent_end", "session_file": sub})).unwrap();
    assert_eq!(st.prompt.as_deref(), Some("fix the sheet"));
    assert_eq!(st.session_file.as_deref(), Some(parent));
    assert_eq!(st.model.as_deref(), Some("opus"));
    assert_eq!(st.last_message, None);
    assert_eq!(st.state, State::Working);
    // A nested subagent (a subagent's own task) is still not the parent.
    let nested = "/s/2026-10-25_abc/HostScout/Deep.jsonl";
    let (_, st) = send(json!({"event": "agent_end", "session_file": nested})).unwrap();
    assert_eq!(st.state, State::Working);
    // The parent's own events still land.
    let (_, st) = send(json!({"event": "agent_end", "session_file": parent})).unwrap();
    assert_eq!(st.state, State::Done);
}

/// A subagent asking for approval still blocks the pane: the human must answer it.
#[test]
fn a_subagent_approval_still_blocks_the_pane() {
    let hub = AgentHub::default();
    let send = |p: Value| hub.apply("omp", &json!({"pane_key": "t1", "payload": p}));
    send(json!({"event": "agent_start", "session_file": "/s/a.jsonl"}));
    let (_, st) =
        send(json!({"event": "tool_approval_requested", "session_file": "/s/a/Sub.jsonl"}))
            .unwrap();
    assert_eq!(st.state, State::Blocked);
}

/// A new session in the same pane (`/new`) is not a subagent: it replaces the file.
#[test]
fn a_new_session_in_the_pane_replaces_the_file() {
    let hub = AgentHub::default();
    let send = |p: Value| hub.apply("omp", &json!({"pane_key": "t1", "payload": p}));
    send(json!({"event": "session_start", "session_file": "/s/a.jsonl"}));
    let (_, st) = send(json!({"event": "session_start", "session_file": "/s/b.jsonl"})).unwrap();
    assert_eq!(st.session_file.as_deref(), Some("/s/b.jsonl"));
}
