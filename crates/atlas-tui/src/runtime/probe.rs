//! Health beyond "the port answers": the pure verdict logic (liveness misses →
//! `unhealthy`, readiness failures and repeated error lines → `degraded`) and the
//! readiness probe itself (HTTP or command). No manager state here: every rule is
//! a function of its inputs, so the tests pin the state machine without a clock.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::ReadyProbe;
use crate::runtime::service::ServiceState;

/// Identical (normalised) error lines in [`REPEAT_WINDOW`] that make a service `degraded`.
pub const REPEAT_MIN: usize = 3;
/// Sliding window of the repeated-error rule.
pub const REPEAT_WINDOW: Duration = Duration::from_secs(60);
/// Distinct error shapes tracked at once; the oldest are dropped past it.
const REPEAT_KEYS: usize = 64;
/// Log lines kept per service as context for an `unhealthy` report.
pub const CONTEXT_LINES: usize = 20;

/// Liveness misses of one service: how many in a row, and whether it ever answered.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Liveness {
    pub misses: u32,
    pub ever_healthy: bool,
}

/// What one liveness result means for a service with a live child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveVerdict {
    /// Answered: clear the misses.
    Healthy,
    /// Missed, still inside the grace (a dev server's hot restart closes its port for a moment).
    Grace,
    /// `threshold` misses in a row after it was healthy once.
    Unhealthy,
    /// Never answered yet: startup is the start-timeout's business, not a verdict.
    Starting,
}

impl Liveness {
    /// Record one probe result and say what it means.
    pub fn observe(&mut self, healthy: bool, threshold: u32) -> LiveVerdict {
        if healthy {
            self.misses = 0;
            self.ever_healthy = true;
            return LiveVerdict::Healthy;
        }
        self.misses += 1;
        if !self.ever_healthy {
            return LiveVerdict::Starting;
        }
        if self.misses >= threshold.max(1) {
            LiveVerdict::Unhealthy
        } else {
            LiveVerdict::Grace
        }
    }

    /// New spawn: forget everything.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Everything the state decision needs, gathered by the manager each tick.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub live: LiveVerdict,
    /// `Some(err)` when a configured `ready` probe failed this tick.
    pub ready_error: Option<&'a str>,
    /// Sample line while the same error repeats (≥ [`REPEAT_MIN`] in [`REPEAT_WINDOW`]).
    pub repeated: Option<&'a str>,
    /// `Some(reason)` while the service is over its `memory_warn` budget.
    pub memory: Option<&'a str>,
}

/// The state a service with a live child settles in, and why when it is not `Running`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub state: ServiceState,
    /// `degraded`: the failing probe's message, the repeated line or the memory overrun;
    /// `unhealthy`: nothing.
    pub reason: Option<String>,
}

/// Precedence: `unhealthy` (liveness lost) beats `degraded`; a failing `ready` probe
/// beats a repeated log line (it carries the endpoint's own error), which beats a memory
/// overrun (the least specific finding: the others say what is broken, this says it is big).
pub fn judge(i: Inputs<'_>) -> Option<Verdict> {
    let degraded = |reason: String| Verdict {
        state: ServiceState::Degraded,
        reason: Some(reason),
    };
    match i.live {
        LiveVerdict::Unhealthy => Some(Verdict {
            state: ServiceState::Unhealthy,
            reason: None,
        }),
        // Not answering yet / blipping: the caller keeps the current state.
        LiveVerdict::Starting | LiveVerdict::Grace => None,
        LiveVerdict::Healthy => Some(match (i.ready_error, i.repeated, i.memory) {
            (Some(e), ..) => degraded(format!("ready probe failed: {e}")),
            (None, Some(line), _) => degraded(format!("repeating error: {line}")),
            (None, None, Some(mem)) => degraded(mem.to_string()),
            (None, None, None) => Verdict {
                state: ServiceState::Running,
                reason: None,
            },
        }),
    }
}

