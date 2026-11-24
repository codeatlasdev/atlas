//! The control socket of a headless run: replaced when stale, 0600, one instance per project.

use super::*;

#[test]
fn stale_socket_is_replaced_perms_0600_and_malformed_lines_are_survived() {
    require_python!();
    let p = project();
    std::fs::create_dir_all(p.logs()).unwrap();
    let sock = p.logs().join("atlas.sock");
    // A socket file left by a crashed run: bound once, listener dropped, file stays.
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    assert!(sock.exists());
    // A process forked by a parallel test between bind and drop holds the listener
    // fd until its exec (CLOEXEC), so the refusal can take a moment.
    wait_for("stale socket refusing", HANG, || {
        std::os::unix::net::UnixStream::connect(&sock)
            .is_err()
            .then_some(())
    });

    let h = Headless::spawn(p.root());
    wait_for("socket serving", HANG, || {
        std::os::unix::net::UnixStream::connect(&sock).ok()
    });
    let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket mode is {mode:o}");

    let roundtrip = |line: &[u8]| -> Value {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        s.set_read_timeout(Some(HANG)).unwrap();
        s.write_all(line).unwrap();
        s.write_all(b"\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&s).read_line(&mut reply).unwrap();
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("bad reply {reply:?}: {e}"))
    };
    for bad in [
        &b"garbage"[..],
        b"{}",
        b"[1,2,3]",
        b"{\"cmd\":5}",
        b"{\"cmd\":\"nope\"}",
        b"{\"cmd\":\"restart_service\"}",
        b"\xff\xfe\xfd",
        b"{\"cmd\":\"restart_service\",\"args\":{\"service\":\"ghost\"}}",
    ] {
        let r = roundtrip(bad);
        assert_eq!(r["ok"], false, "{:?} → {r}", String::from_utf8_lossy(bad));
        assert!(r["error"].as_str().is_some_and(|e| !e.is_empty()));
    }
    // Oversized line is rejected, not buffered forever / crashed on.
    {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        s.set_read_timeout(Some(HANG)).unwrap();
        // The server answers and closes without draining; a late write may hit EPIPE.
        let _ = s.write_all(&vec![b'a'; 200 * 1024]);
        let _ = s.write_all(b"\n");
        let mut reply = String::new();
        BufReader::new(&s).read_line(&mut reply).unwrap();
        let r: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("too large"), "{r}");
    }
    // Connection that hangs up mid-line must not hurt either.
    {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        s.write_all(b"{\"cmd\":\"restart_serv").unwrap();
    }
    // Atlas is still alive and serving valid requests.
    let r = roundtrip(br#"{"cmd":"start_service","args":{"service":"ticker"}}"#);
    assert_eq!(r["ok"], true, "{r}");
    assert!(
        unsafe { libc_kill(h.pid(), 0) } == 0,
        "atlas must survive malformed control input"
    );

    // Clean shutdown removes the socket.
    let mut h = h;
    h.stop();
    wait_for("socket removed", HANG, || (!sock.exists()).then_some(()));
}

#[test]
fn second_headless_instance_refuses_and_leaves_first_working() {
    require_python!();
    let p = project();
    let (_h, mut c) = start_running(&p, &["web", "ticker"]);
    let out = Command::new(BIN)
        .env("ATLAS_LANG", "en")
        .args(["dev", "--headless", "--dir", p.root().to_str().unwrap()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("already running"));
    // The first instance still owns its socket and services.
    let st = status(&mut c);
    assert_eq!(service_state(&st, "web").as_deref(), Some("running"));
    let (err, t) = c.call("atlas_start_service", json!({"service": "web"}));
    assert!(!err, "{t}");
}
