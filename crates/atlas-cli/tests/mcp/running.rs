//! A live headless run: services, status, logs, actions and concurrent calls.

use super::*;

/// Incident: every headless fixture timed out on the macOS runners because `python3 -m
/// http.server` resolves the host name (`socket.getfqdn`) before it listens. The fixture's
/// server must listen without any name lookup; stock `http.server` is the control that proves
/// the stall is real (it must NOT come up in the same time).
#[test]
fn fixture_web_server_does_not_resolve_names() {
    require_python!();
    let p = project();
    let port = free_port();
    assert!(
        !serves_with_stalled_dns(
            p.root(),
            &["http.server", &port.to_string(), "--bind", "127.0.0.1"],
            port,
            Duration::from_secs(3)
        ),
        "control: stock http.server should stall on getfqdn"
    );
    let port = free_port();
    assert!(
        serves_with_stalled_dns(p.root(), &["web.py", &port.to_string()], port, HANG),
        "fixture web.py must listen even when name resolution hangs"
    );
}

/// #136 end to end: a service that stays up but keeps logging the same error is reported
/// `degraded` by `atlas_status`, with the repeating line as `probe_error`. (The shared fixture's
/// ticker prints its error once so the other tests can wait for `running`.)
#[test]
fn a_service_repeating_one_error_is_degraded_in_atlas_status() {
    require_python!();
    let p = project();
    let mut yaml = std::fs::read_to_string(p.root().join("atlas.yaml")).unwrap();
    yaml = yaml.replace(
        "actions:\n",
        "  looper:\n    command: \"sh -c 'while true; do echo ERROR db connect timeout; sleep 0.3; done'\"\n\nactions:\n",
    );
    std::fs::write(p.root().join("atlas.yaml"), yaml).unwrap();
    let h = Headless::spawn(p.root());
    let mut c = McpClient::spawn(p.root());
    c.handshake();
    let last = std::cell::RefCell::new(Value::Null);
    let row = common::wait_for_with(
        "looper degraded",
        HANG,
        || {
            let st = status(&mut c);
            let row = st["services"]
                .as_array()?
                .iter()
                .find(|s| s["name"] == "looper")?
                .clone();
            *last.borrow_mut() = st;
            (row["state"] == "degraded").then_some(row)
        },
        || startup_report(&h, &p, &last.borrow()),
    );
    let why = row["probe_error"].as_str().unwrap_or_default();
    assert!(why.contains("db connect timeout"), "{row}");
    assert!(row["unhealthy_since"].is_null(), "{row}");
}

/// Item 4: action output is persisted and readable through `atlas_logs`, both by
/// `action:<label>` and by file name, and merged into the all-services tail.
#[test]
fn action_output_is_readable_via_atlas_logs() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running(&p, &["web"]);
    let (err, t) = c.call("atlas_run_action", json!({"action": "Hello"}));
    assert!(!err, "{t}");
    let file = p.logs().join("action-hello.log");
    wait_for("action log file", HANG, || {
        std::fs::read_to_string(&file)
            .ok()
            .filter(|s| s.contains("hello-from-action"))
    });
    for name in ["action:Hello", "action-hello"] {
        let (err, t) = c.call("atlas_logs", json!({"service": name}));
        assert!(!err, "{name}: {t}");
        assert!(t.contains("hello-from-action"), "{name}: {t}");
    }
    let (_, t) = c.call("atlas_logs_search", json!({"query": "hello-from"}));
    assert!(t.contains("action-hello"), "{t}");
    let (_, t) = c.call("atlas_logs", json!({"lines": 200}));
    assert!(t.contains("hello-from-action"), "merged tail: {t}");
    // atlas_status carries the new per-service facts.
    let st = status(&mut c);
    let web = st["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "web")
        .unwrap();
    assert!(web["uptime_s"].as_u64().is_some(), "{web}");
    assert!(web["exit_code"].is_null(), "{web}");
    assert_eq!(web["restarts"], 0);

    // `atlas_metrics` returns the memory series the live headless run keeps for the service
    // (the first sample lands within one health tick), and `atlas_status` carries the facts.
    let m = wait_for("first memory sample", HANG, || {
        let (err, t) = c.call("atlas_metrics", json!({"service": "web"}));
        let v: Value = serde_json::from_str(&t).ok().filter(|_| !err)?;
        v["samples"].as_array().filter(|s| !s.is_empty())?;
        Some(v)
    });
    assert_eq!(m["running"], true, "{m}");
    assert_eq!(m["service"], "web");
    assert_eq!(m["interval_s"], 5, "{m}");
    assert!(m["rss_bytes"].as_u64().unwrap() > 0, "{m}");
    assert_eq!(m["budget_state"], "ok", "no budget configured: {m}");
    let (err, t) = c.call("atlas_metrics", json!({"service": "nope"}));
    assert!(
        err && t.contains("Known services") && t.contains("web"),
        "{t}"
    );
    let web = wait_for("memory facts in atlas_status", HANG, || {
        let st = status(&mut c);
        st["services"]
            .as_array()?
            .iter()
            .find(|s| s["name"] == "web" && s["memory_rss"].as_u64().is_some_and(|b| b > 0))
            .cloned()
    });
    assert_eq!(web["budget_state"], "ok", "{web}");
}

