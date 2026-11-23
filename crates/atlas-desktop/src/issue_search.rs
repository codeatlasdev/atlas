//! "Report issue" looks for an open issue that already carries the error's fingerprint before it opens a new one
//! (#320). The page cannot do it: its CSP bars `api.github.com`. So the app asks GitHub's issue search through `curl`
//! (as the gateway probe does: no new dependency, and the request leaves from this process, not the webview).
//!
//! Nothing but the fingerprint is sent: 16 hex characters, checked here, never what the person wrote.
//!
//! Unauthenticated search is limited (10 requests a minute), so a quiet failure must not turn into a stream of them:
//! - an answer (an issue, or none) is remembered for [`KEEP`], so the same screen opened again costs no request;
//! - a rate-limit answer (403 or 429 with `x-ratelimit-remaining: 0` or `retry-after`) stops every search until the
//!   time GitHub named (at most [`MAX_BLOCK`]);
//! - any other failure (offline, a timeout, a 5xx) is not remembered, and it is the page's cue to open the new-issue
//!   form as it always did: a failed search never blocks a report.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atlas_core::sys::CommandExt as _;
use parking_lot::Mutex;
use serde::Serialize;

const API: &str = "https://api.github.com";
const REPO: &str = "codeatlasdev/atlas";
/// How long an answer stands.
const KEEP: Duration = Duration::from_secs(10 * 60);
/// The longest a rate limit is honoured, whatever GitHub says.
const MAX_BLOCK: Duration = Duration::from_secs(15 * 60);
/// Without a reset time in the answer, how long to leave GitHub alone.
const DEFAULT_BLOCK: Duration = Duration::from_secs(60);
const TIMEOUT_SECS: &str = "10";

/// Why no answer came. camelCase and kebab-case: `apps/desktop/src/lib/errors/issueLookup.ts` reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// GitHub's search limit; nothing is asked again until it passes.
    RateLimited,
    /// Offline, a timeout, an answer that is not GitHub's: try nothing, open the new-issue form.
    Failed,
}

/// What the page gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Lookup {
    /// An open issue whose body carries the fingerprint.
    Found {
        number: u64,
        url: String,
        title: String,
    },
    /// The search worked and found nothing.
    None,
    Unavailable {
        reason: Reason,
    },
}

#[derive(Default)]
struct State {
    /// No request before this instant (GitHub said so).
    blocked_until: Option<Instant>,
    seen: HashMap<String, (Instant, Lookup)>,
}

/// One search door: where it asks and what it remembers.
pub struct Searcher {
    base: String,
    state: Mutex<State>,
}

impl Searcher {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            state: Mutex::new(State::default()),
        }
    }

    /// The open issue that carries `fingerprint`, if any. Never an error: a search that did not work says why.
    pub async fn search(&self, fingerprint: &str) -> Lookup {
        if fingerprint.len() != 16 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Lookup::Unavailable {
                reason: Reason::Failed,
            };
        }
        let fp = fingerprint.to_ascii_lowercase();
        {
            let mut st = self.state.lock();
            let now = Instant::now();
            st.seen.retain(|_, (at, _)| now.duration_since(*at) < KEEP);
            if let Some((_, hit)) = st.seen.get(&fp) {
                return hit.clone();
            }
            if st.blocked_until.is_some_and(|t| now < t) {
                return Lookup::Unavailable {
                    reason: Reason::RateLimited,
                };
            }
        }
        let url = format!(
            "{}/search/issues?q={fp}+repo:{REPO}+is:issue+is:open+in:body&per_page=5",
            self.base
        );
        let answer = fetch(&url).await;
        let mut st = self.state.lock();
        let out = match answer {
            None => Lookup::Unavailable {
                reason: Reason::Failed,
            },
            Some(a) => match interpret(&a, &fp, SystemTime::now()) {
                Interpreted::Answer(l) => {
                    st.seen.insert(fp, (Instant::now(), l.clone()));
                    l
                }
                Interpreted::Limited(wait) => {
                    st.blocked_until = Some(Instant::now() + wait);
                    Lookup::Unavailable {
                        reason: Reason::RateLimited,
                    }
                }
                Interpreted::Failed => Lookup::Unavailable {
                    reason: Reason::Failed,
                },
            },
        };
        out
    }
}

/// An HTTP answer: status, headers (names lower-cased) and body.
struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

enum Interpreted {
    Answer(Lookup),
    Limited(Duration),
    Failed,
}

/// The `curl -i` output: `HTTP/x <status> …` line, headers, a blank line, the body.
fn parse_response(raw: &str) -> Option<Answer> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .or_else(|| raw.split_once("\n\n"))?;
    let mut lines = head.lines();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Some(Answer {
        status,
        headers,
        body: body.to_string(),
    })
}

