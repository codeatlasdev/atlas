//! What the page of an isolated crewmate's capture may reach (security review 4, C1).
//!
//! The runner (`node runner.mjs`) runs as the crew user, outside every crewmate's firewall chain, and loads whatever
//! URL the crewmate gives it, so a crewmate could screenshot another crewmate's dev server on `127.0.0.1`, the cloud
//! metadata service, or any private address of the server's network, and read the picture. Two things stop that, both
//! deciding with [`decide`]:
//!
//! - [`Reach::check_url`] refuses the URL itself before any browser starts (the clear answer an agent reads);
//! - [`Guard`] is a forward proxy the browser is started with, so **every** connection the page makes (its
//!   subresources, scripts, WebSockets and each hop of a redirect, which Playwright's `route` never sees) is decided
//!   again, and connects to the address that was checked: a name that resolves to a private address one moment and a
//!   public one the next (DNS rebinding) connects to the first or to nothing.
//!
//! The rule: an unspecified address (`0.0.0.0`, `::`: a connect to it reaches this machine's loopback) and an address of
//! [`NEVER`]/[`NEVER6`] (private, link-local with the metadata service, CGNAT) are refused;
//! this machine's own addresses (loopback, and any interface's) are reachable only on a port whose listening socket
//! belongs to the asking crewmate's uid (`/proc/net/tcp{,6}`); everything else (the public internet) is allowed.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::spec::Reach;
use crate::crew::spawn::provision::{NEVER, NEVER6};

const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// The longest request head the proxy reads (a browser's are a few KB).
const MAX_HEAD: usize = 32 * 1024;

/// The listening sockets of this machine, as `/proc/net/tcp` and `/proc/net/tcp6` print them (concatenated).
type Tables = Arc<dyn Fn() -> String + Send + Sync>;

/// Test seam (set under `capture::TEST_LOCK`): the listening sockets a real browser run is judged by, in place of this
/// machine's, so one process can play two crewmates' servers.
#[cfg(test)]
pub(crate) static TEST_TABLES: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

fn proc_tables() -> String {
    #[cfg(test)]
    if let Some(t) = TEST_TABLES.lock().clone() {
        return t;
    }
    let mut all = String::new();
    for f in ["/proc/net/tcp", "/proc/net/tcp6"] {
        all.push_str(&std::fs::read_to_string(f).unwrap_or_default());
        all.push('\n');
    }
    all
}

impl Reach {
    /// The reach of crewmate `id`: `Ok(None)` when it is not an isolated crewmate (a server without the helper:
    /// nothing separates it from the crew user, so there is nothing to protect it from), an error when it is one and
    /// its user cannot be told (never an unlimited capture for it). The uid is the owner of the crewmate's home:
    /// provisioning gives it to the crewmate, which cannot give it to anyone else.
    pub fn of(store: &crate::crew::store::CrewStore, id: &str) -> Result<Option<Self>, String> {
        use std::os::unix::fs::MetadataExt;
        let Some(iso) = store.isolated(id) else {
            return Ok(None);
        };
        let home = iso.home();
        std::fs::symlink_metadata(&home)
            .map(|m| Some(Self { uid: m.uid() }))
            .map_err(|e| {
                format!(
                    "capture: cannot tell which user crewmate {id} runs as ({}: {e})",
                    home.display()
                )
            })
    }

    /// Refuse `url` when its host is out of reach: a refusal reads as the reason, an `Ok` as "the browser may try".
    pub async fn check_url(&self, url: &str) -> Result<(), String> {
        let (host, port) = authority(url).ok_or_else(|| format!("capture: '{url}' has no host"))?;
        decide(&host, port, self.uid, &(Arc::new(proc_tables) as Tables))
            .await
            .map(|_| ())
    }
}

/// `(host, port)` of an http(s) URL: userinfo dropped, brackets of an IPv6 literal removed, the scheme's port when
/// there is none.
fn authority(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let default = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let auth = rest.split(['/', '?', '#']).next()?;
    let auth = auth.rsplit('@').next()?;
    split_host_port(auth, default)
}

