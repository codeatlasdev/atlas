//! The `agents:` policy on control tools and the audit trail it leaves (#134).

use super::*;

/// The shared fixture with an `agents:` block (YAML, appended at top level).
fn project_with_agents(agents_yaml: &str) -> Project {
    let p = project();
    let mut yaml = std::fs::read_to_string(p.root().join("atlas.yaml")).unwrap();
    yaml.push_str("agents:\n");
    for l in agents_yaml.lines() {
        yaml.push_str(&format!("  {l}\n"));
    }
    std::fs::write(p.root().join("atlas.yaml"), yaml).unwrap();
    p
}

/// Parsed lines of `.logs/atlas-audit.log` (JSON objects, one per line).
fn audit_lines(p: &Project) -> Vec<Value> {
    std::fs::read_to_string(p.logs().join("atlas-audit.log"))
        .unwrap_or_default()
        .lines()
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("audit line not JSON ({e}): {l}"))
        })
        .collect()
}

/// One request on the control socket as the developer's tools send it (no `caller`).
fn socket_request(p: &Project, line: &str) -> Value {
    let mut s = std::os::unix::net::UnixStream::connect(p.logs().join("atlas.sock")).unwrap();
    s.set_read_timeout(Some(HANG)).unwrap();
    s.write_all(line.as_bytes()).unwrap();
    s.write_all(b"\n").unwrap();
    let mut reply = String::new();
    BufReader::new(&s).read_line(&mut reply).unwrap();
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("bad reply {reply:?}: {e}"))
}

/// #134: `agents: { control: deny }` is the one-line read-only switch. Every control tool is
/// refused by atlas dev itself (nothing restarts), with guidance; read tools keep working;
/// each refusal is in the audit trail with the MCP client's name and version.
#[test]
fn agents_control_deny_refuses_every_control_tool_and_audits_the_refusals() {
    require_python!();
    let p = project_with_agents("control: deny");
    let (_h, mut c) = start_running_as(&p, &["web", "ticker"], "omp-test", "9.9");
    let pids_before = pids_file(&p);

    for (tool, args, tool_cmd, target) in [
        (
            "atlas_restart_service",
            json!({"service": "web"}),
            "restart_service",
            "web",
        ),
        (
            "atlas_stop_service",
            json!({"service": "web"}),
            "stop_service",
            "web",
        ),
        (
            "atlas_start_service",
            json!({"service": "web"}),
            "start_service",
            "web",
        ),
        (
            "atlas_run_action",
            json!({"action": "Hello"}),
            "run_action",
            "Hello",
        ),
    ] {
        let (err, t) = c.call(tool, args);
        assert!(err, "{tool} must be refused: {t}");
        assert!(
            t.contains(&format!("atlas.yaml denies agents {tool_cmd} on {target}")),
            "{tool}: {t}"
        );
        assert!(t.contains("ask the developer"), "{tool}: {t}");
    }
    // Nothing happened: same pids, the action never ran.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        pids_file(&p),
        pids_before,
        "a denied restart/stop must not touch services"
    );
    assert!(
        !p.logs().join("action-hello.log").exists(),
        "a denied action must not run"
    );

    // Read tools and wait_ready are unaffected.
    assert_eq!(status(&mut c)["running"], true);
    let (err, t) = c.call("atlas_config", json!({}));
    assert!(!err, "{t}");
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "web", "timeout_s": HANG.as_secs()}),
    );
    assert!(!err, "{t}");

    let log = audit_lines(&p);
    assert_eq!(log.len(), 4, "one line per refused call: {log:?}");
    for (l, (tool, target)) in log.iter().zip([
        ("restart_service", "web"),
        ("stop_service", "web"),
        ("start_service", "web"),
        ("run_action", "Hello"),
    ]) {
        assert_eq!(l["caller"], "agent", "{l}");
        assert_eq!(l["client"], "omp-test/9.9", "{l}");
        assert_eq!(l["tool"], tool, "{l}");
        assert_eq!(l["target"], target, "{l}");
        assert_eq!(l["result"], "denied", "{l}");
        assert!(l["ts"].as_str().is_some_and(|t| t.len() > 20), "{l}");
        assert!(
            l["detail"].as_str().unwrap().contains("ask the developer"),
            "{l}"
        );
    }
}

