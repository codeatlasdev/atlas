//! The decision (ranges, listener ownership from a `/proc/net/tcp` text, names) and the proxy with real sockets.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const ME: u32 = 1001;
const OTHER: u32 = 1002;

/// A `/proc/net/tcp` text: one LISTEN row per `(address, port, uid)`, plus the header and a row in each state that is
/// not LISTEN (an open connection and a socket in TIME_WAIT hold a port too, but nothing answers there).
pub(crate) fn tcp(rows: &[(&str, u16, u32)]) -> String {
    let mut t = String::from(
        "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n",
    );
    for (i, (ip, port, uid)) in rows.iter().enumerate() {
        t.push_str(&format!(
            "  {i}: {}:{port:04X} 00000000:0000 0A 00000000:00000000 00:00000000 00000000  {uid}        0 {} 1 0000000000000000 100 0 0 10 0\n",
            hex_ip(ip),
            10_000 + i
        ));
    }
    t.push_str("   9: 0100007F:1770 0100007F:D431 01 00000000:00000000 00:00000000 00000000  1001        0 99 1 0\n");
    t.push_str("  10: 0100007F:1771 0100007F:D432 06 00000000:00000000 00:00000000 00000000  1001        0 0 1 0\n");
    t
}

/// An address the way the kernel prints it (IPv4: one little-endian word; IPv6: four).
fn hex_ip(ip: &str) -> String {
    match ip.parse::<IpAddr>().unwrap() {
        IpAddr::V4(v4) => format!("{:08X}", u32::from(v4).swap_bytes()),
        IpAddr::V6(v6) => {
            let b = u128::from(v6);
            (0..4)
                .map(|i| format!("{:08X}", ((b >> (96 - 32 * i)) as u32).swap_bytes()))
                .collect()
        }
    }
}

fn judged(ip: &str, port: u16, table: &str, local: bool) -> Result<(), String> {
    judge(ip, ip.parse().unwrap(), port, ME, table, local)
}

// Security review 4, C1: the metadata service and every private range a crewmate's firewall chain drops were open
// to a capture, which runs as the crew user outside that chain. The ranges are the chain's (`NEVER`, `NEVER6`).
#[test]
fn private_link_local_metadata_and_cgnat_addresses_are_refused_in_every_notation() {
    for ip in [
        "10.0.0.1",
        "10.255.255.255",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "169.254.0.1",
        "100.64.0.1",
        "100.127.255.255",
        "fc00::1",
        "fd12:3456::1",
        "fe80::1",
        // NAT64 carries IPv4 addresses, the private ones too (security review 5).
        "64:ff9b::a00:1",
        "64:ff9b::a9fe:a9fe",
        // The same address in IPv6 clothes.
        "::ffff:169.254.169.254",
        "::ffff:10.1.2.3",
        "0.0.0.0",
        "::",
        "::ffff:0.0.0.0",
        "255.255.255.255",
        "224.0.0.1",
    ] {
        let why = judged(ip, 80, &tcp(&[]), false).unwrap_err();
        assert!(
            why.contains("private, link-local or metadata"),
            "{ip}: {why}"
        );
    }
    // A listener of the asking user's own does not open a private address: only this machine's ports are decided by owner.
    assert!(judged("10.0.0.5", 5173, &tcp(&[("10.0.0.5", 5173, ME)]), false).is_err());
}

// Security review 5, N2: a connect to `::` lands on `::1` (and to `0.0.0.0` on 127.0.0.1), so the unspecified address
// was a way to reach another crewmate's `[::1]` server while only one's own `0.0.0.0` socket was looked at.
#[test]
fn an_unspecified_address_is_refused_even_when_my_own_wildcard_listener_is_on_the_port() {
    let table = tcp(&[
        ("::1", 5173, OTHER),
        ("0.0.0.0", 5173, ME),
        ("::", 5174, ME),
    ]);
    for ip in ["::", "0.0.0.0"] {
        for local in [true, false] {
            let why = judged(ip, 5173, &table, local).unwrap_err();
            assert!(
                why.contains("private, link-local or metadata"),
                "{ip}: {why}"
            );
        }
        assert!(
            judged(ip, 5174, &table, true).is_err(),
            "{ip} even for my own `::` server"
        );
    }
}

