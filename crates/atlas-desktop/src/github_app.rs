//! The loopback half of GitHub's "register a GitHub App from a manifest" flow (#643).
//!
//! GitHub wants a browser form POST of the manifest and answers with a redirect to `redirect_url?code=..&state=..`.
//! The page (`apps/desktop`) asks for a one-shot listener here, opens the form page it serves in the user's browser
//! (`open_url`), and waits for the code:
//!
//! - `github_app_listen_start {manifest, post_url}` → `{id, url, expires_in_secs}`. Binds `127.0.0.1:<random>`, adds
//!   `redirect_url` (this listener's `/callback`) to the manifest and a random `state` to `post_url`.
//! - `github_app_listen_wait {id}` → `{status: "code", code}`, `{status: "timed_out"}` or `{status: "cancelled"}`.
//! - `github_app_listen_cancel {id}`.
//!
//! The listener is single use and short lived: it serves the form page once (the path carries a random token),
//! accepts one `/callback` whose `state` matches (a wrong one is refused and the listener keeps waiting), then closes;
//! after [`WINDOW`] it closes anyway. It answers only requests whose `Host` is its own address, so a web page cannot
//! reach it through DNS rebinding. The code goes to the app only; `crew_github_app_convert` spends it on the crew
//! server, which keeps the key. Nothing here is written to disk or logged.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use atlas_i18n::t;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc, oneshot};

/// How long the listener waits for GitHub to redirect back.
const WINDOW: Duration = Duration::from_secs(10 * 60);
/// A connection that has not sent its request head by now is dropped (browsers open speculative connections).
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Request head cap (request line + headers).
const MAX_HEAD: usize = 8 * 1024;
/// GitHub's codes are hex; the same bound hostd's `crew_github_app_convert` applies.
const MAX_CODE: usize = 128;

/// How a listener ended.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Code { code: String },
    TimedOut,
    Cancelled,
}

/// What `github_app_listen_start` answers.
#[derive(Serialize)]
pub struct Started {
    id: String,
    /// The form page to open in the browser.
    url: String,
    expires_in_secs: u64,
}

struct Session {
    form_url: String,
    cancel: Arc<Notify>,
    done: oneshot::Receiver<Outcome>,
}

struct Flow {
    cancel: Arc<Notify>,
    done: Option<oneshot::Receiver<Outcome>>,
}

/// The open listeners by id. A new `start` closes the older ones: one app is created at a time.
static FLOWS: LazyLock<Mutex<HashMap<String, Flow>>> = LazyLock::new(Mutex::default);

#[tauri::command]
pub async fn github_app_listen_start(manifest: Value, post_url: String) -> Result<Started, String> {
    let s = start_flow(manifest, &post_url, WINDOW).await?;
    let id = atlas_host::paths::random_hex(8).map_err(|e| e.to_string())?;
    let mut flows = FLOWS.lock();
    for (_, old) in flows.drain() {
        old.cancel.notify_one();
    }
    flows.insert(
        id.clone(),
        Flow {
            cancel: s.cancel,
            done: Some(s.done),
        },
    );
    Ok(Started {
        id,
        url: s.form_url,
        expires_in_secs: WINDOW.as_secs(),
    })
}

/// Resolves when the listener ends. Once per listener: a second wait finds no session.
#[tauri::command]
pub async fn github_app_listen_wait(id: String) -> Result<Outcome, String> {
    let done = FLOWS
        .lock()
        .get_mut(&id)
        .and_then(|f| f.done.take())
        .ok_or_else(|| t!("native_crew_github_app_no_session"))?;
    let out = done.await.unwrap_or(Outcome::Cancelled);
    FLOWS.lock().remove(&id);
    Ok(out)
}

/// Close the listener now; a wait in progress answers `cancelled`. Closing one that is gone is not an error.
#[tauri::command]
pub fn github_app_listen_cancel(id: String) {
    if let Some(f) = FLOWS.lock().remove(&id) {
        f.cancel.notify_one();
    }
}

