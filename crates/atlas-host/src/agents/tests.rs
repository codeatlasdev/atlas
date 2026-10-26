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

/// Pushes of `agent_status` for `pane` the hub made so far (drained from `rx`).
fn pushes(rx: &mut broadcast::Receiver<Value>, pane: &str) -> usize {
    let mut n = 0;
    while let Ok(ev) = rx.try_recv() {
        n += (ev["ev"] == "agent_status" && ev["pane_key"] == pane) as usize;
    }
    n
}

// Incident (#110, "terminal freezes with 7+ agents"): omp posts every tool event, and each hook became an
// `agent_status` push to the desktop (a Tauri event, a store write, tray/sidebar/feed recomputation) and to every
// phone, whether or not anything a client shows had changed. 7 agents x a few tool events a second is a steady
// stream of identical statuses on the UI thread.
#[test]
fn a_hook_that_changes_nothing_is_not_pushed_every_time() {
    let hub = AgentHub::default();
    let mut rx = hub.subscribe();
    let send = |t: u64, p: Value| hub.apply_at("omp", &json!({"pane_key": "t1", "payload": p}), t);
    send(
        1_000,
        json!({"event": "agent_start", "session_file": "/s/a.jsonl", "model": "m"}),
    );
    assert_eq!(pushes(&mut rx, "t1"), 1, "the first status is news");
    for i in 1..=20 {
        send(
            1_000 + i * 10,
            json!({"event": "tool_execution_start", "session_file": "/s/a.jsonl", "model": "m"}),
        );
    }
    assert_eq!(
        pushes(&mut rx, "t1"),
        0,
        "20 identical events in 200 ms: nothing to say"
    );
}

// The desktop falls back to the terminal title once `updated_at` is `HOOK_GRACE_MS` (5 s) old, so a working agent
// whose hooks are all no-ops must still be heard from: at most once per `HEARTBEAT_MS`, always below the grace.
#[test]
fn an_unchanged_agent_still_heartbeats_inside_the_desktops_grace_period() {
    use super::HEARTBEAT_MS;
    const { assert!(HEARTBEAT_MS < 5_000, "the desktop trusts hooks for 5 s") };
    let hub = AgentHub::default();
    let mut rx = hub.subscribe();
    let send = |t: u64| {
        hub.apply_at(
            "omp",
            &json!({"pane_key": "t1", "payload": {"event": "tool_execution_start"}}),
            t,
        )
    };
    send(10_000);
    assert_eq!(pushes(&mut rx, "t1"), 1);
    send(10_000 + HEARTBEAT_MS - 1);
    assert_eq!(pushes(&mut rx, "t1"), 0, "just inside the interval");
    send(10_000 + HEARTBEAT_MS);
    assert_eq!(
        pushes(&mut rx, "t1"),
        1,
        "a full interval later: the heartbeat"
    );
    send(10_000 + HEARTBEAT_MS + 1);
    assert_eq!(
        pushes(&mut rx, "t1"),
        0,
        "the interval restarts at each push"
    );
}

// Class of the incident above: every field a client shows must still push at once. A throttle keyed on the state
// alone would swallow a new prompt, a model switch, the assistant's message or a session change.
#[test]
fn every_visible_change_is_pushed_at_once_not_held_for_the_heartbeat() {
    let hub = AgentHub::default();
    let mut rx = hub.subscribe();
    let send = |t: u64, p: Value| hub.apply_at("omp", &json!({"pane_key": "t1", "payload": p}), t);
    send(
        1_000,
        json!({"event": "agent_start", "session_file": "/s/a.jsonl", "model": "m1"}),
    );
    pushes(&mut rx, "t1");
    let cases = [
        ("state", json!({"event": "tool_call", "tool_name": "ask"})),
        (
            "model",
            json!({"event": "tool_call", "tool_name": "ask", "model": "m2"}),
        ),
        (
            "prompt",
            json!({"event": "before_agent_start", "prompt": "do it"}),
        ),
        (
            "message",
            json!({"event": "message_end", "text": "done with step one"}),
        ),
        (
            "session",
            json!({"event": "session_start", "session_file": "/s/b.jsonl"}),
        ),
    ];
    for (i, (what, p)) in cases.into_iter().enumerate() {
        send(1_001 + i as u64, p);
        assert_eq!(
            pushes(&mut rx, "t1"),
            1,
            "a changed {what} must push at once"
        );
    }
}

// A pane is throttled on its own: one agent's chatter must not swallow another's first status or heartbeat.
#[test]
fn the_heartbeat_interval_is_per_pane() {
    let hub = AgentHub::default();
    let mut rx = hub.subscribe();
    let ev = |pane: &str| json!({"pane_key": pane, "payload": {"event": "agent_start"}});
    let drain = |rx: &mut broadcast::Receiver<Value>| {
        let mut panes = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if e["ev"] == "agent_status" {
                panes.push(e["pane_key"].as_str().unwrap().to_string());
            }
        }
        panes
    };
    hub.apply_at("omp", &ev("a"), 1_000);
    hub.apply_at("omp", &ev("b"), 1_001);
    assert_eq!(
        drain(&mut rx),
        ["a", "b"],
        "each pane's first status is pushed"
    );
    hub.apply_at("omp", &ev("a"), 1_002);
    hub.apply_at("omp", &ev("b"), 1_003);
    assert!(
        drain(&mut rx).is_empty(),
        "both are inside their own interval"
    );
    hub.apply_at("omp", &ev("b"), 1_001 + super::HEARTBEAT_MS);
    assert_eq!(drain(&mut rx), ["b"], "b is due, a is on its own clock");
}