/// `line` with what changes between occurrences of the same error removed:
/// ANSI, timestamps, UUIDs, hex ids and numbers become placeholders and whitespace collapses.
pub fn normalize_line(line: &str) -> String {
    use std::sync::LazyLock;
    static RES: LazyLock<[(regex::Regex, &'static str); 4]> = LazyLock::new(|| {
        let re = |p: &str| regex::Regex::new(p).expect("static regex");
        [
            // ISO-ish and HH:MM:SS(.mmm) timestamps, with an optional zone.
            (
                re(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?"),
                "<ts>",
            ),
            (re(r"\b\d{1,2}:\d{2}:\d{2}(?:[.,]\d+)?\b"), "<ts>"),
            (
                re(
                    r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b",
                ),
                "<id>",
            ),
            (re(r"\b0x[0-9a-fA-F]+\b|\b[0-9a-fA-F]{12,}\b"), "<hex>"),
        ]
    });
    static NUM: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\d+(?:\.\d+)?").expect("static regex"));
    let mut s = crate::runtime::text::strip_ansi(line);
    for (re, to) in RES.iter() {
        s = re.replace_all(&s, *to).into_owned();
    }
    let s = NUM.replace_all(&s, "<n>");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sliding-window counter of identical error lines. Time is passed in, never read.
#[derive(Debug, Default)]
pub struct RepeatDetector {
    seen: HashMap<String, VecDeque<Instant>>,
    /// Last raw line of each shape (the sample shown to the user).
    sample: HashMap<String, String>,
}

impl RepeatDetector {
    /// Record an error-level `line` at `now`.
    pub fn push(&mut self, line: &str, now: Instant) {
        let key = normalize_line(line);
        if key.is_empty() {
            return;
        }
        if self.seen.len() >= REPEAT_KEYS && !self.seen.contains_key(&key) {
            // Drop the shape whose newest hit is oldest.
            if let Some(oldest) = self
                .seen
                .iter()
                .min_by_key(|(_, v)| v.back().copied())
                .map(|(k, _)| k.clone())
            {
                self.seen.remove(&oldest);
                self.sample.remove(&oldest);
            }
        }
        let hits = self.seen.entry(key.clone()).or_default();
        hits.push_back(now);
        while hits
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) > REPEAT_WINDOW)
        {
            hits.pop_front();
        }
        self.sample
            .insert(key, crate::runtime::text::strip_ansi(line));
    }

    /// The sample of a shape that hit [`REPEAT_MIN`] within the window ending at `now`
    /// (the most frequent one). `None` once they stop: old hits age out.
    pub fn repeated(&self, now: Instant) -> Option<&str> {
        self.seen
            .iter()
            .map(|(k, v)| {
                let n = v
                    .iter()
                    .filter(|t| now.saturating_duration_since(**t) <= REPEAT_WINDOW)
                    .count();
                (k, n)
            })
            .filter(|(_, n)| *n >= REPEAT_MIN)
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
            .and_then(|(k, _)| self.sample.get(k).map(String::as_str))
    }

    pub fn clear(&mut self) {
        self.seen.clear();
        self.sample.clear();
    }
}

/// Per-spawn health context shared between a service's log pumps (writers) and
/// the manager (reader): the last lines, the repeated-error detector and the reloads seen.
#[derive(Debug, Default)]
pub struct LogContext {
    pub lines: VecDeque<String>,
    pub repeats: RepeatDetector,
    /// Unix seconds of each reload line since the manager last drained them (bounded).
    reloads: Vec<u64>,
}

pub type SharedContext = Arc<Mutex<LogContext>>;

/// Reload lines kept between two drains (a sample every 5 s; a bundler in a loop must not grow it).
const RELOADS_CAP: usize = 256;

impl LogContext {
    /// Feed one output line; `error` is whether it is error-level.
    pub fn push(&mut self, line: &str, error: bool, now: Instant) {
        if self.lines.len() >= CONTEXT_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(crate::runtime::text::strip_ansi(line));
        if error {
            self.repeats.push(line, now);
        }
    }

    /// `line` came at unix second `t`: remember it when it announces a code reload.
    pub fn note_reload(&mut self, line: &str, t: u64) {
        if self.reloads.len() < RELOADS_CAP && crate::runtime::memory::is_reload_line(line) {
            self.reloads.push(t);
        }
    }