fn interpret(a: &Answer, fingerprint: &str, now: SystemTime) -> Interpreted {
    if matches!(a.status, 403 | 429) {
        let remaining_zero = a.header("x-ratelimit-remaining") == Some("0");
        let retry_after = a.header("retry-after").and_then(|v| v.parse::<u64>().ok());
        if !remaining_zero && retry_after.is_none() {
            return Interpreted::Failed;
        }
        let until_reset = a
            .header("x-ratelimit-reset")
            .and_then(|v| v.parse::<u64>().ok())
            .map(|reset| {
                Duration::from_secs(reset)
                    .saturating_sub(now.duration_since(UNIX_EPOCH).unwrap_or_default())
            });
        let wait = retry_after
            .map(Duration::from_secs)
            .or(until_reset)
            .unwrap_or(DEFAULT_BLOCK);
        return Interpreted::Limited(wait.min(MAX_BLOCK));
    }
    if a.status != 200 {
        return Interpreted::Failed;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&a.body) else {
        return Interpreted::Failed;
    };
    let Some(items) = v.get("items").and_then(|i| i.as_array()) else {
        return Interpreted::Failed;
    };
    // The search tokenizes; the body must really carry the fingerprint.
    let hit = items.iter().find_map(|i| {
        let body = i.get("body")?.as_str()?.to_ascii_lowercase();
        if !body.contains(fingerprint) || i.get("pull_request").is_some() {
            return None;
        }
        Some(Lookup::Found {
            number: i.get("number")?.as_u64()?,
            url: i.get("html_url")?.as_str()?.to_string(),
            title: i.get("title")?.as_str()?.to_string(),
        })
    });
    Interpreted::Answer(hit.unwrap_or(Lookup::None))
}