fn split_host_port(auth: &str, default: u16) -> Option<(String, u16)> {
    if let Some(v6) = auth.strip_prefix('[') {
        let (host, rest) = v6.split_once(']')?;
        let port = match rest.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if rest.is_empty() => default,
            None => return None,
        };
        return Some((host.to_string(), port));
    }
    let (host, port) = match auth.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (auth, default),
    };
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// Resolve `host` and judge every address it has: the first one is what a connection must use (pinned), and a name
/// with any forbidden address is refused as a whole (a browser would try the others).
async fn decide(host: &str, port: u16, uid: u32, tables: &Tables) -> Result<SocketAddr, String> {
    let addrs: Vec<SocketAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => tokio::time::timeout(RESOLVE_TIMEOUT, tokio::net::lookup_host((host, port)))
            .await
            .map_err(|_| format!("capture: resolving {host} took more than {RESOLVE_TIMEOUT:?}"))?
            .map_err(|e| format!("capture: cannot resolve {host}: {e}"))?
            .collect(),
    };
    let first = *addrs
        .first()
        .ok_or_else(|| format!("capture: {host} has no address"))?;
    let table = tables();
    for a in &addrs {
        judge(host, a.ip(), a.port(), uid, &table, is_local(a.ip()))?;
    }
    Ok(first)
}

/// Whether `ip` is an address of this machine (assigned to an interface): bind succeeds only for those.
fn is_local(ip: IpAddr) -> bool {
    ip.is_loopback() || std::net::UdpSocket::bind(SocketAddr::new(ip, 0)).is_ok()
}

/// The decision for one address: see the module's rule. `local` is whether `ip` is an address of this machine.
fn judge(
    host: &str,
    ip: IpAddr,
    port: u16,
    uid: u32,
    tables: &str,
    local: bool,
) -> Result<(), String> {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    let shown = if host == ip.to_string() {
        host.to_string()
    } else {
        format!("{host} ({ip})")
    };
    let private = NEVER.iter().chain(&NEVER6).any(|c| in_cidr(ip, c))
        || ip.is_unspecified()
        || matches!(ip, IpAddr::V4(v4) if v4.octets()[0] == 0 || v4.is_broadcast() || v4.is_multicast())
        || matches!(ip, IpAddr::V6(v6) if v6.is_multicast());
    if private {
        return Err(format!(
            "capture: {shown} is a private, link-local or metadata address: a crewmate's capture does not load other machines of the server's network"
        ));
    }
    if !(local || ip.is_loopback()) {
        return Ok(());
    }
    let here: Vec<u32> = listeners(tables)
        .into_iter()
        .filter(|l| l.port == port && l.covers(ip))
        .map(|l| l.uid)
        .collect();
    if !here.is_empty() && here.iter().all(|u| *u == uid) {
        return Ok(());
    }
    Err(format!(
        "capture: {shown}, port {port} is this server's, and no server of yours listens on it: a crewmate's capture loads only a server its own user started (another crewmate's server is refused)"
    ))
}

/// `ip` inside `cidr` (`a.b.c.d/n` or `x::/n`); a malformed range matches nothing.
fn in_cidr(ip: IpAddr, cidr: &str) -> bool {
    let Some((net, bits)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(net), Ok(bits)) = (net.parse::<IpAddr>(), bits.parse::<u32>()) else {
        return false;
    };
    let (a, b, width) = match (ip, net) {
        (IpAddr::V4(a), IpAddr::V4(b)) => (u128::from(u32::from(a)), u128::from(u32::from(b)), 32),
        (IpAddr::V6(a), IpAddr::V6(b)) => (u128::from(a), u128::from(b), 128),
        _ => return false,
    };
    if bits > width {
        return false;
    }
    let shift = width - bits;
    shift == width || a >> shift == b >> shift
}

/// One listening TCP socket of `/proc/net/tcp{,6}`.
#[derive(Debug, PartialEq, Eq)]
struct Listener {
    ip: IpAddr,
    port: u16,
    uid: u32,
}

impl Listener {
    /// Whether a connection to `ip` on this socket's port reaches it: bound to that address, or to every address of
    /// its family (an IPv6 wildcard also takes IPv4, never the reverse: an IPv4 socket cannot answer `::1`). `ip` is
    /// never unspecified here (`judge` refuses those: a connect to `::` goes to `::1`).
    fn covers(&self, ip: IpAddr) -> bool {
        self.ip == ip || (self.ip.is_unspecified() && (self.ip.is_ipv6() || ip.is_ipv4()))
    }
}

/// The sockets in state LISTEN (`0A`) of a `/proc/net/tcp` or `tcp6` text; lines that do not parse are skipped.
fn listeners(text: &str) -> Vec<Listener> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 8 || f[3] != "0A" {
                return None;
            }
            let (addr, port) = f[1].split_once(':')?;
            Some(Listener {
                ip: parse_hex_ip(addr)?,
                port: u16::from_str_radix(port, 16).ok()?,
                uid: f[7].parse().ok()?,
            })
        })
        .collect()
}

