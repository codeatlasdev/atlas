//! Live preview server for HTML files: a tiny static HTTP/1.1 server on `127.0.0.1`
//! (ephemeral port, one per root directory) plus live reload over Server-Sent Events.
//!
//! Served HTML gets a small script injected before `</body>`; it listens on
//! `/__atlas_live` and, when a file under the root changes, either hot-swaps the
//! stylesheets (CSS-only change) or reloads the page. A server stops when nobody has
//! touched it (`live_open` / `live_touch`, sent by the open preview tabs) for [`IDLE`].
//!
//! Commands: `live_open {path}` → `{url, root}`, `live_touch {root}`, `live_stop {root}`.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, broadcast};

/// A server nobody touched for this long shuts down.
const IDLE: Duration = Duration::from_secs(120);
/// How often the reaper checks.
const REAP_EVERY: Duration = Duration::from_secs(15);
/// Quiet period that coalesces a burst of writes (editors save via tmp + rename) into one event.
const DEBOUNCE: Duration = Duration::from_millis(80);
/// SSE keep-alive comment, so proxies/webviews never see a silent connection.
const SSE_PING: Duration = Duration::from_secs(20);
/// Request head cap (request line + headers).
const MAX_HEAD: usize = 16 * 1024;
const MAX_FILE: u64 = 256 * 1024 * 1024;
/// Never reload for changes inside these directories.
const SKIP_DIRS: [&str; 5] = [".git", "node_modules", "target", ".logs", ".DS_Store"];
const EVENTS_PATH: &str = "/__atlas_live";

/// Page script: one EventSource; `css` swaps `<link rel=stylesheet>` hrefs, `reload` reloads.
const CLIENT_JS: &str = r#"<script>(function(){var es=new EventSource("/__atlas_live");es.onmessage=function(e){if(e.data==="css"){var l=document.querySelectorAll('link[rel~="stylesheet"]');for(var i=0;i<l.length;i++){var u=new URL(l[i].href);if(u.origin!==location.origin)continue;u.searchParams.set("__live",Date.now());l[i].href=u.href}}else if(e.data==="reload"){location.reload()}}})();</script>"#;

/// What a burst of changed paths means for the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reload {
    /// Only stylesheets changed: swap them in place.
    Css,
    /// Anything else: full reload.
    Full,
}

impl Reload {
    fn wire(self) -> &'static str {
        match self {
            Reload::Css => "css",
            Reload::Full => "reload",
        }
    }
}

/// Reload kind for changed `paths`, or `None` when none of them matter (ignored dirs only).
pub fn reload_kind<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<Reload> {
    let mut kind = None;
    for p in paths {
        if p.components()
            .any(|c| matches!(c, Component::Normal(n) if SKIP_DIRS.iter().any(|s| n == *s)))
        {
            continue;
        }
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("css")) {
            kind.get_or_insert(Reload::Css);
        } else {
            return Some(Reload::Full);
        }
    }
    kind
}

/// `Content-Type` for a file, by extension.
pub fn mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs" | "cjs") => "text/javascript; charset=utf-8",
        Some("json" | "map" | "webmanifest") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("pdf") => "application/pdf",
        Some("xml") => "application/xml; charset=utf-8",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `html` with the live-reload script before the last `</body>` (case-insensitive), or
/// appended when the document has none.
pub fn inject(html: &str) -> String {
    let at = html
        .to_ascii_lowercase()
        .rfind("</body>")
        .unwrap_or(html.len());
    let mut out = String::with_capacity(html.len() + CLIENT_JS.len());
    out.push_str(&html[..at]);
    out.push_str(CLIENT_JS);
    out.push_str(&html[at..]);
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Map a request target (`/a/b.html?x#y`) to a file under `root`, or `None` when it would
/// leave the root (`..`, absolute components, NUL, a symlink pointing outside) or does not
/// exist. Directories resolve to their `index.html`.
pub fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let raw = target.split(['?', '#']).next()?;
    let decoded = percent_decode(raw)?;
    if decoded.contains('\0') {
        return None;
    }
    let mut p = root.to_path_buf();
    for part in decoded.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            part if Path::new(part).components().count() != 1 => return None,
            part => p.push(part),
        }
    }
    if p.is_dir() {
        p.push("index.html");
    }
    // Canonicalize resolves symlinks: a link out of the root must not be served.
    let canon = p.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    (canon.starts_with(&root) && canon.is_file()).then_some(canon)
}