// An IPv4 wildcard socket is IPv4 only: it never stands for an IPv6 address (an IPv6 wildcard may stand for IPv4).
#[test]
fn an_ipv4_wildcard_listener_does_not_cover_an_ipv6_address() {
    let table = tcp(&[("0.0.0.0", 5173, ME), ("::", 5175, ME)]);
    assert!(
        judged("::1", 5173, &table, true).is_err(),
        "::1 is not served by 0.0.0.0"
    );
    assert!(judged("127.0.0.1", 5173, &table, true).is_ok());
    assert!(
        judged("::1", 5175, &table, true).is_ok()
            && judged("127.0.0.1", 5175, &table, true).is_ok()
    );
    // B on `[::1]` only, me on `0.0.0.0`: the very listener table of the attack, asking for ::1 itself.
    let table = tcp(&[("::1", 5173, OTHER), ("0.0.0.0", 5173, ME)]);
    assert!(judged("::1", 5173, &table, true).is_err());
}

#[test]
fn the_edges_of_the_private_ranges_are_public() {
    for ip in [
        "172.15.255.255",
        "172.32.0.1",
        "100.63.255.255",
        "100.128.0.1",
        "11.0.0.1",
        "169.253.255.255",
        "169.255.0.1",
        "192.167.255.255",
        "192.169.0.1",
        "8.8.8.8",
        "2606:4700::1111",
        "fbff::1",
        "fec0::1",
    ] {
        assert_eq!(judged(ip, 443, &tcp(&[]), false), Ok(()), "{ip}");
    }
}

// The decision for this machine's own ports is by who listens: the page of crewmate A may load A's server and nobody
// else's. Rows are real `/proc/net/tcp` lines (see `tcp`): IPv4 words are little-endian, the port is hex.
#[test]
fn a_port_of_this_machine_is_loaded_only_when_every_socket_listening_on_it_is_the_askers() {
    let table = tcp(&[
        ("127.0.0.1", 5173, ME),
        ("127.0.0.1", 5174, OTHER),
        ("0.0.0.0", 8080, OTHER),
        ("::1", 3000, OTHER),
        ("::", 9000, ME),
        ("127.0.0.1", 7000, ME),
        ("127.0.0.1", 7000, OTHER),
    ]);
    let ok = |ip: &str, port| judged(ip, port, &table, true).is_ok();
    assert!(ok("127.0.0.1", 5173), "my own server");
    assert!(!ok("127.0.0.1", 5174), "another crewmate's server");
    assert!(
        !ok("127.0.0.1", 8080),
        "another crewmate's, on every address"
    );
    assert!(!ok("::1", 3000), "another crewmate's, over IPv6");
    assert!(
        ok("::1", 9000) && ok("127.0.0.1", 9000),
        "mine on `::` is reached on both"
    );
    assert!(
        !ok("127.0.0.1", 7000),
        "a port both of us listen on (SO_REUSEPORT) is not only mine"
    );
    assert!(!ok("127.0.0.1", 6000), "nothing listens");
    // An open connection or a TIME_WAIT row of mine on a port is not a server of mine.
    assert!(!ok("127.0.0.1", 0x1770) && !ok("127.0.0.1", 0x1771));
    // The wildcard `0.0.0.0` row of another user covers 127.0.0.2 as well.
    assert!(!ok("127.0.0.2", 8080));
    // This machine's other address (an interface's) is decided the same way, not waved through as "public".
    let table = tcp(&[("203.0.113.5", 3000, OTHER), ("0.0.0.0", 3001, ME)]);
    assert!(judged("203.0.113.5", 3000, &table, true).is_err());
    assert!(judged("203.0.113.5", 3001, &table, true).is_ok());
    // Not this machine's: the table is not asked.
    assert!(judged("203.0.113.5", 3000, &table, false).is_ok());
}

