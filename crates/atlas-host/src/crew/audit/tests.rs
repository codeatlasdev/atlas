//! The audit log's own contract: what is written, what never is (secrets), that it stays bounded,
//! and that it survives a restart. Issue #201.

use serde_json::{Value, json};

use super::*;
use crate::crew::store::CrewStore;

struct Fx {
    _t: tempfile::TempDir,
    base: std::path::PathBuf,
    store: CrewStore,
}

fn fx() -> Fx {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().to_path_buf();
    let store = CrewStore::new(&base);
    Fx { _t: t, base, store }
}

fn all() -> Query {
    Query {
        since: None,
        limit: 1000,
        kind: None,
    }
}

// A tool call is audited with its FULL input (the task step keeps 80 characters of it).
#[test]
fn a_tool_call_keeps_its_full_tool_name_and_input() {
    let f = fx();
    let a = Audit::new(&f.base);
    let cmd = format!("echo {}", "x".repeat(500));
    a.record(
        Some("c1"),
        "tool_call",
        json!({"task_id": "t1", "tool_name": "mcp__github__create_issue", "tool_input": {"command": cmd}}),
    );
    let got = a.read(Some("c1"), &all());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0]["kind"], "tool_call");
    assert_eq!(got[0]["crewmate_id"], "c1");
    assert_eq!(got[0]["tool_name"], "mcp__github__create_issue");
    assert_eq!(
        got[0]["tool_input"]["command"], cmd,
        "not cut at 80 characters"
    );
    assert!(got[0]["ts"].as_u64().unwrap() > 0);
}

// A secret the crewmate holds (or the hook token) never reaches the file, wherever it sits in the input.
#[test]
fn secret_values_and_the_hook_token_are_redacted_by_name() {
    let f = fx();
    f.store
        .set_secret("c1", "GITHUB_TOKEN", "ghp_supersecretvalue123")
        .unwrap();
    f.store
        .set_secret("c1", "STRIPE_KEY", "sk_live_abcdef987654")
        .unwrap();
    let a = Audit::new(&f.base);
    // The token is the one hostd wrote to the hook endpoint file for this run.
    crate::paths::write_private(
        &crate::paths::hook_endpoint(&f.base),
        b"ATLAS_HOOK_PORT=1234\nATLAS_HOOK_TOKEN=hooktoken0123456789abcdef\n",
    )
    .unwrap();
    a.record(
        Some("c1"),
        "tool_call",
        json!({
            "tool_name": "Bash",
            "tool_input": {
                "command": "curl -H 'Authorization: ghp_supersecretvalue123' https://x?k=sk_live_abcdef987654",
                "nested": ["hooktoken0123456789abcdef", {"k": "ghp_supersecretvalue123"}],
            },
        }),
    );
    let raw = std::fs::read_to_string(f.store.crewmate_dir("c1").join("audit.jsonl")).unwrap();
    for s in [
        "ghp_supersecretvalue123",
        "sk_live_abcdef987654",
        "hooktoken0123456789abcdef",
    ] {
        assert!(!raw.contains(s), "{s} leaked into the audit file: {raw}");
    }
    let e = &a.read(Some("c1"), &all())[0];
    assert!(
        e["tool_input"]["command"]
            .as_str()
            .unwrap()
            .contains("[secret:GITHUB_TOKEN]")
    );
    assert!(
        e["tool_input"]["command"]
            .as_str()
            .unwrap()
            .contains("[secret:STRIPE_KEY]")
    );
    assert_eq!(e["tool_input"]["nested"][0], "[secret:hook_token]");
}

// Issue #200: an isolated crewmate holds a hook token of its own, and the agent sees it in its
// environment, so a tool input can contain it. It is redacted like the global one.
#[test]
fn a_crewmates_own_hook_token_is_redacted_from_its_audit_file() {
    let f = fx();
    let c = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
    let token = f.store.ensure_hook_token(c).unwrap();
    let a = Audit::new(&f.base);
    a.record(
        Some(c),
        "tool_call",
        json!({"tool_input": {"command": format!("env | grep {token}")}}),
    );
    let raw = std::fs::read_to_string(f.store.crewmate_dir(c).join("audit.jsonl")).unwrap();
    assert!(!raw.contains(&token), "token leaked: {raw}");
    assert!(raw.contains("[secret:hook_token]"), "{raw}");
}

// A secret set AFTER the audit was created is still redacted: the values are read per write, not cached.
#[test]
fn a_secret_set_later_is_redacted_too() {
    let f = fx();
    let a = Audit::new(&f.base);
    f.store
        .set_secret("c1", "LATE", "late-secret-value-1")
        .unwrap();
    a.record(
        Some("c1"),
        "tool_call",
        json!({"tool_input": {"c": "late-secret-value-1"}}),
    );
    let raw = std::fs::read_to_string(f.store.crewmate_dir("c1").join("audit.jsonl")).unwrap();
    assert!(!raw.contains("late-secret-value-1"));
    assert!(raw.contains("[secret:LATE]"));
}

// Too short to be a secret (an empty value or "1" would blank every digit of the log).
#[test]
fn tiny_secret_values_do_not_mangle_the_log() {
    let f = fx();
    f.store.set_secret("c1", "FLAG", "1").unwrap();
    f.store.set_secret("c1", "EMPTY", "").unwrap();
    let a = Audit::new(&f.base);
    a.record(
        Some("c1"),
        "tool_call",
        json!({"tool_input": {"n": "build 1 of 12"}}),
    );
    assert_eq!(
        a.read(Some("c1"), &all())[0]["tool_input"]["n"],
        "build 1 of 12"
    );
}