#[test]
fn running_full_flow() {
    require_python!();
    let p = project_with_never();
    let (_h, mut c) = start_running(&p, &["web", "ticker"]);

    // status
    let st = status(&mut c);
    assert_eq!(st["running"], true);
    assert!(st["pid"].as_i64().unwrap() > 0);
    assert_eq!(service_state(&st, "web").as_deref(), Some("running"));
    assert_eq!(service_state(&st, "ticker").as_deref(), Some("running"));
    let web = st["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "web")
        .unwrap();
    assert_eq!(web["port"].as_u64(), Some(p.web_port as u64));

    // logs: wait for the ticker to have written lines.
    wait_for("ticker logs", HANG, || {
        let (_, t) = c.call("atlas_logs", json!({"service": "ticker"}));
        t.contains("fixture-boom").then_some(())
    });
    let (_, t) = c.call("atlas_logs", json!({"service": "ticker", "lines": 1}));
    assert_eq!(t.lines().count(), 1);
    let (_, t) = c.call("atlas_logs", json!({"service": "ticker", "level": "error"}));
    assert!(t.lines().all(|l| l.contains("fixture-boom")), "{t}");
    let (_, t) = c.call("atlas_logs", json!({"since_minutes": 5}));
    assert!(t.contains("[ticker:") && t.contains("[web:"), "{t}");
    let (_, t) = c.call(
        "atlas_logs",
        json!({"since_minutes": 0, "service": "ticker"}),
    );
    assert!(t.lines().count() <= 3, "since 0 should be near-empty: {t}");
    let (_, t) = c.call("atlas_logs", json!({}));
    assert!(t.contains("[ticker:") && t.contains("[web:"), "merged: {t}");

    // search: substring, regex, include_previous (no prev yet → still ok)
    let (_, t) = c.call("atlas_logs_search", json!({"query": "FIXTURE-BOOM"}));
    assert!(t.contains("fixture-boom"), "{t}");
    wait_for("two ticks", HANG, || {
        let (_, t) = c.call("atlas_logs", json!({"service": "ticker"}));
        (t.matches("tick-line").count() >= 2).then_some(())
    });
    let (_, t) = c.call(
        "atlas_logs_search",
        json!({"query": "/tick-(line|none)/", "service": "ticker", "limit": 2}),
    );
    assert_eq!(t.lines().count(), 2, "{t}");
    let (_, t) = c.call(
        "atlas_logs_search",
        json!({"query": "fixture-boom", "include_previous": true, "service": "ticker"}),
    );
    assert!(t.contains("fixture-boom"));

    // restart_service changes the pid and the service returns to running.
    let pids_before = pids_file(&p);
    assert!(!pids_before.trim().is_empty(), "no .pids while running");
    let (err, t) = c.call("atlas_restart_service", json!({"service": "web"}));
    assert!(!err, "{t}");
    assert_eq!(serde_json::from_str::<Value>(&t).unwrap()["queued"], true);
    wait_for("web pid to change", HANG, || {
        (pids_file(&p) != pids_before).then_some(())
    });
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "web", "timeout_s": HANG.as_secs()}),
    );
    assert!(!err, "{t}");
    assert!(
        t.contains("ready after") && t.contains("web=running"),
        "{t}"
    );
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", p.web_port)).is_ok(),
        "web must listen again after restart"
    );

    // stop → stopped/disabled, start → running
    let (err, t) = c.call("atlas_stop_service", json!({"service": "web"}));
    assert!(!err, "{t}");
    wait_for("web stopped", HANG, || {
        let st = status(&mut c);
        matches!(
            service_state(&st, "web").as_deref(),
            Some("stopped" | "disabled")
        )
        .then_some(())
    });
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", p.web_port)).is_err(),
        "web port must be closed after stop"
    );
    let (err, t) = c.call("atlas_start_service", json!({"service": "web"}));
    assert!(!err, "{t}");
    wait_for("web running again", HANG, || {
        let st = status(&mut c);
        (service_state(&st, "web").as_deref() == Some("running")).then_some(())
    });

    // unknown service lists known names (for every control tool).
    for tool in [
        "atlas_restart_service",
        "atlas_stop_service",
        "atlas_start_service",
    ] {
        let (err, t) = c.call(tool, json!({"service": "ghost"}));
        assert!(err, "{tool}");
        assert!(
            t.contains("unknown service 'ghost'") && t.contains("web") && t.contains("ticker"),
            "{t}"
        );
        let (err, t) = c.call(tool, json!({}));
        assert!(err && t.contains("service"), "{t}");
    }

    // actions
    let (err, t) = c.call("atlas_run_action", json!({"action": "Wipe"}));
    assert!(err, "must refuse without confirmed");
    assert!(
        t.contains("Wipe all data?") && t.contains("confirmed"),
        "{t}"
    );
    let (err, _) = c.call(
        "atlas_run_action",
        json!({"action": "x", "confirmed": false}),
    );
    assert!(err);
    let (err, t) = c.call(
        "atlas_run_action",
        json!({"action": "Wipe", "confirmed": true}),
    );
    assert!(!err, "{t}");
    let (err, t) = c.call("atlas_run_action", json!({"action": "h"}));
    assert!(!err, "{t}");
    assert_eq!(
        serde_json::from_str::<Value>(&t).unwrap()["action"],
        "Hello"
    );
    let (err, t) = c.call("atlas_run_action", json!({"action": "hello"})); // label, case-insens
    assert!(!err || t.contains("already running"), "{t}");
    let (err, t) = c.call("atlas_run_action", json!({"action": "nope"}));
    assert!(err);
    assert!(
        t.contains("unknown action") && t.contains("Hello") && t.contains("Wipe"),
        "{t}"
    );
    let (err, t) = c.call("atlas_run_action", json!({}));
    assert!(err && t.contains("action"), "{t}");

    // Never-ready service: wait_ready gives up at its own timeout. The 2 s is the tool's argument (the feature), the
    // lower bound proves it really waited; the time it took beyond that is not asserted (a hang is caught by the
    // RPC ceiling, `HANG`, of the client).
    let started = Instant::now();
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "never", "timeout_s": 2}),
    );
    assert!(err, "{t}");
    assert!(
        t.contains("timed out after 2s") && t.contains("never="),
        "{t}"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "gave up before its timeout: {:?}",
        started.elapsed()
    );
    let (err, t) = c.call(
        "atlas_wait_ready",
        json!({"service": "ghost", "timeout_s": 1}),
    );
    assert!(err && t.contains("unknown service 'ghost'"), "{t}");
    let (err, _) = c.call("atlas_wait_ready", json!({"timeout_s": "soon"}));
    assert!(err);
}