#[test]
fn the_kernels_address_notation_is_read_back_exactly() {
    assert_eq!(parse_hex_ip("0100007F"), Some("127.0.0.1".parse().unwrap()));
    assert_eq!(parse_hex_ip("00000000"), Some("0.0.0.0".parse().unwrap()));
    assert_eq!(parse_hex_ip(&hex_ip("::1")), Some("::1".parse().unwrap()));
    assert_eq!(
        parse_hex_ip(&hex_ip("fe80::dead:beef")),
        Some("fe80::dead:beef".parse().unwrap())
    );
    // A dual-stack socket's address is printed as IPv4-mapped IPv6.
    assert_eq!(
        parse_hex_ip("0000000000000000FFFF00000100007F"),
        Some("127.0.0.1".parse().unwrap())
    );
    assert_eq!(parse_hex_ip("zz"), None);
    let l = listeners(&tcp(&[("127.0.0.1", 5173, ME)]));
    assert_eq!(
        l,
        [Listener {
            ip: "127.0.0.1".parse().unwrap(),
            port: 5173,
            uid: ME
        }]
    );
}

// The URL a check or a proxy request names: userinfo is not the host (`http://127.0.0.1@evil.com` goes to evil.com;
// `http://evil.com@127.0.0.1` to loopback), brackets hold an IPv6 literal, and the scheme gives the port.
#[test]
fn the_host_of_a_url_is_the_one_a_browser_connects_to() {
    let a = |u| authority(u);
    assert_eq!(
        a("http://127.0.0.1:5173/x"),
        Some(("127.0.0.1".into(), 5173))
    );
    assert_eq!(a("https://example.com/"), Some(("example.com".into(), 443)));
    assert_eq!(
        a("http://example.com?q=1"),
        Some(("example.com".into(), 80))
    );
    assert_eq!(
        a("http://evil.com@127.0.0.1:80/"),
        Some(("127.0.0.1".into(), 80))
    );
    assert_eq!(
        a("http://127.0.0.1@example.com/"),
        Some(("example.com".into(), 80))
    );
    assert_eq!(a("http://[::1]:3000/"), Some(("::1".into(), 3000)));
    assert_eq!(a("http://[fe80::1]/"), Some(("fe80::1".into(), 80)));
    assert_eq!(a("HTTP://Localhost/"), Some(("Localhost".into(), 80)));
    assert_eq!(a("file:///etc/passwd"), None);
    assert_eq!(a("http://:80/"), None);
    assert_eq!(a("http://host:notaport/"), None);
}

#[test]
fn an_address_of_this_machine_is_told_from_a_remote_one() {
    assert!(is_local("127.0.0.1".parse().unwrap()));
    assert!(is_local("::1".parse().unwrap()));
    // TEST-NET-3 is on no interface.
    assert!(!is_local("203.0.113.9".parse().unwrap()));
}

async fn check(url: &str, table: &str) -> Result<(), String> {
    let t = table.to_string();
    let (host, port) = authority(url).unwrap();
    decide(&host, port, ME, &(Arc::new(move || t.clone()) as Tables))
        .await
        .map(|_| ())
}

// A hostname is judged by what it resolves to: `localhost` is this machine whatever the URL's spelling.
#[tokio::test]
async fn a_name_that_resolves_to_this_machine_is_judged_as_this_machine() {
    let t = tcp(&[
        ("127.0.0.1", 5173, ME),
        ("::1", 5173, ME),
        ("127.0.0.1", 5174, OTHER),
        ("::1", 5174, OTHER),
    ]);
    assert_eq!(check("http://localhost:5173/", &t).await, Ok(()));
    let why = check("http://localhost:5174/", &t).await.unwrap_err();
    assert!(why.contains("no server of yours"), "{why}");
    let why = check("http://169.254.169.254/latest/meta-data/", &t)
        .await
        .unwrap_err();
    assert!(why.contains("metadata"), "{why}");
    let why = check("http://[::ffff:a9fe:a9fe]/", &t).await.unwrap_err();
    assert!(why.contains("metadata"), "{why}");
    let why = check("http://nosuch.invalid/", &t).await.unwrap_err();
    assert!(why.contains("cannot resolve"), "{why}");
}

// ── the proxy, with real sockets ────────────────────────────────────────────────────────────────────────