/// Per-service allow list: the listed service restarts (and the audit says allowed), any
/// other service and any other tool stays denied.
#[test]
fn agents_allow_list_lets_only_the_listed_service_restart_and_audits_both() {
    require_python!();
    let p = project_with_agents("control: deny\nrestart_service: { allow: [ticker] }");
    let (_h, mut c) = start_running_as(&p, &["web", "ticker"], "claude-code", "2.1");

    let pids_before = pids_file(&p);
    let (err, t) = c.call("atlas_restart_service", json!({"service": "ticker"}));
    assert!(!err, "{t}");
    wait_for("ticker pid to change", HANG, || {
        (pids_file(&p) != pids_before).then_some(())
    });
    let (err, t) = c.call("atlas_restart_service", json!({"service": "web"}));
    assert!(
        err && t.contains("denies agents restart_service on web"),
        "{t}"
    );
    let (err, t) = c.call("atlas_stop_service", json!({"service": "ticker"}));
    assert!(
        err && t.contains("denies agents stop_service on ticker"),
        "{t}"
    );

    let log = audit_lines(&p);
    let seen: Vec<(String, String, String)> = log
        .iter()
        .map(|l| {
            (
                l["tool"].as_str().unwrap().to_string(),
                l["target"].as_str().unwrap().to_string(),
                l["result"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("restart_service".into(), "ticker".into(), "allowed".into()),
            ("restart_service".into(), "web".into(), "denied".into()),
            ("stop_service".into(), "ticker".into(), "denied".into()),
        ]
    );
    assert!(
        log.iter().all(|l| l["client"] == "claude-code/2.1"),
        "{log:?}"
    );
}

/// Existing users: no `agents:` block = every control tool works exactly as before, and
/// every call is still recorded. An allowed call atlas itself refuses is `error`, with why.
#[test]
fn without_an_agents_block_control_tools_work_as_before_and_are_audited() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running_as(&p, &["web", "ticker"], "cursor", "0.5");
    let (err, t) = c.call("atlas_restart_service", json!({"service": "ticker"}));
    assert!(!err, "{t}");
    let (err, t) = c.call(
        "atlas_run_action",
        json!({"action": "Hello", "confirmed": false}),
    );
    assert!(!err, "{t}");
    let (err, t) = c.call("atlas_restart_service", json!({"service": "ghost"}));
    assert!(err && t.contains("unknown service"), "{t}");

    let log = audit_lines(&p);
    let results: Vec<&str> = log.iter().map(|l| l["result"].as_str().unwrap()).collect();
    assert_eq!(results, ["allowed", "allowed", "error"], "{log:?}");
    assert!(
        log[2]["detail"]
            .as_str()
            .unwrap()
            .contains("unknown service"),
        "{log:?}"
    );
    assert_eq!(log[1]["tool"], "run_action");
    assert_eq!(log[1]["target"], "Hello");
}

/// The developer's own tools (desktop, CLI: no `caller` on the socket) are never limited by
/// `agents:` and show up in the trail as `human`.
#[test]
fn the_developers_socket_calls_bypass_the_policy_and_are_audited_as_human() {
    require_python!();
    let p = project_with_agents("control: deny");
    let (_h, _c) = start_running_as(&p, &["web", "ticker"], "omp-test", "9.9");
    let r = socket_request(
        &p,
        r#"{"cmd":"restart_service","args":{"service":"ticker"}}"#,
    );
    assert_eq!(r["ok"], true, "{r}");
    // The same call as an agent on the same socket is refused.
    let r = socket_request(
        &p,
        r#"{"cmd":"restart_service","args":{"service":"ticker"},"caller":{"kind":"agent","name":"raw"}}"#,
    );
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"].as_str().unwrap().contains("denies agents"),
        "{r}"
    );

    let log = audit_lines(&p);
    assert_eq!(
        (log[0]["caller"].as_str(), log[0]["result"].as_str()),
        (Some("human"), Some("allowed"))
    );
    assert!(log[0].get("client").is_none(), "{log:?}");
    assert_eq!(
        (
            log[1]["caller"].as_str(),
            log[1]["client"].as_str(),
            log[1]["result"].as_str()
        ),
        (Some("agent"), Some("raw"), Some("denied"))
    );
}