/// Server root for an HTML file: the nearest ancestor with `index.html` or `package.json`,
/// bounded by `worktree` when given (never above it), else the file's own folder.
pub fn root_for(file: &Path, worktree: Option<&Path>) -> PathBuf {
    let dir = file.parent().unwrap_or(file);
    for d in dir.ancestors() {
        if d.join("index.html").is_file() || d.join("package.json").is_file() {
            return d.to_path_buf();
        }
        if worktree.is_some_and(|w| d == w) {
            break;
        }
    }
    dir.to_path_buf()
}

struct Server {
    port: u16,
    last_used: Mutex<Instant>,
    stop: Notify,
    // Dropping it stops the watcher.
    _watcher: Option<notify::RecommendedWatcher>,
}

/// Tauri-managed registry: canonical root → running server.
#[derive(Default)]
pub struct LiveServers {
    servers: Mutex<HashMap<PathBuf, Arc<Server>>>,
    /// Serializes `ensure` so two racing opens of one root bind a single server.
    start: tokio::sync::Mutex<()>,
}

impl LiveServers {
    fn get(&self, root: &Path) -> Option<Arc<Server>> {
        self.servers.lock().get(root).cloned()
    }

    async fn ensure(self: &Arc<Self>, root: &Path) -> Result<Arc<Server>, String> {
        let _g = self.start.lock().await;
        if let Some(s) = self.get(root) {
            *s.last_used.lock() = Instant::now();
            return Ok(s);
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| format!("live server: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (tx, _) = broadcast::channel::<&'static str>(16);
        let watcher = watch(root, tx.clone());
        let server = Arc::new(Server {
            port,
            last_used: Mutex::new(Instant::now()),
            stop: Notify::new(),
            _watcher: watcher,
        });
        self.servers
            .lock()
            .insert(root.to_path_buf(), server.clone());
        let (srv, root_buf) = (server.clone(), root.to_path_buf());
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            accept_loop(listener, root_buf.clone(), tx, &srv).await;
            let mut map = this.servers.lock();
            if map.get(&root_buf).is_some_and(|s| Arc::ptr_eq(s, &srv)) {
                map.remove(&root_buf);
            }
        });
        Ok(server)
    }

    fn stop(&self, root: &Path) -> bool {
        let s = self.servers.lock().remove(root);
        s.inspect(|s| s.stop.notify_one()).is_some()
    }

    fn reap(&self) {
        let idle: Vec<PathBuf> = self
            .servers
            .lock()
            .iter()
            .filter(|(_, s)| s.last_used.lock().elapsed() >= IDLE)
            .map(|(r, _)| r.clone())
            .collect();
        for r in idle {
            self.stop(&r);
        }
    }

    /// Spawn the idle reaper; call once at startup.
    pub fn start_reaper(self: &Arc<Self>) {
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut t = tokio::time::interval(REAP_EVERY);
            loop {
                t.tick().await;
                this.reap();
            }
        });
    }
}

/// Recursive watcher on `root` that debounces bursts into one broadcast. `None` when the
/// platform watcher cannot start (the page then just never auto-reloads).
fn watch(root: &Path, tx: broadcast::Sender<&'static str>) -> Option<notify::RecommendedWatcher> {
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<Reload>();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, notify::EventKind::Access(_)) {
            return;
        }
        if let Some(k) = reload_kind(ev.paths.iter().map(PathBuf::as_path)) {
            let _ = ev_tx.send(k);
        }
    })
    .ok()?;
    w.watch(root, RecursiveMode::Recursive).ok()?;
    tauri::async_runtime::spawn(async move {
        // Ends when the watcher (sender) is dropped.
        while let Some(first) = ev_rx.recv().await {
            let mut kind = first;
            while let Ok(Some(k)) = tokio::time::timeout(DEBOUNCE, ev_rx.recv()).await {
                if k == Reload::Full {
                    kind = Reload::Full;
                }
            }
            let _ = tx.send(kind.wire());
        }
    });
    Some(w)
}