/// A server on 127.0.0.1 that answers every request with `reply` (a whole HTTP response) and counts connections.
async fn origin(reply: String) -> (u16, Arc<AtomicUsize>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = Arc::new(AtomicUsize::new(0));
    let n = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut c, _)) = l.accept().await {
            n.fetch_add(1, Ordering::SeqCst);
            let reply = reply.clone();
            tokio::spawn(async move {
                let mut b = [0u8; 4096];
                let _ = c.read(&mut b).await;
                let _ = c.write_all(reply.as_bytes()).await;
                let _ = c.shutdown().await;
            });
        }
    });
    (port, seen)
}

fn ok_reply(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// What a browser does for one `http://` URL through the proxy: `(status, location, body)`.
async fn get_via(proxy: &Guard, url: &str) -> (u16, Option<String>, String) {
    let mut c = TcpStream::connect(("127.0.0.1", proxy.port)).await.unwrap();
    let host = authority(url).unwrap().0;
    c.write_all(
        format!("GET {url} HTTP/1.1\r\nHost: {host}\r\nProxy-Connection: keep-alive\r\n\r\n")
            .as_bytes(),
    )
    .await
    .unwrap();
    let mut raw = String::new();
    let mut buf = Vec::new();
    c.read_to_end(&mut buf).await.unwrap();
    raw.push_str(&String::from_utf8_lossy(&buf));
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let location = head
        .lines()
        .find_map(|l| {
            l.strip_prefix("Location: ")
                .or_else(|| l.strip_prefix("location: "))
        })
        .map(str::to_string);
    (status, location, body.to_string())
}

async fn connect_via(proxy: &Guard, target: &str) -> String {
    let mut c = TcpStream::connect(("127.0.0.1", proxy.port)).await.unwrap();
    c.write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut b = vec![0u8; 4096];
    let n = c.read(&mut b).await.unwrap();
    String::from_utf8_lossy(&b[..n]).into_owned()
}

async fn guard(table: String) -> Guard {
    Guard::start_with(ME, Arc::new(move || table.clone()))
        .await
        .unwrap()
}

#[tokio::test]
async fn the_proxy_carries_a_request_to_the_askers_own_server_and_tunnels_to_it() {
    let (port, seen) = origin(ok_reply("A's own page")).await;
    let g = guard(tcp(&[("127.0.0.1", port, ME)])).await;
    let (status, _, body) = get_via(&g, &format!("http://127.0.0.1:{port}/index.html?x=1")).await;
    assert_eq!((status, body.as_str()), (200, "A's own page"));
    let head = connect_via(&g, &format!("127.0.0.1:{port}")).await;
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(seen.load(Ordering::SeqCst), 2);
}

// Incident: the crew user's capture loaded another crewmate's dev server (and the metadata service). The refusal is
// before any connection: the other server never sees the request.
#[tokio::test]
async fn the_proxy_never_connects_to_another_crewmates_port_or_to_the_metadata_service() {
    let (port, seen) = origin(ok_reply("B's secret")).await;
    let g = guard(tcp(&[("127.0.0.1", port, OTHER)])).await;
    let (status, _, body) = get_via(&g, &format!("http://127.0.0.1:{port}/src/App.tsx")).await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("no server of yours"), "{body}");
    assert!(
        connect_via(&g, &format!("127.0.0.1:{port}"))
            .await
            .starts_with("HTTP/1.1 403")
    );
    // The name `localhost` is the same machine.
    assert_eq!(
        get_via(&g, &format!("http://localhost:{port}/")).await.0,
        403
    );
    let (status, _, body) = get_via(
        &g,
        "http://169.254.169.254/latest/meta-data/iam/security-credentials/",
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert!(
        connect_via(&g, "169.254.169.254:80")
            .await
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        connect_via(&g, "10.0.0.7:443")
            .await
            .starts_with("HTTP/1.1 403")
    );
    assert_eq!(
        seen.load(Ordering::SeqCst),
        0,
        "B's server saw a connection"
    );
}

// Incident (review 4, C1, last step): the URL is allowed, the page is the asker's own, and its server answers with a
// redirect to another crewmate's port. Playwright's `route` is not called for the hop, the browser follows it itself:
// the hop reaches the proxy as one more request and is decided like the first.
#[tokio::test]
async fn a_redirect_from_the_askers_server_to_another_crewmates_port_is_refused_at_the_hop() {
    let (b_port, b_seen) = origin(ok_reply("B's secret")).await;
    let to_b = format!(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{b_port}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let (a_port, a_seen) = origin(to_b).await;
    let g = guard(tcp(&[
        ("127.0.0.1", a_port, ME),
        ("127.0.0.1", b_port, OTHER),
    ]))
    .await;
    let (status, location, _) = get_via(&g, &format!("http://127.0.0.1:{a_port}/start")).await;
    assert_eq!(status, 302, "the first hop is the asker's own");
    let next = location.expect("a Location");
    assert!(next.contains(&b_port.to_string()), "{next}");
    let (status, _, body) = get_via(&g, &next).await;
    assert_eq!(status, 403, "the hop to B's port: {body}");
    assert_eq!(
        (a_seen.load(Ordering::SeqCst), b_seen.load(Ordering::SeqCst)),
        (1, 0)
    );
    // And a redirect to the metadata service.
    let to_meta = "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
    let (m_port, _) = origin(to_meta).await;
    let g = guard(tcp(&[("127.0.0.1", m_port, ME)])).await;
    let (_, location, _) = get_via(&g, &format!("http://127.0.0.1:{m_port}/")).await;
    assert_eq!(get_via(&g, &location.unwrap()).await.0, 403);
}

// The connection goes to the address that was checked, not to a second lookup of the name; and the table is read at
// each request (a server that stops, or one that starts, is seen at once).
#[tokio::test]
async fn the_proxy_reads_who_listens_at_every_request() {
    let (port, _) = origin(ok_reply("hi")).await;
    let t = Arc::new(parking_lot::Mutex::new(tcp(&[("127.0.0.1", port, ME)])));
    let t2 = t.clone();
    let g = Guard::start_with(ME, Arc::new(move || t2.lock().clone()))
        .await
        .unwrap();
    let url = format!("http://127.0.0.1:{port}/");
    assert_eq!(get_via(&g, &url).await.0, 200);
    *t.lock() = tcp(&[("127.0.0.1", port, OTHER)]);
    assert_eq!(get_via(&g, &url).await.0, 403, "the port changed hands");
}

#[tokio::test]
async fn a_request_the_proxy_cannot_read_is_a_400_and_a_dead_target_a_502() {
    let g = guard(tcp(&[])).await;
    let mut c = TcpStream::connect(("127.0.0.1", g.port)).await.unwrap();
    c.write_all(b"GET /not-absolute HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut b = String::new();
    let mut raw = Vec::new();
    c.read_to_end(&mut raw).await.unwrap();
    b.push_str(&String::from_utf8_lossy(&raw));
    assert!(b.starts_with("HTTP/1.1 400"), "{b}");
    // A public address nothing answers on in this sandbox is not the proxy's to judge; the connect error is a 502.
    // (A loopback port of mine with no listener: allowed by the table, refused by the kernel.)
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = l.local_addr().unwrap().port();
    let g = guard(tcp(&[("127.0.0.1", dead, ME)])).await;
    drop(l);
    assert_eq!(
        get_via(&g, &format!("http://127.0.0.1:{dead}/")).await.0,
        502
    );
}

// `Reach::check_url` is the first line: the URL is refused before any browser starts, with the same words.
#[tokio::test]
async fn check_url_refuses_what_the_proxy_would() {
    let r = Reach { uid: u32::MAX - 7 };
    let why = r
        .check_url("http://169.254.169.254/latest/meta-data/")
        .await
        .unwrap_err();
    assert!(why.contains("metadata"), "{why}");
    // Real /proc/net/tcp: this test process listens, as someone who is not `r.uid`.
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let why = r
        .check_url(&format!("http://127.0.0.1:{port}/"))
        .await
        .unwrap_err();
    assert!(why.contains("no server of yours"), "{why}");
    #[cfg(target_os = "linux")]
    {
        // SAFETY: plain getter.
        let me = Reach {
            uid: unsafe { libc::geteuid() },
        };
        assert_eq!(
            me.check_url(&format!("http://127.0.0.1:{port}/")).await,
            Ok(())
        );
    }
}