#[test]
fn a_very_long_input_is_capped_with_a_marker() {
    let f = fx();
    let a = Audit::new(&f.base);
    let big = "y".repeat(100 * 1024);
    a.record(
        Some("c1"),
        "tool_call",
        json!({"tool_name": "Write", "tool_input": {"content": big}}),
    );
    let e = &a.read(Some("c1"), &all())[0];
    let s = e["tool_input"]["content"].as_str().unwrap();
    assert!(s.len() < 9 * 1024, "capped, was {}", s.len());
    assert!(
        s.contains("truncated"),
        "marker says so: {}",
        &s[s.len().saturating_sub(80)..]
    );
    assert!(s.contains("102400"), "marker says how much there was");
}

// The cap never splits a UTF-8 character.
#[test]
fn the_cap_cuts_on_a_character_boundary() {
    let f = fx();
    let a = Audit::new(&f.base);
    a.record(
        Some("c1"),
        "tool_call",
        json!({"tool_input": {"t": "é".repeat(20_000)}}),
    );
    assert_eq!(a.read(Some("c1"), &all()).len(), 1, "valid UTF-8 JSON line");
}

// Never unbounded: a full file rotates, only N files are kept, the newest entries survive.
#[test]
fn rotation_bounds_the_total_size_and_keeps_the_newest() {
    let f = fx();
    let a = Audit::with_limits(&f.base, 4 * 1024, 3);
    for i in 0..400 {
        a.record(
            Some("c1"),
            "tool_call",
            json!({"tool_name": "T", "n": i, "pad": "p".repeat(100)}),
        );
    }
    let dir = f.store.crewmate_dir("c1");
    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("audit.jsonl"))
        .collect();
    assert!(
        files.len() <= 3,
        "keeps at most 3 files, has {}",
        files.len()
    );
    let total: u64 = files.iter().map(|e| e.metadata().unwrap().len()).sum();
    assert!(total <= 3 * 4 * 1024 + 1024, "bounded, is {total}");
    let got = a.read(Some("c1"), &all());
    assert_eq!(got[0]["n"], 399, "newest first, and the newest survived");
    assert!(got.len() < 400, "the oldest were dropped");
}

// Entries are on disk when written: a new Audit (a restarted hostd) reads them, and numbering continues.
#[test]
fn entries_survive_a_restart_and_sequence_numbers_continue() {
    let f = fx();
    Audit::new(&f.base).record(Some("c1"), "task_queued", json!({"task_id": "t1"}));
    let a2 = Audit::new(&f.base);
    a2.record(Some("c1"), "task_started", json!({"task_id": "t1"}));
    let got = a2.read(Some("c1"), &all());
    assert_eq!(got.len(), 2);
    assert_eq!(got[0]["kind"], "task_started");
    assert!(
        got[0]["seq"].as_u64() > got[1]["seq"].as_u64(),
        "seq keeps growing: {got:?}"
    );
}

#[test]
fn read_filters_by_kind_since_and_limit_newest_first() {
    let f = fx();
    let a = Audit::new(&f.base);
    for i in 0..5 {
        a.record(Some("c1"), "tool_call", json!({"n": i}));
    }
    a.record(Some("c1"), "task_finished", json!({"state": "done"}));
    let tools = a.read(
        Some("c1"),
        &Query {
            since: None,
            limit: 3,
            kind: Some("tool_call".into()),
        },
    );
    assert_eq!(
        tools
            .iter()
            .map(|e| e["n"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![4, 3, 2]
    );
    let fin = a.read(
        Some("c1"),
        &Query {
            since: None,
            limit: 10,
            kind: Some("task_finished".into()),
        },
    );
    assert_eq!(fin.len(), 1);
    let future = a.read(
        Some("c1"),
        &Query {
            since: Some(u64::MAX),
            limit: 10,
            kind: None,
        },
    );
    assert!(future.is_empty());
}

// One crewmate's log never shows in another's, and the crew-level log is separate.
#[test]
fn logs_are_per_crewmate_and_the_crew_level_one_is_separate() {
    let f = fx();
    let a = Audit::new(&f.base);
    a.record(Some("c1"), "tool_call", json!({}));
    a.record(Some("c2"), "tool_call", json!({}));
    a.record(None, "crewmate_created", json!({"id": "c1"}));
    assert_eq!(a.read(Some("c1"), &all()).len(), 1);
    assert_eq!(a.read(Some("c2"), &all()).len(), 1);
    let crew = a.read(None, &all());
    assert_eq!(crew.len(), 1);
    assert_eq!(crew[0]["kind"], "crewmate_created");
    assert!(crew[0].get("crewmate_id").is_none());
}

// A torn or hand-edited line does not hide the others.
#[test]
fn an_unreadable_line_is_skipped_not_fatal() {
    let f = fx();
    let a = Audit::new(&f.base);
    a.record(Some("c1"), "tool_call", json!({"n": 1}));
    let p = f.store.crewmate_dir("c1").join("audit.jsonl");
    let mut s = std::fs::read_to_string(&p).unwrap();
    s.push_str("{\"kind\":\"tool_ca\n");
    std::fs::write(&p, s).unwrap();
    a.record(Some("c1"), "tool_call", json!({"n": 2}));
    let got: Vec<Value> = a.read(Some("c1"), &all());
    assert_eq!(got.len(), 2);
}

// The file holds tool inputs and approvals: owner-only like secrets.json.
#[cfg(unix)]
#[test]
fn the_audit_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let f = fx();
    Audit::new(&f.base).record(Some("c1"), "tool_call", json!({}));
    let m = std::fs::metadata(f.store.crewmate_dir("c1").join("audit.jsonl")).unwrap();
    assert_eq!(
        m.permissions().mode() & 0o077,
        0,
        "mode {:o}",
        m.permissions().mode()
    );
}