async fn accept_loop(
    listener: TcpListener,
    root: PathBuf,
    tx: broadcast::Sender<&'static str>,
    server: &Arc<Server>,
) {
    loop {
        tokio::select! {
            _ = server.stop.notified() => return,
            r = listener.accept() => {
                let Ok((stream, _)) = r else { continue };
                let (root, rx, server) = (root.clone(), tx.subscribe(), server.clone());
                tauri::async_runtime::spawn(async move {
                    let _ = serve_conn(stream, &root, rx, &server).await;
                });
            }
        }
    }
}

async fn respond(
    s: &mut TcpStream,
    status: &str,
    ctype: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).await?;
    if !head_only {
        s.write_all(body).await?;
    }
    s.flush().await
}

async fn serve_conn(
    stream: TcpStream,
    root: &Path,
    mut rx: broadcast::Receiver<&'static str>,
    server: &Server,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.take(MAX_HEAD as u64));
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    // Drain headers; none matter (no keep-alive, no ranges).
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).await? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
    }
    let mut s = reader.into_inner().into_inner();
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !matches!(method, "GET" | "HEAD") {
        return respond(
            &mut s,
            "405 Method Not Allowed",
            "text/plain",
            b"method not allowed",
            false,
        )
        .await;
    }
    let head_only = method == "HEAD";
    *server.last_used.lock() = Instant::now();

    if target.split('?').next() == Some(EVENTS_PATH) {
        s.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n: ok\n\n",
        )
        .await?;
        loop {
            let msg = tokio::select! {
                m = rx.recv() => match m {
                    Ok(m) => m,
                    // Lagged: we missed events, reload to be safe.
                    Err(broadcast::error::RecvError::Lagged(_)) => Reload::Full.wire(),
                    Err(_) => return Ok(()),
                },
                _ = tokio::time::sleep(SSE_PING) => {
                    s.write_all(b": ping\n\n").await?;
                    continue;
                }
            };
            s.write_all(format!("data: {msg}\n\n").as_bytes()).await?;
            s.flush().await?;
        }
    }

    let Some(file) = resolve(root, target) else {
        return respond(
            &mut s,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found",
            head_only,
        )
        .await;
    };
    let ctype = mime(&file);
    if tokio::fs::metadata(&file).await.map_or(0, |m| m.len()) > MAX_FILE {
        return respond(
            &mut s,
            "413 Payload Too Large",
            "text/plain",
            b"too large",
            head_only,
        )
        .await;
    }
    let Ok(bytes) = tokio::fs::read(&file).await else {
        return respond(
            &mut s,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found",
            head_only,
        )
        .await;
    };
    let body = if ctype.starts_with("text/html") {
        match String::from_utf8(bytes) {
            Ok(h) => inject(&h).into_bytes(),
            Err(e) => e.into_bytes(),
        }
    } else {
        bytes
    };
    respond(&mut s, "200 OK", ctype, &body, head_only).await
}