/// `GET url` with curl; `None` when curl cannot run or does not get an HTTP answer (DNS, TLS, timeout).
async fn fetch(url: &str) -> Option<Answer> {
    let out = tokio::process::Command::new("curl")
        .no_window()
        .args([
            "-sS",
            "-i",
            "-m",
            TIMEOUT_SECS,
            "-A",
            "atlas-desktop",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            url,
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    parse_response(&String::from_utf8_lossy(&out.stdout))
}

static SEARCHER: LazyLock<Searcher> = LazyLock::new(|| Searcher::new(API));

/// The open issue with this fingerprint, for "Report issue" (see the module docs).
#[tauri::command]
pub async fn issue_search(fingerprint: String) -> Lookup {
    SEARCHER.search(&fingerprint).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FP: &str = "0123456789abcdef";

    /// A GitHub on loopback: answers every request with `reply(request line)` and counts them.
    fn fake(reply: impl Fn(&str) -> String + Send + 'static) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { return };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                counted.fetch_add(1, Ordering::SeqCst);
                let _ = s.write_all(reply(req.lines().next().unwrap_or("")).as_bytes());
            }
        });
        (base, hits)
    }

    fn http(status: &str, headers: &[&str], body: &str) -> String {
        let extra: String = headers.iter().map(|h| format!("{h}\r\n")).collect();
        format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n{body}",
            body.len()
        )
    }

    fn issues(items: &[serde_json::Value]) -> String {
        serde_json::json!({ "total_count": items.len(), "items": items }).to_string()
    }

    fn issue(n: u64, body: &str) -> serde_json::Value {
        serde_json::json!({
            "number": n,
            "title": format!("Crash {n}"),
            "html_url": format!("https://github.com/codeatlasdev/atlas/issues/{n}"),
            "body": body,
        })
    }

    /// The real curl against a loopback GitHub: the open issue whose body holds the fingerprint is found, the request
    /// asks for open issues of this repo by that text and nothing else, and a second ask costs no request.
    #[tokio::test]
    async fn an_open_issue_with_the_fingerprint_is_found_once_and_remembered() {
        let (base, hits) = fake(|req| {
            assert!(
                req.starts_with(&format!(
                    "GET /search/issues?q={FP}+repo:codeatlasdev/atlas+is:issue+is:open+in:body"
                )),
                "{req}"
            );
            http(
                "200 OK",
                &[],
                &issues(&[
                    issue(7, "a different bug"),
                    issue(
                        9,
                        &format!("## Error\nFingerprint: `{}`", FP.to_uppercase()),
                    ),
                ]),
            )
        });
        let s = Searcher::new(&base);
        let want = Lookup::Found {
            number: 9,
            url: "https://github.com/codeatlasdev/atlas/issues/9".into(),
            title: "Crash 9".into(),
        };
        assert_eq!(s.search(FP).await, want);
        assert_eq!(s.search(FP).await, want);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_issue_with_the_fingerprint_is_none_and_a_pull_request_is_not_an_issue() {
        let (base, _) = fake(|_| {
            let mut pr = issue(3, &format!("fixes {FP}"));
            pr["pull_request"] = serde_json::json!({});
            http("200 OK", &[], &issues(&[issue(4, "no"), pr]))
        });
        assert_eq!(Searcher::new(&base).search(FP).await, Lookup::None);
    }

    /// GitHub's search limit is honoured: after one limited answer no other search is sent until the time it named.
    #[tokio::test]
    async fn a_rate_limit_stops_the_searches_until_it_passes() {
        let reset = (SystemTime::now() + Duration::from_secs(120))
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (base, hits) = fake(move |_| {
            http(
                "403 Forbidden",
                &[
                    "x-ratelimit-remaining: 0",
                    &format!("x-ratelimit-reset: {reset}"),
                ],
                r#"{"message":"API rate limit exceeded"}"#,
            )
        });
        let s = Searcher::new(&base);
        let limited = Lookup::Unavailable {
            reason: Reason::RateLimited,
        };
        assert_eq!(s.search(FP).await, limited);
        assert_eq!(
            s.search("fedcba9876543210").await,
            limited,
            "another fingerprint is blocked too"
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    /// A 403 that is not the limit (a blocked client, an abuse notice) and a 5xx are failures, asked again next time.
    #[tokio::test]
    async fn other_failures_are_failed_and_not_remembered() {
        let (base, hits) = fake(|_| http("503 Service Unavailable", &[], "{}"));
        let s = Searcher::new(&base);
        let failed = Lookup::Unavailable {
            reason: Reason::Failed,
        };
        assert_eq!(s.search(FP).await, failed);
        assert_eq!(s.search(FP).await, failed);
        assert_eq!(hits.load(Ordering::SeqCst), 2);

        let (base, _) = fake(|_| http("403 Forbidden", &[], r#"{"message":"blocked"}"#));
        assert_eq!(Searcher::new(&base).search(FP).await, failed);
    }

    #[tokio::test]
    async fn an_unreachable_github_and_a_body_that_is_not_json_are_failed() {
        let failed = Lookup::Unavailable {
            reason: Reason::Failed,
        };
        // A port nothing listens on: bind, note it, close.
        let dead = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", l.local_addr().unwrap())
        };
        assert_eq!(Searcher::new(&dead).search(FP).await, failed);
        let (base, _) = fake(|_| http("200 OK", &[], "<html>captive portal</html>"));
        assert_eq!(Searcher::new(&base).search(FP).await, failed);
    }

    /// Only a fingerprint goes out: anything else is refused before a request exists.
    #[tokio::test]
    async fn only_sixteen_hex_characters_are_ever_searched() {
        let (base, hits) = fake(|_| http("200 OK", &[], &issues(&[])));
        let s = Searcher::new(&base);
        for bad in [
            "",
            "0123",
            "0123456789abcdeg",
            "0123456789abcde+",
            "0123456789abcdef0",
            "zzzz zzzz zzzz zz",
        ] {
            assert_eq!(
                s.search(bad).await,
                Lookup::Unavailable {
                    reason: Reason::Failed
                },
                "{bad:?}"
            );
        }
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    /// The page reads these keys (`issueLookup.ts`); the real values are serialized, never hand-written.
    #[test]
    fn the_lookup_serializes_the_keys_the_page_reads() {
        assert_eq!(
            serde_json::to_value(Lookup::Found {
                number: 9,
                url: "u".into(),
                title: "t".into()
            })
            .unwrap(),
            serde_json::json!({"kind": "found", "number": 9, "url": "u", "title": "t"})
        );
        assert_eq!(
            serde_json::to_value(Lookup::None).unwrap(),
            serde_json::json!({"kind": "none"})
        );
        assert_eq!(
            serde_json::to_value(Lookup::Unavailable {
                reason: Reason::RateLimited
            })
            .unwrap(),
            serde_json::json!({"kind": "unavailable", "reason": "rate-limited"})
        );
    }

    #[test]
    fn a_curl_i_answer_is_read_with_http_2_status_lines_and_lf_only_breaks() {
        let a = parse_response("HTTP/2 429 \nRetry-After: 30\n\n{}").unwrap();
        assert_eq!((a.status, a.header("retry-after")), (429, Some("30")));
        assert!(matches!(
            interpret(&a, FP, SystemTime::now()),
            Interpreted::Limited(d) if d == Duration::from_secs(30)
        ));
        assert!(parse_response("curl: (6) Could not resolve host").is_none());
    }
}