/// Reading the trail: `atlas_logs service:"audit"` renders it (and honors `lines`), the merged
/// all-services tail and search never mix the JSON file in, and an agent can search it.
#[test]
fn audit_trail_is_readable_via_atlas_logs_and_stays_out_of_the_merged_tail() {
    require_python!();
    let p = project_with_agents("run_action: deny");
    let (_h, mut c) = start_running_as(&p, &["web", "ticker"], "omp-test", "9.9");
    let (err, _) = c.call("atlas_restart_service", json!({"service": "ticker"}));
    assert!(!err);
    let (err, _) = c.call("atlas_run_action", json!({"action": "Hello"}));
    assert!(err);

    let (err, t) = c.call("atlas_logs", json!({"service": "audit"}));
    assert!(!err, "{t}");
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines.len(), 2, "{t}");
    assert!(
        lines[0].contains("agent omp-test/9.9 restart_service ticker -> allowed"),
        "{t}"
    );
    assert!(
        lines[1].contains("agent omp-test/9.9 run_action Hello -> denied"),
        "{t}"
    );
    let (_, t) = c.call("atlas_logs", json!({"service": "audit", "lines": 1}));
    assert_eq!(t.lines().count(), 1);
    assert!(t.contains("run_action Hello -> denied"), "{t}");
    let (_, t) = c.call(
        "atlas_logs_search",
        json!({"query": "denied", "service": "audit"}),
    );
    assert_eq!(t.lines().count(), 1, "{t}");

    let (_, t) = c.call("atlas_logs", json!({"lines": 500}));
    assert!(
        !t.contains("atlas-audit") && !t.contains("restart_service"),
        "merged tail: {t}"
    );
    let (_, t) = c.call("atlas_logs_search", json!({"query": "restart_service"}));
    assert_eq!(
        t, "(no matches)",
        "unfiltered search must skip the audit file"
    );
    let (err, t) = c.call("atlas_logs", json!({"service": "nope"}));
    assert!(
        err && !t.contains("atlas-audit"),
        "service list must not offer the audit file: {t}"
    );
}

/// A new `atlas dev` run starts a fresh audit file; the previous run stays readable as `.prev`.
#[test]
fn a_new_run_rotates_the_audit_trail_to_prev() {
    require_python!();
    let p = project_with_agents("control: deny");
    {
        let (_h, mut c) = start_running_as(&p, &["web", "ticker"], "omp-test", "9.9");
        let (err, _) = c.call("atlas_stop_service", json!({"service": "web"}));
        assert!(err);
        assert_eq!(audit_lines(&p).len(), 1);
    }
    let (_h, mut c) = start_running_as(&p, &["web", "ticker"], "omp-test", "9.9");
    assert!(audit_lines(&p).is_empty(), "fresh file for the new run");
    let prev = std::fs::read_to_string(p.logs().join("atlas-audit.prev.log")).unwrap();
    assert!(prev.contains("\"stop_service\""), "{prev}");
    let (_, t) = c.call(
        "atlas_logs_search",
        json!({"query": "stop_service", "service": "audit", "include_previous": true}),
    );
    assert!(t.contains("stop_service web -> denied"), "{t}");
}

/// The audit line says which agent session acted: the host's session variables are inherited by
/// `atlas mcp` and recorded (only the ones that are set).
#[test]
fn audit_records_the_agent_session_from_the_environment() {
    require_python!();
    let p = project_with_agents("control: deny");
    let _h = Headless::spawn(p.root());
    let mut c = McpClient::spawn_with_env(
        &["mcp", "--dir", p.root().to_str().unwrap()],
        None,
        &[("OMP_SESSION_ID", "sess-42"), ("CLAUDE_SESSION_ID", "")],
    );
    c.handshake_as("omp", "14.1");
    wait_for("socket serving", HANG, || {
        std::os::unix::net::UnixStream::connect(p.logs().join("atlas.sock")).ok()
    });
    let (err, t) = c.call("atlas_restart_service", json!({"service": "web"}));
    assert!(err && t.contains("denies agents"), "{t}");
    let log = audit_lines(&p);
    assert_eq!(
        log[0]["session"],
        json!({"OMP_SESSION_ID": "sess-42"}),
        "{log:?}"
    );
}