/// An address as the kernel prints it: IPv4 as one little-endian word, IPv6 as four of them. An IPv4-mapped IPv6
/// address (a dual-stack socket's) is the IPv4 one.
fn parse_hex_ip(s: &str) -> Option<IpAddr> {
    let word = |h: &str| u32::from_str_radix(h, 16).ok().map(u32::swap_bytes);
    match s.len() {
        8 => Some(IpAddr::V4(Ipv4Addr::from(word(s)?))),
        32 => {
            let mut bits = 0u128;
            for i in 0..4 {
                bits = bits << 32 | u128::from(word(&s[i * 8..i * 8 + 8])?);
            }
            let v6 = Ipv6Addr::from(bits);
            Some(v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4))
        }
        _ => None,
    }
}

// ── the proxy ───────────────────────────────────────────────────────────────────────────────────────────

/// A forward proxy on this machine's loopback that connects only where [`decide`] allows, for as long as it lives.
pub struct Guard {
    port: u16,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Guard {
    pub async fn start(reach: &Reach) -> std::io::Result<Self> {
        Self::start_with(reach.uid, Arc::new(proc_tables)).await
    }

    async fn start_with(uid: u32, tables: Tables) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let task = tokio::spawn(async move {
            while let Ok((conn, _)) = listener.accept().await {
                let tables = tables.clone();
                tokio::spawn(async move {
                    let _ = serve(conn, uid, tables).await;
                });
            }
        });
        Ok(Self { port, task })
    }

    /// What the browser is started with (`proxy.server`).
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// One proxied connection. HTTPS and WebSockets arrive as `CONNECT host:port` and are tunnelled; plain HTTP arrives in
/// absolute form and is sent on as one request per connection (`Connection: close`), so each request, and each hop of
/// a redirect, is decided on its own.
async fn serve(mut c: TcpStream, uid: u32, tables: Tables) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > MAX_HEAD {
            return refuse(&mut c, "400 Bad Request", "request head too large").await;
        }
        let mut chunk = [0u8; 4096];
        let n = c.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split(' ');
    let (method, target) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let version = parts.next().unwrap_or("HTTP/1.1");

    let tunnel = method.eq_ignore_ascii_case("CONNECT");
    let (host, port) = if tunnel {
        split_host_port(target, 443)
    } else {
        authority(target)
    }
    .unwrap_or_default();
    if host.is_empty() {
        return refuse(
            &mut c,
            "400 Bad Request",
            "this proxy takes CONNECT and absolute http:// requests",
        )
        .await;
    }
    let addr = match decide(&host, port, uid, &tables).await {
        Ok(a) => a,
        Err(why) => return refuse(&mut c, "403 Forbidden", &why).await,
    };
    let mut up = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(s)) => s,
        _ => {
            return refuse(
                &mut c,
                "502 Bad Gateway",
                &format!("cannot connect to {host}:{port}"),
            )
            .await;
        }
    };
    if tunnel {
        c.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
    } else {
        // Origin form, no hop-by-hop headers, one request per connection.
        let path = target
            .split_once("://")
            .and_then(|(_, r)| r.find(['/', '?']).map(|i| &r[i..]))
            .map(|p| {
                if p.starts_with('/') {
                    p.to_string()
                } else {
                    format!("/{p}")
                }
            })
            .unwrap_or_else(|| "/".into());
        let mut out = format!("{method} {path} {version}\r\n");
        for l in lines.filter(|l| !l.is_empty()) {
            let name = l.split(':').next().unwrap_or_default().to_ascii_lowercase();
            if !matches!(
                name.as_str(),
                "proxy-connection" | "proxy-authorization" | "connection" | "keep-alive"
            ) {
                out.push_str(l);
                out.push_str("\r\n");
            }
        }
        out.push_str("Connection: close\r\n\r\n");
        up.write_all(out.as_bytes()).await?;
    }
    // What came after the head belongs to the upstream (a request body, a tunnel's first bytes).
    up.write_all(&buf[end..]).await?;
    tokio::io::copy_bidirectional(&mut c, &mut up).await?;
    Ok(())
}

async fn refuse(c: &mut TcpStream, status: &str, why: &str) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{why}",
        why.len()
    );
    c.write_all(resp.as_bytes()).await?;
    c.shutdown().await
}

#[cfg(test)]
pub(crate) mod tests;