/// `post_url` as a URL when it is GitHub's own "new app" page (an account's or an organization's), nothing else:
/// the manifest comes from the crew server and the browser is sent wherever this says.
fn github_target(post_url: &str) -> Option<url::Url> {
    let u = url::Url::parse(post_url).ok()?;
    (u.scheme() == "https"
        && u.host_str() == Some("github.com")
        && u.port().is_none()
        && u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none()
        && u.path().ends_with("/settings/apps/new"))
    .then_some(u)
}

async fn start_flow(
    mut manifest: Value,
    post_url: &str,
    window: Duration,
) -> Result<Session, String> {
    let mut action = github_target(post_url)
        .ok_or_else(|| t!("native_crew_github_app_bad_target", url = post_url))?;
    let Some(fields) = manifest.as_object_mut() else {
        return Err(t!("native_crew_github_app_bad_manifest"));
    };
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let state = atlas_host::paths::random_hex(16).map_err(|e| e.to_string())?;
    let token = atlas_host::paths::random_hex(16).map_err(|e| e.to_string())?;
    fields.insert(
        "redirect_url".into(),
        Value::String(format!("http://127.0.0.1:{port}/callback")),
    );
    action.query_pairs_mut().append_pair("state", &state);

    let (tx, rx) = mpsc::channel(1);
    let ctx = Arc::new(Ctx {
        port,
        state,
        token: token.clone(),
        form: form_page(action.as_str(), &manifest.to_string()),
        form_served: AtomicBool::new(false),
        tx,
    });
    let cancel = Arc::new(Notify::new());
    let (done_tx, done) = oneshot::channel();
    let stop = cancel.clone();
    tokio::spawn(async move {
        let _ = done_tx.send(serve(listener, ctx, rx, window, stop).await);
    });
    Ok(Session {
        form_url: format!("http://127.0.0.1:{port}/start/{token}"),
        cancel,
        done,
    })
}

struct Ctx {
    port: u16,
    state: String,
    /// The path segment that fetches the form page.
    token: String,
    form: String,
    form_served: AtomicBool,
    /// The one slot for the code: a second valid callback finds it full.
    tx: mpsc::Sender<String>,
}

async fn serve(
    listener: TcpListener,
    ctx: Arc<Ctx>,
    mut codes: mpsc::Receiver<String>,
    window: Duration,
    cancel: Arc<Notify>,
) -> Outcome {
    let deadline = tokio::time::sleep(window);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    tokio::spawn(handle(stream, ctx.clone()));
                }
            }
            Some(code) = codes.recv() => return Outcome::Code { code },
            () = &mut deadline => return Outcome::TimedOut,
            () = cancel.notified() => return Outcome::Cancelled,
        }
    }
}

async fn handle(mut stream: TcpStream, ctx: Arc<Ctx>) {
    let Ok(Ok(head)) = tokio::time::timeout(READ_TIMEOUT, read_head(&mut stream)).await else {
        return;
    };
    let (status, headers, body) = match parse_request(&head) {
        Some(req) => route(&ctx, &req),
        None => refused(400),
    };
    let reply = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\n\
         {headers}Connection: close\r\n\r\n{body}",
        reason(status),
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn read_head(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < MAX_HEAD {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        _ => "Error",
    }
}

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    host: Option<String>,
}

fn parse_request(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?, first.next()?);
    if !target.starts_with('/') {
        return None;
    }
    let url = url::Url::parse(&format!("http://x{target}")).ok()?;
    let host = lines.take_while(|l| !l.is_empty()).find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.eq_ignore_ascii_case("host")
            .then(|| value.trim().to_string())
    });
    Some(Request {
        method: method.to_string(),
        path: url.path().to_string(),
        query: url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect(),
        host,
    })
}

