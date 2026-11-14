use serde_json::json;

use super::*;
use crate::agents::AgentHub;

fn token(node: &str, tok: &str) -> PushToken {
    crate::crew::push::new_token(node, tok, "fcm", "g")
}

/// Feed `states` for `pane`, one second apart from `t0`; the kinds that spoke, in order.
fn run(t: &mut Tracker, pane: &str, t0: u64, states: &[State]) -> Vec<&'static str> {
    states
        .iter()
        .enumerate()
        .filter_map(|(i, s)| t.on_status(pane, *s, t0 + i as u64 * 1_000))
        .collect()
}

#[test]
fn working_then_done_pushes_once() {
    let mut t = Tracker::default();
    use State::*;
    assert_eq!(run(&mut t, "p", 0, &[Working, Done]), [KIND_DONE]);
}

// The agent hub pushes an unchanged agent every second (the heartbeat): the same state again is
// not a second transition.
#[test]
fn heartbeats_and_repeats_never_push_again() {
    let mut t = Tracker::default();
    use State::*;
    assert_eq!(
        run(&mut t, "p", 0, &[Working, Working, Done, Done, Done]),
        [KIND_DONE]
    );
}

// A pane that appears already Done/Idle (a hook replay, a daemon that took over, a terminal opened
// and closed) never worked here: nothing finished.
#[test]
fn done_without_working_is_not_news() {
    let mut t = Tracker::default();
    use State::*;
    assert_eq!(run(&mut t, "p", 0, &[Done, Idle, Done]), Vec::<&str>::new());
}

// The program exiting while the agent worked ends the turn too (`AgentHub::idle`).
#[test]
fn working_then_idle_pushes_done() {
    let mut t = Tracker::default();
    use State::*;
    assert_eq!(run(&mut t, "p", 0, &[Working, Idle]), [KIND_DONE]);
}

#[test]
fn approval_and_question_push_needs_you_once_per_wait() {
    let mut t = Tracker::default();
    use State::*;
    assert_eq!(run(&mut t, "p", 0, &[Working, Blocked]), [KIND_NEEDS_YOU]);
    // Blocked → Waiting is the same wait, and the answer (Working) is not news.
    assert_eq!(
        run(&mut t, "p", 5_000, &[Waiting, Working]),
        Vec::<&str>::new()
    );
    let mut t = Tracker::default();
    assert_eq!(run(&mut t, "p", 0, &[Working, Waiting]), [KIND_NEEDS_YOU]);
}

// Done after a wait still announces the finish.
#[test]
fn a_turn_that_asked_then_finished_pushes_both() {
    let mut t = Tracker::default();
    use State::*;
    let mut out = run(&mut t, "p", 0, &[Working, Blocked, Working]);
    out.extend([Done].iter().filter_map(|s| t.on_status("p", *s, 40_000)));
    assert_eq!(out, [KIND_NEEDS_YOU, KIND_DONE]);
}

// An agent chaining turns (Done then Working again within seconds) is one piece of news, not a
// buzz per turn.
#[test]
fn quick_turns_are_throttled_per_kind() {
    let mut t = Tracker::default();
    use State::*;
    let out = run(
        &mut t,
        "p",
        0,
        &[
            Working, Done, Working, Done, Working, Blocked, Working, Blocked,
        ],
    );
    assert_eq!(out, [KIND_DONE, KIND_NEEDS_YOU]);
    // After the cooldown the next one speaks again.
    assert_eq!(
        t.on_status("p", Done, COOLDOWN_MS + 10_000),
        Some(KIND_DONE)
    );
}

#[test]
fn panes_are_tracked_apart() {
    let mut t = Tracker::default();
    use State::*;
    assert_eq!(t.on_status("a", Working, 0), None);
    assert_eq!(t.on_status("b", Working, 0), None);
    assert_eq!(t.on_status("a", Done, 1_000), Some(KIND_DONE));
    assert_eq!(t.on_status("b", Done, 1_500), Some(KIND_DONE));
}