    /// The reloads seen since the last call, oldest first.
    pub fn take_reloads(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.reloads)
    }

    pub fn tail(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

/// Why a readiness probe failed, one line.
pub type ProbeError = String;

/// Run the service's `ready` probe once, bounded by its timeout. `Ok(())` = ready.
/// `port` resolves a bare `http: /path`; `dir` is the command's working directory.
pub async fn check_ready(
    probe: &ReadyProbe,
    port: Option<u16>,
    dir: &std::path::Path,
) -> Result<(), ProbeError> {
    let timeout = probe.timeout();
    if let Some(cmd) = probe.command.as_deref() {
        return run_command(cmd, dir, timeout).await;
    }
    let Some(target) = probe.http.as_deref() else {
        return Err("ready: needs `http` or `command`".into());
    };
    let url = match ready_url(target, port) {
        Ok(u) => u,
        Err(e) => return Err(e),
    };
    crate::runtime::health::http_status(&url, timeout)
        .await
        .and_then(|code| match probe.status {
            Some(want) if code != want => Err(format!("GET {target} → {code} (expected {want})")),
            None if !(200..300).contains(&code) => Err(format!("GET {target} → {code}")),
            _ => Ok(()),
        })
}

/// `/path` → `http://127.0.0.1:<port>/path`; a full URL is used as is.
pub fn ready_url(target: &str, port: Option<u16>) -> Result<String, ProbeError> {
    if target.starts_with("http://") || target.starts_with("https://") {
        return Ok(target.to_string());
    }
    let Some(port) = port else {
        return Err(format!("ready http '{target}' needs the service `port`"));
    };
    let path = target.trim_start_matches('/');
    Ok(format!("http://127.0.0.1:{port}/{path}"))
}

async fn run_command(
    cmd: &str,
    dir: &std::path::Path,
    timeout: Duration,
) -> Result<(), ProbeError> {
    let mut c = crate::runtime::process::task_command(cmd);
    c.current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = c.spawn().map_err(|e| format!("`{cmd}`: {e}"))?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Err(_) => Err(format!("`{cmd}` timed out after {}s", timeout.as_secs())),
        Ok(Err(e)) => Err(format!("`{cmd}`: {e}")),
        Ok(Ok(out)) if out.status.success() => Ok(()),
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr);
            let last = err
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            let code = out
                .status
                .code()
                .map_or("signal".to_string(), |c| c.to_string());
            Err(if last.is_empty() {
                format!("`{cmd}` exited {code}")
            } else {
                format!("`{cmd}` exited {code}: {}", last.trim())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    // ── liveness threshold ──────────────────────────────────────────────

    /// Incident #132: a hung SSR service stayed `running`. It must turn unhealthy
    /// on exactly the threshold-th consecutive miss, never before.
    #[test]
    fn unhealthy_on_exactly_the_threshold_th_miss_after_being_healthy() {
        let mut l = Liveness::default();
        assert_eq!(l.observe(true, 3), LiveVerdict::Healthy);
        assert_eq!(l.observe(false, 3), LiveVerdict::Grace);
        assert_eq!(l.observe(false, 3), LiveVerdict::Grace);
        assert_eq!(l.observe(false, 3), LiveVerdict::Unhealthy);
        assert_eq!(
            l.observe(false, 3),
            LiveVerdict::Unhealthy,
            "stays unhealthy"
        );
    }

    #[test]
    fn a_single_answer_resets_the_miss_streak() {
        let mut l = Liveness::default();
        l.observe(true, 3);
        l.observe(false, 3);
        l.observe(false, 3);
        assert_eq!(l.observe(true, 3), LiveVerdict::Healthy);
        assert_eq!(l.observe(false, 3), LiveVerdict::Grace, "streak restarted");
        assert_eq!(l.observe(false, 3), LiveVerdict::Grace);
    }

    #[test]
    fn never_healthy_is_starting_not_unhealthy() {
        let mut l = Liveness::default();
        for _ in 0..10 {
            assert_eq!(l.observe(false, 3), LiveVerdict::Starting);
        }
    }

    #[test]
    fn threshold_one_and_zero_both_mean_the_first_miss() {
        for th in [0, 1] {
            let mut l = Liveness::default();
            l.observe(true, th);
            assert_eq!(
                l.observe(false, th),
                LiveVerdict::Unhealthy,
                "threshold {th}"
            );
        }
    }

    #[test]
    fn reset_forgets_that_it_was_ever_healthy() {
        let mut l = Liveness::default();
        l.observe(true, 3);
        l.reset();
        assert_eq!(l.observe(false, 1), LiveVerdict::Starting);
    }

    // ── verdict precedence ──────────────────────────────────────────────

    fn inputs<'a>(live: LiveVerdict, ready: Option<&'a str>, rep: Option<&'a str>) -> Inputs<'a> {
        Inputs {
            live,
            ready_error: ready,
            repeated: rep,
            memory: None,
        }
    }

    /// `memory_warn` (#131): a service over its soft budget, otherwise healthy, is `degraded`
    /// with the memory reason, and it never outranks what the probes or the log say.
    #[test]
    fn a_memory_budget_overrun_alone_is_degraded_with_the_memory_reason() {
        let v = judge(Inputs {
            memory: Some("using 2.1 GB, over memory_warn 1.5 GB"),
            ..inputs(LiveVerdict::Healthy, None, None)
        })
        .unwrap();
        assert_eq!(v.state, ServiceState::Degraded);
        assert_eq!(v.reason.unwrap(), "using 2.1 GB, over memory_warn 1.5 GB");
    }

    #[test]
    fn probe_and_log_findings_outrank_the_memory_reason() {
        let mem = Some("using 2.1 GB, over memory_warn 1.5 GB");
        let v = judge(Inputs {
            memory: mem,
            ..inputs(LiveVerdict::Healthy, Some("boom"), Some("noise"))
        })
        .unwrap();
        assert!(v.reason.unwrap().contains("boom"));
        let v = judge(Inputs {
            memory: mem,
            ..inputs(LiveVerdict::Healthy, None, Some("noise"))
        })
        .unwrap();
        assert!(v.reason.unwrap().contains("noise"));
    }

    #[test]
    fn memory_never_masks_a_hung_service_or_decides_while_starting() {
        let mem = Some("using 2.1 GB, over memory_warn 1.5 GB");
        let hung = judge(Inputs {
            memory: mem,
            ..inputs(LiveVerdict::Unhealthy, None, None)
        })
        .unwrap();
        assert_eq!(hung.state, ServiceState::Unhealthy);
        assert!(
            judge(Inputs {
                memory: mem,
                ..inputs(LiveVerdict::Grace, None, None)
            })
            .is_none()
        );
    }

    #[test]
    fn healthy_liveness_without_findings_is_running() {
        let v = judge(inputs(LiveVerdict::Healthy, None, None)).unwrap();
        assert_eq!(v.state, ServiceState::Running);
        assert!(v.reason.is_none());
    }

    /// Incident #136: liveness green, auth endpoint 500 → degraded with the probe error.
    #[test]
    fn ready_failure_with_liveness_passing_is_degraded_with_the_probe_error() {
        let v = judge(inputs(LiveVerdict::Healthy, Some("GET /ready → 500"), None)).unwrap();
        assert_eq!(v.state, ServiceState::Degraded);
        assert!(v.reason.unwrap().contains("GET /ready → 500"));
    }

    #[test]
    fn repeated_error_alone_is_degraded_with_the_sample() {
        let v = judge(inputs(
            LiveVerdict::Healthy,
            None,
            Some("db connect timeout"),
        ))
        .unwrap();
        assert_eq!(v.state, ServiceState::Degraded);
        assert!(v.reason.unwrap().contains("db connect timeout"));
    }

    #[test]
    fn ready_error_wins_over_a_repeated_line() {
        let v = judge(inputs(LiveVerdict::Healthy, Some("boom"), Some("noise"))).unwrap();
        assert!(v.reason.unwrap().contains("boom"));
    }

    #[test]
    fn lost_liveness_is_unhealthy_even_with_ready_findings() {
        let v = judge(inputs(LiveVerdict::Unhealthy, Some("x"), Some("y"))).unwrap();
        assert_eq!(v.state, ServiceState::Unhealthy);
    }

    #[test]
    fn grace_and_starting_decide_nothing() {
        assert!(judge(inputs(LiveVerdict::Grace, Some("x"), None)).is_none());
        assert!(judge(inputs(LiveVerdict::Starting, Some("x"), None)).is_none());
    }

    // ── line normalisation ──────────────────────────────────────────────

    #[test]
    fn same_error_with_other_timestamps_ids_and_numbers_normalises_equal() {
        let a = "\x1b[31m2026-11-13T10:00:01.123Z ERROR db connect timeout after 5000ms (pool 3) req 9f8e7d6c-1234-4abc-8def-0123456789ab\x1b[0m";
        let b = "2026-11-13 10:00:44,901 ERROR db connect timeout after 5012ms (pool 7) req 11111111-2222-4333-8444-555555555555";
        assert_eq!(normalize_line(a), normalize_line(b));
        let c = "12:00:01 ERROR   db   connect timeout after 7ms (pool 1) req 0xdeadbeef";
        let d = "12:00:09 ERROR db connect timeout after 9ms (pool 2) req 0xcafebabe";
        assert_eq!(normalize_line(c), normalize_line(d));
    }

    #[test]
    fn different_errors_stay_different() {
        assert_ne!(
            normalize_line("ERROR db connect timeout"),
            normalize_line("ERROR redis connect timeout")
        );
    }

    #[test]
    fn blank_or_ansi_only_lines_normalise_empty() {
        assert_eq!(normalize_line("   "), "");
        assert_eq!(normalize_line("\x1b[0m"), "");
    }

    // ── reloads seen in the log ─────────────────────────────────────────

    #[test]
    fn reload_lines_are_queued_with_their_time_and_drained_once() {
        let mut c = LogContext::default();
        c.note_reload("listening on :3000", 100);
        c.note_reload("[vite] hmr update /src/a.ts", 110);
        c.note_reload("[nodemon] restarting due to changes...", 120);
        assert_eq!(
            c.take_reloads(),
            vec![110, 120],
            "ordinary output is skipped"
        );
        assert!(c.take_reloads().is_empty(), "drained");
    }

    #[test]
    fn the_reload_queue_is_bounded_between_drains() {
        let mut c = LogContext::default();
        for i in 0..(RELOADS_CAP as u64 * 2) {
            c.note_reload("[vite] hmr update /x", i);
        }
        assert_eq!(c.take_reloads().len(), RELOADS_CAP);
    }

    // ── repeated-error window ───────────────────────────────────────────

    #[test]
    fn three_identical_errors_in_sixty_seconds_are_repeated_two_are_not() {
        let t = t0();
        let mut d = RepeatDetector::default();
        d.push("ERROR db timeout 1ms", t);
        d.push("ERROR db timeout 2ms", t + Duration::from_secs(10));
        assert!(d.repeated(t + Duration::from_secs(10)).is_none(), "2 < 3");
        d.push("ERROR db timeout 3ms", t + Duration::from_secs(20));
        let s = d.repeated(t + Duration::from_secs(20)).unwrap();
        assert!(s.contains("db timeout"), "{s}");
    }

    /// The boundary: three hits spread over more than 60 s are not a burst.
    #[test]
    fn hits_spread_wider_than_the_window_never_count() {
        let t = t0();
        let mut d = RepeatDetector::default();
        d.push("ERROR x", t);
        d.push("ERROR x", t + Duration::from_secs(40));
        d.push("ERROR x", t + Duration::from_secs(61));
        assert!(d.repeated(t + Duration::from_secs(61)).is_none());
    }

    #[test]
    fn repeated_clears_when_the_errors_stop() {
        let t = t0();
        let mut d = RepeatDetector::default();
        for s in [0, 5, 10] {
            d.push("ERROR db timeout", t + Duration::from_secs(s));
        }
        assert!(d.repeated(t + Duration::from_secs(10)).is_some());
        assert!(
            d.repeated(t + Duration::from_secs(71)).is_none(),
            "all three aged out"
        );
    }

    #[test]
    fn different_errors_do_not_add_up() {
        let t = t0();
        let mut d = RepeatDetector::default();
        d.push("ERROR a", t);
        d.push("ERROR b", t);
        d.push("ERROR c", t);
        assert!(d.repeated(t).is_none());
    }

    #[test]
    fn tracked_shapes_are_bounded() {
        let t = t0();
        let mut d = RepeatDetector::default();
        for i in 0..(REPEAT_KEYS * 3) {
            // letters, not digits: digits normalise away and would collapse into one shape
            let word: String = (0..6)
                .map(|k| (b'a' + ((i >> (k * 2)) & 3) as u8) as char)
                .collect();
            d.push(&format!("ERROR {word}x{i:03}z"), t);
        }
        assert!(d.seen.len() <= REPEAT_KEYS, "{}", d.seen.len());
    }

    #[test]
    fn context_keeps_only_the_last_n_lines_and_feeds_errors_to_the_detector() {
        let t = t0();
        let mut c = LogContext::default();
        for i in 0..(CONTEXT_LINES + 5) {
            c.push(&format!("line {i}"), false, t);
        }
        let tail = c.tail();
        assert_eq!(tail.len(), CONTEXT_LINES);
        assert_eq!(tail.first().unwrap(), "line 5");
        assert_eq!(tail.last().unwrap(), &format!("line {}", CONTEXT_LINES + 4));
        assert!(
            c.repeats.repeated(t).is_none(),
            "non-error lines are not counted"
        );
        for _ in 0..3 {
            c.push("ERROR boom", true, t);
        }
        assert!(c.repeats.repeated(t).is_some());
    }

    // ── ready_url ───────────────────────────────────────────────────────

    #[test]
    fn ready_url_resolves_paths_against_the_port_and_keeps_full_urls() {
        assert_eq!(
            ready_url("/ready", Some(3000)).unwrap(),
            "http://127.0.0.1:3000/ready"
        );
        assert_eq!(
            ready_url("ready", Some(3000)).unwrap(),
            "http://127.0.0.1:3000/ready"
        );
        assert_eq!(
            ready_url("https://x.test/r", None).unwrap(),
            "https://x.test/r"
        );
        assert!(ready_url("/ready", None).unwrap_err().contains("port"));
    }
}