/// Percent-encode a path for use inside a URL (keeps `/`).
fn encode_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Start (or reuse) the server for the HTML file `path`; returns `{url, root}`.
/// `worktree` bounds the root search so a preview never serves above its checkout.
#[tauri::command]
pub async fn live_open(
    servers: tauri::State<'_, Arc<LiveServers>>,
    path: String,
    worktree: Option<String>,
) -> Result<Value, String> {
    let file = PathBuf::from(&path)
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    if !file.is_file() {
        return Err(format!("{path}: not a file"));
    }
    let wt = worktree.and_then(|w| PathBuf::from(w).canonicalize().ok());
    let root = root_for(&file, wt.as_deref().filter(|w| file.starts_with(w)));
    let server = servers.ensure(&root).await?;
    let rel = file
        .strip_prefix(&root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(json!({
        "url": format!("http://127.0.0.1:{}/{}", server.port, encode_path(&rel)),
        "root": root.to_string_lossy(),
    }))
}

/// Keep a server alive (sent periodically while a preview tab exists).
#[tauri::command]
pub fn live_touch(servers: tauri::State<'_, Arc<LiveServers>>, root: String) -> bool {
    servers.get(Path::new(&root)).is_some_and(|s| {
        *s.last_used.lock() = Instant::now();
        true
    })
}

/// Stop the server of `root` now. `false` when none was running.
#[tauri::command]
pub fn live_stop(servers: tauri::State<'_, Arc<LiveServers>>, root: String) -> bool {
    servers.stop(Path::new(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::create_dir_all(r.join("site/sub")).unwrap();
        std::fs::write(r.join("secret.txt"), "top secret").unwrap();
        std::fs::write(r.join("site/index.html"), "<p>hi</p>").unwrap();
        std::fs::write(r.join("site/sub/index.html"), "<p>sub</p>").unwrap();
        std::fs::write(r.join("site/a b.css"), "a{}").unwrap();
        d
    }

    #[test]
    fn traversal_is_rejected() {
        let d = tree();
        let root = d.path().join("site");
        for t in [
            "/../secret.txt",
            "/sub/../../secret.txt",
            "/%2e%2e/secret.txt",
            "/%2E%2E%2Fsecret.txt",
            "/..%5Csecret.txt",
            "/sub/%2e%2e/%2e%2e/secret.txt",
            "/\0x",
            "/%00",
            "//etc/passwd",
            "/%zz",
        ] {
            assert_eq!(resolve(&root, t), None, "{t}");
        }
    }

    // Unix symlinks (Windows needs Developer Mode to make one).
    #[cfg(unix)]
    #[test]
    fn symlink_out_of_root_is_rejected() {
        let d = tree();
        let root = d.path().join("site");
        std::os::unix::fs::symlink(d.path().join("secret.txt"), root.join("leak.txt")).unwrap();
        assert_eq!(resolve(&root, "/leak.txt"), None);
    }

    #[test]
    fn files_directories_and_encoded_names_resolve() {
        let d = tree();
        let root = d.path().join("site").canonicalize().unwrap();
        assert_eq!(resolve(&root, "/"), Some(root.join("index.html")));
        assert_eq!(resolve(&root, "/sub/"), Some(root.join("sub/index.html")));
        assert_eq!(
            resolve(&root, "/sub?x=1#y"),
            Some(root.join("sub/index.html"))
        );
        assert_eq!(resolve(&root, "/a%20b.css"), Some(root.join("a b.css")));
        assert_eq!(resolve(&root, "/missing.html"), None);
    }

    #[test]
    fn mime_by_extension() {
        assert_eq!(mime(Path::new("a.HTML")), "text/html; charset=utf-8");
        assert_eq!(mime(Path::new("a.css")), "text/css; charset=utf-8");
        assert_eq!(mime(Path::new("a.mjs")), "text/javascript; charset=utf-8");
        assert_eq!(mime(Path::new("a.svg")), "image/svg+xml");
        assert_eq!(mime(Path::new("a.woff2")), "font/woff2");
        assert_eq!(mime(Path::new("a.wasm")), "application/wasm");
        assert_eq!(mime(Path::new("a.bin")), "application/octet-stream");
        assert_eq!(mime(Path::new("noext")), "application/octet-stream");
    }

    #[test]
    fn script_goes_before_the_last_body_close() {
        let out = inject("<html><body><p>x</p></BODY></html>");
        let at = out.find(CLIENT_JS).unwrap();
        assert!(out[..at].ends_with("<p>x</p>"));
        assert!(out[at + CLIENT_JS.len()..].starts_with("</BODY>"));
        // A `</body>` inside a script string earlier in the page is not the target.
        let out = inject("<script>var s='</body>'</script><body></body>");
        assert_eq!(out.matches(CLIENT_JS).count(), 1);
        assert!(out.ends_with(&format!("{CLIENT_JS}</body>")));
    }

    #[test]
    fn script_is_appended_without_a_body_close() {
        let out = inject("<h1>fragment</h1>");
        assert_eq!(out, format!("<h1>fragment</h1>{CLIENT_JS}"));
        assert_eq!(inject(""), CLIENT_JS);
    }

    #[test]
    fn css_only_changes_hot_swap_everything_else_reloads() {
        let p = |s: &'static str| Path::new(s);
        assert_eq!(reload_kind([p("/r/a.css")]), Some(Reload::Css));
        assert_eq!(
            reload_kind([p("/r/a.CSS"), p("/r/b.css")]),
            Some(Reload::Css)
        );
        assert_eq!(
            reload_kind([p("/r/a.css"), p("/r/i.html")]),
            Some(Reload::Full)
        );
        assert_eq!(reload_kind([p("/r/app.js")]), Some(Reload::Full));
        assert_eq!(reload_kind([p("/r/dir")]), Some(Reload::Full));
        assert_eq!(reload_kind([p("/r/node_modules/x/y.js")]), None);
        assert_eq!(
            reload_kind([p("/r/.git/index"), p("/r/a.css")]),
            Some(Reload::Css)
        );
        assert_eq!(reload_kind(std::iter::empty()), None);
    }

    #[test]
    fn root_is_the_nearest_marker_folder_bounded_by_the_worktree() {
        let d = tempfile::tempdir().unwrap();
        let wt = d.path().canonicalize().unwrap();
        std::fs::create_dir_all(wt.join("web/pages")).unwrap();
        std::fs::write(wt.join("web/package.json"), "{}").unwrap();
        std::fs::write(wt.join("web/pages/a.html"), "").unwrap();
        assert_eq!(
            root_for(&wt.join("web/pages/a.html"), Some(&wt)),
            wt.join("web")
        );
        std::fs::create_dir_all(wt.join("docs")).unwrap();
        std::fs::write(wt.join("docs/b.html"), "").unwrap();
        // No marker below the worktree: the file's own folder.
        assert_eq!(
            root_for(&wt.join("docs/b.html"), Some(&wt)),
            wt.join("docs")
        );
    }

    #[test]
    fn encode_path_keeps_slashes_and_escapes_the_rest() {
        assert_eq!(encode_path("a b/ç.html"), "a%20b/%C3%A7.html");
    }

    async fn get(port: u16, target: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).await.unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    #[tokio::test]
    async fn serves_injected_html_rejects_traversal_and_streams_events() {
        let d = tree();
        let root = d.path().join("site").canonicalize().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, _) = broadcast::channel(4);
        let server = Arc::new(Server {
            port,
            last_used: Mutex::new(Instant::now()),
            stop: Notify::new(),
            _watcher: None,
        });
        let (r, t, s) = (root.clone(), tx.clone(), server.clone());
        let task = tokio::spawn(async move { accept_loop(listener, r, t, &s).await });

        let page = get(port, "/").await;
        assert!(page.starts_with("HTTP/1.1 200 OK"));
        assert!(page.contains("Content-Type: text/html; charset=utf-8"));
        assert!(page.contains("<p>hi</p>") && page.contains("/__atlas_live"));
        assert!(
            get(port, "/a%20b.css")
                .await
                .contains("Content-Type: text/css")
        );
        assert!(
            get(port, "/../secret.txt")
                .await
                .starts_with("HTTP/1.1 404")
        );
        assert!(
            get(port, "/%2e%2e/secret.txt")
                .await
                .starts_with("HTTP/1.1 404")
        );

        let mut ev = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        ev.write_all(b"GET /__atlas_live HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let mut buf = vec![0u8; 512];
        let n = ev.read(&mut buf).await.unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).contains("text/event-stream"));
        // Wait for the subscriber to exist, then broadcast.
        tokio::time::timeout(Duration::from_secs(2), async {
            while tx.receiver_count() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tx.send("css").unwrap();
        let n = tokio::time::timeout(Duration::from_secs(2), ev.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&buf[..n]), "data: css\n\n");

        server.stop.notify_one();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn watcher_broadcasts_css_then_full() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let (tx, mut rx) = broadcast::channel(8);
        let _w = watch(&root, tx).expect("watcher");
        // Timing workaround, root cause unproven: the watcher can report the tempdir itself late under
        // load (a directory path is a Full reload), which flaked the v1.5.0-alpha.1 release build.
        // Drain until quiet, at most 3 s, then expect only the events this test causes.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let quiet_by = tokio::time::Instant::now() + Duration::from_secs(3);
        while tokio::time::Instant::now() < quiet_by
            && tokio::time::timeout(DEBOUNCE * 3, rx.recv()).await.is_ok()
        {}
        std::fs::write(root.join("a.css"), "a{}").unwrap();
        let m = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(m, "css");
        std::fs::write(root.join("index.html"), "x").unwrap();
        let m = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(m, "reload");
    }
}