/// `(status, extra headers, body)`.
type Reply = (u16, &'static str, String);

fn route(ctx: &Ctx, req: &Request) -> Reply {
    if req.host.as_deref() != Some(format!("127.0.0.1:{}", ctx.port).as_str()) {
        return refused(400);
    }
    if req.method != "GET" {
        return refused(405);
    }
    if let Some(token) = req.path.strip_prefix("/start/") {
        // Once: whoever fetched the page first (the browser Atlas opened) holds the state.
        return if same(token, &ctx.token) && !ctx.form_served.swap(true, Ordering::SeqCst) {
            (200, FORM_CSP, ctx.form.clone())
        } else {
            refused(404)
        };
    }
    if req.path != "/callback" {
        return refused(404);
    }
    let param = |name: &str| {
        req.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    let (Some(state), Some(code)) = (param("state"), param("code")) else {
        return refused(400);
    };
    if !same(state, &ctx.state) || !is_code(code) {
        return refused(400);
    }
    match ctx.tx.try_send(code.to_string()) {
        Ok(()) => (200, PAGE_CSP, done_page()),
        Err(_) => refused(409),
    }
}

fn is_code(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_CODE
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Equality that does not stop at the first different byte.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

const PAGE_CSP: &str = "Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'\r\n";
/// The form may submit to GitHub only.
const FORM_CSP: &str = "Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; \
                        script-src 'unsafe-inline'; form-action https://github.com\r\n";

fn refused(status: u16) -> Reply {
    (
        status,
        PAGE_CSP,
        page(
            &t!("native_crew_github_app_refused_title"),
            &format!(
                "<h1>{}</h1><p>{}</p>",
                esc(&t!("native_crew_github_app_refused_title")),
                esc(&t!("native_crew_github_app_refused_body"))
            ),
            "",
        ),
    )
}

fn done_page() -> String {
    page(
        &t!("native_crew_github_app_done_title"),
        &format!(
            "<h1>{}</h1><p>{}</p>",
            esc(&t!("native_crew_github_app_done_title")),
            esc(&t!("native_crew_github_app_done_body"))
        ),
        // The address carries the code: leave it out of the tab's history.
        "<script>history.replaceState(null,'','/')</script>",
    )
}

/// The page that POSTs `manifest` to `action` by itself (and shows a button for a browser that blocks the script).
fn form_page(action: &str, manifest: &str) -> String {
    page(
        &t!("native_crew_github_app_page_title"),
        &format!(
            "<h1>{}</h1><form id=\"f\" method=\"post\" action=\"{}\"><input type=\"hidden\" name=\"manifest\" value=\"{}\">\
             <button type=\"submit\">{}</button></form>",
            esc(&t!("native_crew_github_app_page_redirecting")),
            esc(action),
            esc(manifest),
            esc(&t!("native_crew_github_app_page_continue")),
        ),
        "<script>document.getElementById('f').submit()</script>",
    )
}

fn page(title: &str, body: &str, script: &str) -> String {
    format!(
        "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\"><meta name=\"color-scheme\" content=\"light dark\">\
         <title>{}</title><style>body{{font:16px system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem}}\
         h1{{font-size:1.2rem}}button{{font:inherit;padding:.4rem .9rem}}</style></head><body>{body}{script}</body></html>",
        esc(atlas_i18n::lang()),
        esc(title),
    )
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    impl Session {
        /// The listener's port, read from its form URL (the one place the session keeps it).
        fn port(&self) -> u16 {
            url::Url::parse(&self.form_url).unwrap().port().unwrap()
        }
    }

    const POST: &str = "https://github.com/organizations/acme/settings/apps/new";

    fn manifest() -> Value {
        json!({"name": "atlas-crew-box", "description": "a \"quoted\" <b>box</b> & co", "public": false})
    }

    /// One raw HTTP/1.1 GET against the listener: `(status, body)`.
    async fn get(port: u16, target: &str, host: Option<&str>) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let host = host.map_or_else(|| format!("127.0.0.1:{port}"), str::to_string);
        s.write_all(format!("GET {target} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut raw = String::new();
        s.read_to_string(&mut raw).await.unwrap();
        let status = raw[9..12].parse().unwrap();
        let body = raw.split_once("\r\n\r\n").unwrap().1.to_string();
        (status, body)
    }

    fn unesc(s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    }

    /// The `value` of the hidden `manifest` input and the form's `action`, as the browser would read them.
    fn form_of(html: &str) -> (String, Value) {
        let attr = |name: &str, from: &str| {
            let at = html.find(from).unwrap();
            let rest = &html[at..];
            let start = rest.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
            let end = start + rest[start..].find('"').unwrap();
            unesc(&rest[start..end])
        };
        let action = attr("action", "<form");
        let manifest = serde_json::from_str(&attr("value", "<input")).unwrap();
        (action, manifest)
    }

    fn path_of(url: &str) -> String {
        url::Url::parse(url).unwrap().path().to_string()
    }

    async fn started() -> (Session, String) {
        let s = start_flow(manifest(), POST, Duration::from_secs(30))
            .await
            .unwrap();
        let (_, html) = get(s.port(), &path_of(&s.form_url), None).await;
        (s, html)
    }

    fn state_of(action: &str) -> String {
        url::Url::parse(action)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1
            .into_owned()
    }

    #[tokio::test]
    async fn the_form_posts_the_manifest_with_this_listeners_redirect_and_a_state() {
        let (s, html) = started().await;
        let (action, manifest) = form_of(&html);
        assert!(action.starts_with(POST), "{action}");
        assert_eq!(state_of(&action).len(), 32);
        // The crew server's fields survive the HTML round trip, quotes and markup included.
        assert_eq!(manifest["description"], "a \"quoted\" <b>box</b> & co");
        assert_eq!(
            manifest["redirect_url"],
            format!("http://127.0.0.1:{}/callback", s.port())
        );
        assert!(html.contains("submit()"), "the page submits itself");
    }

    #[tokio::test]
    async fn the_form_page_is_served_once_and_only_to_its_token() {
        let s = start_flow(manifest(), POST, Duration::from_secs(30))
            .await
            .unwrap();
        let path = path_of(&s.form_url);
        assert_eq!(get(s.port(), "/start/not-the-token", None).await.0, 404);
        assert_eq!(get(s.port(), "/", None).await.0, 404);
        assert_eq!(get(s.port(), &path, None).await.0, 200);
        assert_eq!(
            get(s.port(), &path, None).await.0,
            404,
            "a second fetch would leak the state"
        );
    }

    #[tokio::test]
    async fn the_code_comes_back_and_the_listener_closes() {
        let (s, html) = started().await;
        let state = state_of(&form_of(&html).0);
        let port = s.port();
        let (status, body) = get(port, &format!("/callback?code=abc123&state={state}"), None).await;
        assert_eq!(status, 200);
        assert!(
            body.contains("replaceState"),
            "the code leaves the address bar"
        );
        assert_eq!(
            s.done.await.unwrap(),
            Outcome::Code {
                code: "abc123".into()
            }
        );
        assert!(
            TcpStream::connect(("127.0.0.1", port)).await.is_err(),
            "a closed listener refuses connections"
        );
    }

    #[tokio::test]
    async fn a_wrong_state_is_refused_and_the_listener_keeps_waiting_for_the_right_one() {
        let (s, html) = started().await;
        let state = state_of(&form_of(&html).0);
        assert_eq!(
            get(s.port(), "/callback?code=stolen&state=wrong", None)
                .await
                .0,
            400
        );
        assert_eq!(get(s.port(), "/callback?code=stolen", None).await.0, 400);
        assert_eq!(
            get(s.port(), &format!("/callback?state={state}"), None)
                .await
                .0,
            400
        );
        assert_eq!(
            get(
                s.port(),
                &format!("/callback?code=a%20b&state={state}"),
                None
            )
            .await
            .0,
            400
        );
        get(
            s.port(),
            &format!("/callback?code=real&state={state}"),
            None,
        )
        .await;
        assert_eq!(
            s.done.await.unwrap(),
            Outcome::Code {
                code: "real".into()
            }
        );
    }

    #[test]
    fn a_second_valid_callback_finds_the_slot_taken() {
        let (tx, mut rx) = mpsc::channel(1);
        let ctx = Ctx {
            port: 7,
            state: "s".into(),
            token: "t".into(),
            form: String::new(),
            form_served: AtomicBool::new(false),
            tx,
        };
        let req = |code: &str| Request {
            method: "GET".into(),
            path: "/callback".into(),
            query: vec![("code".into(), code.into()), ("state".into(), "s".into())],
            host: Some("127.0.0.1:7".into()),
        };
        assert_eq!(route(&ctx, &req("first")).0, 200);
        assert_eq!(route(&ctx, &req("second")).0, 409);
        assert_eq!(rx.try_recv().unwrap(), "first");
        assert!(rx.try_recv().is_err(), "the second code never arrived");
    }

    #[tokio::test]
    async fn a_request_for_another_host_is_refused() {
        // A page on evil.example resolving to 127.0.0.1 (DNS rebinding) sends its own Host.
        let (s, html) = started().await;
        let state = state_of(&form_of(&html).0);
        let target = format!("/callback?code=x&state={state}");
        assert_eq!(get(s.port(), &target, Some("evil.example")).await.0, 400);
        assert_eq!(get(s.port(), &target, Some("localhost:1")).await.0, 400);
    }

    #[tokio::test]
    async fn it_closes_by_itself_after_the_window() {
        let s = start_flow(manifest(), POST, Duration::from_millis(80))
            .await
            .unwrap();
        let port = s.port();
        assert_eq!(s.done.await.unwrap(), Outcome::TimedOut);
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    }

    #[tokio::test]
    async fn cancel_closes_it() {
        let s = start_flow(manifest(), POST, Duration::from_secs(30))
            .await
            .unwrap();
        s.cancel.notify_one();
        assert_eq!(s.done.await.unwrap(), Outcome::Cancelled);
    }

    #[tokio::test]
    async fn a_silent_connection_does_not_block_the_callback() {
        // Browsers open speculative connections that never send a byte.
        let (s, html) = started().await;
        let state = state_of(&form_of(&html).0);
        let _idle = TcpStream::connect(("127.0.0.1", s.port())).await.unwrap();
        get(s.port(), &format!("/callback?code=ok&state={state}"), None).await;
        assert_eq!(s.done.await.unwrap(), Outcome::Code { code: "ok".into() });
    }

    #[tokio::test]
    async fn only_githubs_new_app_page_is_a_target() {
        for bad in [
            "http://github.com/settings/apps/new",
            "https://github.com.evil.example/settings/apps/new",
            "https://evil.example/settings/apps/new",
            "https://user@github.com/settings/apps/new",
            "https://github.com:8443/settings/apps/new",
            "https://github.com/settings/apps/new?state=x",
            "https://github.com/login",
            "javascript:alert(1)",
            "",
        ] {
            let refused = start_flow(manifest(), bad, Duration::from_secs(1))
                .await
                .is_err();
            assert!(refused, "{bad} was accepted");
        }
        for ok in [POST, "https://github.com/settings/apps/new"] {
            assert!(
                start_flow(manifest(), ok, Duration::from_millis(1))
                    .await
                    .is_ok(),
                "{ok}"
            );
        }
    }

    #[tokio::test]
    async fn a_manifest_that_is_not_an_object_is_refused() {
        assert!(
            start_flow(json!([1]), POST, Duration::from_secs(1))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn start_wait_and_cancel_work_through_the_commands() {
        let started = github_app_listen_start(manifest(), POST.into())
            .await
            .unwrap();
        assert_eq!(started.expires_in_secs, 600);
        let port = url::Url::parse(&started.url).unwrap().port().unwrap();
        let (_, html) = get(port, &path_of(&started.url), None).await;
        let state = state_of(&form_of(&html).0);

        let id = started.id.clone();
        let waiting = tokio::spawn(github_app_listen_wait(id.clone()));
        get(port, &format!("/callback?code=c0de&state={state}"), None).await;
        assert_eq!(
            waiting.await.unwrap().unwrap(),
            Outcome::Code {
                code: "c0de".into()
            }
        );
        assert!(
            github_app_listen_wait(id).await.is_err(),
            "a listener answers once"
        );

        // A newer start closes the older listener; cancel ends a wait in progress.
        let first = github_app_listen_start(manifest(), POST.into())
            .await
            .unwrap();
        let waiting = tokio::spawn(github_app_listen_wait(first.id.clone()));
        while FLOWS
            .lock()
            .get(&first.id)
            .is_some_and(|f| f.done.is_some())
        {
            tokio::task::yield_now().await; // until the wait has taken the receiver
        }
        github_app_listen_cancel(first.id);
        assert_eq!(waiting.await.unwrap().unwrap(), Outcome::Cancelled);
        let second = github_app_listen_start(manifest(), POST.into())
            .await
            .unwrap();
        github_app_listen_cancel(second.id);
    }
}