// After a missed stretch of events (broadcast lag) the tracker adopts the current state quietly.
#[test]
fn adopting_a_state_says_nothing_and_does_not_invent_work() {
    let mut t = Tracker::default();
    t.adopt("p", State::Working);
    // It was working before we looked: finishing is still news (worked is set by on_status only,
    // so a pane adopted mid-turn stays quiet rather than guessing).
    assert_eq!(t.on_status("p", State::Done, 0), None);
}

// A subagent never finishes its parent's pane (its `agent_end` is ignored by the hub), so driving
// the real hub the way omp does produces no `Done` for a subagent, and no push for one.
#[test]
fn a_subagent_finishing_never_pushes_but_the_parent_does() {
    let hub = AgentHub::default();
    let mut t = Tracker::default();
    let mut now = 0u64;
    let mut send = |p: Value| {
        now += 2_000;
        let (pane, st) = hub
            .apply_at("omp", &json!({"pane_key": "t1", "payload": p}), now)
            .unwrap();
        t.on_status(&pane, st.state, now)
    };
    let parent = "/s/abc.jsonl";
    let sub = "/s/abc/HostScout.jsonl";
    assert_eq!(
        send(json!({"event": "before_agent_start", "session_file": parent})),
        None
    );
    assert_eq!(
        send(json!({"event": "before_agent_start", "session_file": sub})),
        None
    );
    assert_eq!(
        send(json!({"event": "agent_end", "session_file": sub})),
        None,
        "a subagent finishing pushed"
    );
    assert_eq!(
        send(json!({"event": "agent_end", "session_file": parent})),
        Some(KIND_DONE)
    );
}

#[test]
fn titles_are_agent_and_folder_only() {
    assert_eq!(
        title_for("omp", Some("/Users/me/code/atlas")),
        "omp · atlas"
    );
    assert_eq!(
        title_for("claude", Some("/Users/me/code/atlas/")),
        "claude · atlas"
    );
    assert_eq!(title_for("codex", None), "codex");
    assert_eq!(title_for("codex", Some("/")), "codex");
    let long = "x".repeat(200);
    assert_eq!(
        title_for("omp", Some(&format!("/a/{long}")))
            .chars()
            .count(),
        TITLE_MAX
    );
}

// What leaves for the relay is exactly kind, pane and title: no transcript text can ride along.
#[test]
fn the_payload_has_kind_pane_and_title_only() {
    let msg = PushMsg {
        kind: KIND_DONE,
        pane: "pane-1".into(),
        title: "omp · atlas".into(),
    };
    assert_eq!(
        serde_json::to_value(&msg).unwrap(),
        json!({"kind": "agent_done", "pane": "pane-1", "title": "omp · atlas"})
    );
    assert_eq!(
        crate::crew::push::agent_body(&msg),
        json!({"kind": "agent_done", "pane": "pane-1", "title": "omp · atlas"})
    );
}

#[test]
fn a_connected_phone_is_left_out_but_an_offline_one_is_told() {
    let toks = [
        token("phone-a", "ta"),
        token("phone-b", "tb"),
        token("", "tc"),
    ];
    let live = ["phone-a".to_string()];
    let who: Vec<_> = audience(&toks, &live)
        .into_iter()
        .map(|t| t.token)
        .collect();
    assert_eq!(who, ["tb", "tc"]);
    assert_eq!(audience(&toks, &[]).len(), 3);
}

// Contract with the phone: `apps/mobile/test/fixtures/push_agent_payload.json` is the data of a push
// as hostd builds it (the relay copies these three keys into the FCM data). The phone's
// `push_payload_test.dart` parses this file. `UPDATE_FIXTURES=1` rewrites it.
#[test]
fn push_payload_fixture_is_the_real_serializer_output() {
    let live = serde_json::to_value(PushMsg {
        kind: KIND_NEEDS_YOU,
        pane: "term-7f3a".into(),
        title: title_for("omp", Some("/Users/me/code/atlas")),
    })
    .unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures/push_agent_payload.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(&live).unwrap() + "\n").unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect(
        "fixture missing: UPDATE_FIXTURES=1 cargo test -p atlas-host push_payload_fixture",
    ))
    .unwrap();
    assert_eq!(
        stored, live,
        "push_agent_payload.json is stale: rerun with UPDATE_FIXTURES=1"
    );
}