#[test]
fn action_output_is_observable_and_paused_services_resume() {
    require_python!();
    let p = project();
    let (h, mut c) = start_running(&p, &["web", "ticker"]);

    // Plain action: output reaches atlas's stdout, and the in-flight guard is
    // released afterwards so the same action can run again.
    let (err, t) = c.call("atlas_run_action", json!({"action": "Hello"}));
    assert!(!err, "{t}");
    wait_for("action output", HANG, || {
        h.stdout().contains("hello-from-action").then_some(())
    });
    wait_for("Hello runnable again", HANG, || {
        let (err, _) = c.call("atlas_run_action", json!({"action": "Hello"}));
        (!err).then_some(())
    });

    // Confirmed action runs after confirmed=true only.
    let (err, _) = c.call("atlas_run_action", json!({"action": "Wipe"}));
    assert!(err);
    assert!(!h.stdout().contains("wiped-the-thing"));
    let (err, t) = c.call(
        "atlas_run_action",
        json!({"action": "Wipe", "confirmed": true}),
    );
    assert!(!err, "{t}");
    wait_for("wipe output", HANG, || {
        h.stdout().contains("wiped-the-thing").then_some(())
    });

    // pause_services: web is suspended while the action runs, restarted after.
    let (err, t) = c.call("atlas_run_action", json!({"action": "p"}));
    assert!(!err, "{t}");
    // The same action cannot start twice concurrently.
    let (err, t) = c.call("atlas_run_action", json!({"action": "Pause web"}));
    assert!(err && t.contains("already running"), "{t}");
    wait_for("web suspended", HANG, || {
        let st = status(&mut c);
        (service_state(&st, "web").as_deref() == Some("suspended")).then_some(())
    });
    wait_for("pause action finished", HANG, || {
        h.stdout().contains("pause-finished").then_some(())
    });
    wait_for("web resumed", HANG, || {
        let st = status(&mut c);
        (service_state(&st, "web").as_deref() == Some("running")).then_some(())
    });
    let (err, t) = c.call("atlas_wait_ready", json!({"timeout_s": HANG.as_secs()}));
    assert!(!err, "{t}");
}

#[test]
fn concurrent_calls_are_all_answered_with_correct_ids() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running(&p, &["web", "ticker"]);

    // 10 in flight: mix of reads, a control call and a (short) blocking wait_ready.
    let mut ids = Vec::new();
    for i in 0..10i64 {
        let id = json!(format!("req-{i}"));
        let (tool, args) = match i % 5 {
            0 => ("atlas_status", json!({})),
            1 => ("atlas_logs", json!({"service": "ticker", "lines": 5})),
            2 => ("atlas_config", json!({})),
            3 => (
                "atlas_wait_ready",
                json!({"service": "web", "timeout_s": HANG.as_secs()}),
            ),
            _ => ("atlas_start_service", json!({"service": "ticker"})),
        };
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":args}}));
        ids.push((id, tool));
    }
    // A ping sent behind a blocking wait_ready must not be stuck behind it.
    c.send(&json!({"jsonrpc":"2.0","id":"ping-1","method":"ping"}));
    let r = c.recv_id(&json!("ping-1"));
    assert_eq!(r["result"], json!({}));

    for (id, tool) in ids.into_iter().rev() {
        let r = c.recv_id(&id);
        assert_eq!(r["id"], id);
        let (err, text) = parse_tool(&r);
        assert!(!err, "{tool}: {text}");
    }
}
