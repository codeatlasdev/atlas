//! The git credential helper of a crewmate (`atlas crew git-credential get`): git runs it for every operation that
//! needs a password for github.com, and it asks hostd, over the crewmate's own authenticated hook channel, for an
//! installation token made for that repository at that moment. Nothing is cached here and nothing is written
//! anywhere: the token exists in this process for the length of one git operation, so no token sits in the
//! crewmate's environment and a push an hour after the task began still works.
//!
//! The protocol is git's (`gitcredentials(7)`): `key=value` lines in, `key=value` lines out. A request it will not
//! answer (another host, no app, no endpoint) prints nothing and ends 0, which tells git to try the next helper
//! (and, with none left, to fail the way it always does).

use std::io::{Read, Write};
use std::time::Duration;

/// What git sent.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub protocol: String,
    pub host: String,
    pub path: String,
}

/// The `key=value` lines of git's input, up to the blank line.
pub fn parse_request(input: &str) -> Request {
    let mut r = Request::default();
    for line in input.lines() {
        if line.is_empty() {
            break;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k {
            "protocol" => r.protocol = v.to_string(),
            "host" => r.host = v.to_string(),
            "path" => r.path = v.to_string(),
            _ => {}
        }
    }
    r
}

/// `owner/repo` of the `path` git sends with `credential.useHttpPath` (`owner/repo.git`, maybe with more after).
pub fn repo_of_path(path: &str) -> Option<String> {
    let mut parts = path.trim_matches('/').splitn(3, '/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let name = name.strip_suffix(".git").unwrap_or(name);
    (!owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
}

/// Where hostd's hook server is and how this crewmate proves who it is, from the environment the agent was started
/// with: the port and the crewmate's own token (or the file that names them), and which crewmate and task.
#[derive(Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
    pub crewmate: String,
    pub pane_key: String,
}

impl Endpoint {
    /// `env` answers environment variable lookups (a map under test).
    pub fn from_env(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let (mut port, mut token) = (env("ATLAS_HOOK_PORT"), env("ATLAS_HOOK_TOKEN"));
        let file = if port.is_none() || token.is_none() {
            // tree-access: this runs as the crewmate (git's helper), reading the endpoint file its own agent was given
            env("ATLAS_HOOK_ENDPOINT").and_then(|p| std::fs::read_to_string(p).ok())
        } else {
            None
        };
        if let Some(file) = file {
            let get = |k: &str| {
                file.lines()
                    .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
                    .map(|v| v.trim().to_string())
            };
            port = port.or_else(|| get("ATLAS_HOOK_PORT"));
            token = token.or_else(|| get("ATLAS_HOOK_TOKEN"));
        }
        let pane_key = env("ATLAS_PANE_KEY")?;
        let crewmate = env("ATLAS_CREWMATE_ID")
            .or_else(|| crate::crew::pane::crewmate_id(&pane_key).map(str::to_string))?;
        Some(Self {
            port: port?.parse().ok().filter(|p| *p != 0)?,
            token: token.filter(|t| !t.is_empty())?,
            crewmate,
            pane_key,
        })
    }
}

/// `get`: read git's request from `input`, ask hostd, print the credentials to `out`. Everything that is not an
/// answer prints nothing (see the module docs); the reason goes to `err`.
pub fn get(
    mut input: impl Read,
    mut out: impl Write,
    mut err: impl Write,
    env: impl Fn(&str) -> Option<String>,
) -> std::io::Result<()> {
    let mut text = String::new();
    // tree-access: git's own stdin, no path
    input.read_to_string(&mut text)?;
    let req = parse_request(&text);
    // Only github.com over https: the token of a repo is never offered to another host a URL names.
    if req.protocol != "https" || !req.host.eq_ignore_ascii_case("github.com") {
        return Ok(());
    }
    let Some(ep) = Endpoint::from_env(env) else {
        // i18n-ignore: stderr of a git credential helper, read in the agent's log
        writeln!(
            err,
            "atlas git-credential: not running inside a crewmate task"
        )?;
        return Ok(());
    };
    let body = serde_json::json!({
        "pane_key": ep.pane_key,
        "host": req.host,
        "path": req.path,
    });
    match ask(&ep, &body.to_string(), Duration::from_secs(25)) {
        Ok(v) => {
            // `username` and `password` only: nothing else of the answer reaches git.
            let (Some(u), Some(p)) = (v["username"].as_str(), v["password"].as_str()) else {
                return Ok(());
            };
            writeln!(out, "username={u}\npassword={p}")?;
        }
        // i18n-ignore: stderr of a git credential helper, read in the agent's log
        Err(e) => writeln!(err, "atlas git-credential: {e}")?,
    }
    Ok(())
}

/// `POST /crew/<id>/git_credential` to the hook server over loopback; the JSON of a `200`.
fn ask(ep: &Endpoint, body: &str, timeout: Duration) -> Result<serde_json::Value, String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], ep.port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|e| format!("hostd is not reachable: {e}"))?;
    let _ = s.set_read_timeout(Some(timeout));
    let req = format!(
        "POST /crew/{}/git_credential HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nX-Atlas-Hook-Token: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        ep.crewmate,
        ep.token,
        body.len()
    );
    s.write_all(req.as_bytes())
        .map_err(|e| format!("hostd: {e}"))?;
    let mut resp = String::new();
    // tree-access: a loopback socket to hostd, no path
    let _ = s.take(1 << 20).read_to_string(&mut resp);
    let (head, payload) = resp.split_once("\r\n\r\n").ok_or("hostd sent no answer")?;
    let status = head.split_whitespace().nth(1).unwrap_or("");
    let v: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    if status == "200" {
        return Ok(v);
    }
    Err(v["error"]
        .as_str()
        .map_or_else(|| format!("hostd answered {status}"), str::to_string))
}

/// Run as `atlas crew git-credential <op>`: stdin and stdout are git's, the environment the agent's.
pub fn run(op: &str) -> std::io::Result<()> {
    // `store` and `erase` are for helpers that keep credentials; this one keeps none.
    if op != "get" {
        let mut sink = Vec::new();
        return std::io::stdin().lock().read_to_end(&mut sink).map(|_| ());
    }
    get(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        std::io::stderr().lock(),
        |k| std::env::var(k).ok(),
    )
}
