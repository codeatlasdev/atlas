//! Per-service memory over time: the budget state machine (`memory_warn` / `max_memory`), the RSS
//! time series, its trend, and the leak detector (monotonic growth across reloads/restarts).
//! Pure: every rule is a function of its inputs and the timestamps passed in, so the tests pin
//! each one without a clock or a process.

use std::collections::VecDeque;
use std::sync::LazyLock;

use serde::Serialize;

const MB: u64 = 1 << 20;

/// Samples kept per service. One every 5 s (`USAGE_EVERY`) = the last hour.
pub const SERIES_CAP: usize = 720;
/// The series of every service, rewritten after each sample next to the status file; what
/// `atlas_metrics` reads (the MCP server is another process than `atlas dev`).
pub const METRICS_FILE: &str = ".tui-metrics.json";
/// A trend needs this many samples, spanning at least [`TREND_MIN_SPAN_S`].
const TREND_MIN_SAMPLES: usize = 6;
const TREND_MIN_SPAN_S: u64 = 25;
/// Growth under this many MB per minute is `flat` (allocator noise, GC saw-tooth).
pub const FLAT_MB_PER_MIN: f64 = 2.0;
/// Reload/restart cycles of history the leak detector keeps.
const CYCLES_CAP: usize = 32;
/// Cycles in a row that must grow before a leak is suspected.
pub const LEAK_MIN_CYCLES: usize = 4;
/// Total growth over those cycles below which it is not worth a warning.
pub const LEAK_MIN_GROWTH: u64 = 64 * MB;
/// A cycle may dip by this fraction of its predecessor (GC) and still count as "not falling".
const LEAK_DIP_FRACTION: f64 = 0.03;
/// Reload lines closer together than this are one reload (a bundler prints several per change).
pub const RELOAD_DEBOUNCE_S: u64 = 2;

/// One RSS reading of a service's process tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sample {
    /// Unix seconds.
    pub t: u64,
    /// Bytes.
    pub rss: u64,
}

/// Bounded ring of samples, oldest first.
#[derive(Debug, Default, Clone)]
pub struct Series {
    samples: VecDeque<Sample>,
}

impl Series {
    pub fn push(&mut self, t: u64, rss: u64) {
        if self.samples.len() >= SERIES_CAP {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { t, rss });
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn last(&self) -> Option<Sample> {
        self.samples.back().copied()
    }

    /// The newest `n` samples (all when `n` is `None`), oldest first.
    pub fn tail(&self, n: Option<usize>) -> Vec<Sample> {
        let skip = n.map_or(0, |n| self.samples.len().saturating_sub(n));
        self.samples.iter().skip(skip).copied().collect()
    }

    /// Slope of the series, bytes per minute (least squares over the whole ring window of the
    /// last `window_s` seconds): `None` until it has enough samples spanning enough time.
    pub fn slope_bytes_per_min(&self, window_s: u64) -> Option<f64> {
        let last = self.samples.back()?;
        let from = last.t.saturating_sub(window_s);
        let pts: Vec<&Sample> = self.samples.iter().filter(|s| s.t >= from).collect();
        if pts.len() < TREND_MIN_SAMPLES || last.t - pts[0].t < TREND_MIN_SPAN_S {
            return None;
        }
        let n = pts.len() as f64;
        let t0 = pts[0].t as f64;
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
        for p in &pts {
            let x = (p.t as f64 - t0) / 60.0;
            let y = p.rss as f64;
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
        }
        let den = n * sxx - sx * sx;
        (den.abs() > f64::EPSILON).then(|| (n * sxy - sx * sy) / den)
    }
}

/// Window the trend looks at: long enough to see through a build spike, short enough to react.
pub const TREND_WINDOW_S: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Rising,
    Flat,
    Falling,
}

impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rising => "rising",
            Self::Flat => "flat",
            Self::Falling => "falling",
        }
    }
}

/// Direction and speed of the RSS: `mb_per_min` is signed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Trend {
    pub direction: Direction,
    pub mb_per_min: f64,
}

/// The trend of `series`, `None` while there is too little data to say.
pub fn trend(series: &Series) -> Option<Trend> {
    let raw = series.slope_bytes_per_min(TREND_WINDOW_S)? / MB as f64;
    // One decimal is what is reported, and the direction is judged on exactly that number, so
    // the flat band's edge never depends on float dust.
    let mb = (raw * 10.0).round() / 10.0;
    let direction = if mb > FLAT_MB_PER_MIN {
        Direction::Rising
    } else if mb < -FLAT_MB_PER_MIN {
        Direction::Falling
    } else {
        Direction::Flat
    };
    Some(Trend {
        direction,
        mb_per_min: mb,
    })
}

/// Where a service stands against its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// Under `memory_warn` (or no budget configured).
    #[default]
    Ok,
    /// Over `memory_warn`, under `max_memory`.
    Warn,
    /// Over `max_memory`.
    Over,
}

impl Budget {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Over => "over",
        }
    }
}

/// Thresholds in bytes, as configured.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    pub warn: Option<u64>,
    pub max: Option<u64>,
}

/// The budget a reading is in. `max` wins over `warn`; a `warn` above `max` is meaningless and
/// validation rejects it, but the order here keeps the answer sane regardless.
pub fn budget_of(rss: u64, l: Limits) -> Budget {
    match (l.max, l.warn) {
        (Some(max), _) if rss > max => Budget::Over,
        (_, Some(warn)) if rss > warn => Budget::Warn,
        _ => Budget::Ok,
    }
}

/// Samples in a row over a threshold before the state changes (a one-sample build spike is not
/// a leak), and in a row under it before it clears (no flapping around the line).
pub const SAMPLES_TO_ENTER: u32 = 2;
pub const SAMPLES_TO_LEAVE: u32 = 2;

/// Hysteresis around [`budget_of`]: the confirmed state only moves after
/// [`SAMPLES_TO_ENTER`] / [`SAMPLES_TO_LEAVE`] consistent readings.
#[derive(Debug, Default, Clone)]
pub struct BudgetMachine {
    state: Budget,
    candidate: Budget,
    streak: u32,
}

impl BudgetMachine {
    pub fn state(&self) -> Budget {
        self.state
    }

    /// Feed one reading; returns the new confirmed state when it just changed.
    pub fn observe(&mut self, rss: u64, limits: Limits) -> Option<Budget> {
        let reading = budget_of(rss, limits);
        if reading == self.state {
            self.candidate = reading;
            self.streak = 0;
            return None;
        }
        if reading == self.candidate {
            self.streak += 1;
        } else {
            self.candidate = reading;
            self.streak = 1;
        }
        let need = if reading > self.state {
            SAMPLES_TO_ENTER
        } else {
            SAMPLES_TO_LEAVE
        };
        if self.streak >= need {
            self.state = reading;
            self.streak = 0;
            return Some(reading);
        }
        None
    }

    /// New spawn: the previous process's budget standing does not carry over.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl PartialOrd for Budget {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Budget {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

/// `1.5 GB` / `512 MB`: the one size format of every reason text.
pub fn fmt_size(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
    } else {
        format!("{} MB", bytes / MB)
    }
}

/// Why a service is `degraded` over its soft budget: `using 2.1 GB, over memory_warn 1.5 GB`.
pub fn warn_reason(rss: u64, warn: u64) -> String {
    format!(
        "using {}, over memory_warn {}",
        fmt_size(rss),
        fmt_size(warn)
    )
}

/// What the cycles of one service say: RSS grew every time it reloaded or restarted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Leak {
    pub growth_bytes: u64,
    pub cycles: usize,
    /// `reloads` or `restarts`: the cycle kind that produced most of the history.
    pub kind: &'static str,
}

impl Leak {
    /// `leak suspected: +600 MB over 12 reloads`
    pub fn text(&self) -> String {
        format!(
            "leak suspected: +{} over {} {}",
            fmt_size(self.growth_bytes),
            self.cycles,
            self.kind
        )
    }
}

/// RSS at the end of every reload/restart cycle. A leak is RSS that only goes up from one cycle
/// to the next: a healthy dev server returns to about the same size after a reload.
#[derive(Debug, Clone)]
pub struct LeakDetector {
    /// `(rss at the cycle boundary, was it a restart)`, oldest first.
    cycles: VecDeque<(u64, bool)>,
    /// Unix seconds of the last counted reload (debounce).
    last_reload: Option<u64>,
    /// Reload lines closer than this count once.
    debounce_s: u64,
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::with_debounce(RELOAD_DEBOUNCE_S)
    }
}

impl LeakDetector {
    /// A detector whose reload lines closer than `debounce_s` seconds count once.
    pub fn with_debounce(debounce_s: u64) -> Self {
        Self {
            cycles: VecDeque::new(),
            last_reload: None,
            debounce_s,
        }
    }

    /// A reload was just logged at `t` with the tree at `rss`. Lines of one reload (a bundler
    /// prints several) closer than the debounce count once. `true` when it was counted.
    pub fn reload(&mut self, t: u64, rss: u64) -> bool {
        if self
            .last_reload
            .is_some_and(|l| t.saturating_sub(l) < self.debounce_s)
        {
            return false;
        }
        self.last_reload = Some(t);
        self.push(rss, false);
        true
    }

    /// The process was respawned; `rss` is the last reading of the process that just ended.
    pub fn restart(&mut self, rss: u64) {
        self.push(rss, true);
    }

    fn push(&mut self, rss: u64, restart: bool) {
        if self.cycles.len() >= CYCLES_CAP {
            self.cycles.pop_front();
        }
        self.cycles.push_back((rss, restart));
    }

    pub fn cycles(&self) -> usize {
        self.cycles.len()
    }

    /// The longest trailing run of non-falling cycles, when it is long and big enough to report.
    pub fn suspect(&self) -> Option<Leak> {
        let v: Vec<&(u64, bool)> = self.cycles.iter().collect();
        let mut start = v.len().checked_sub(1)?;
        while start > 0 {
            let (prev, cur) = (v[start - 1].0 as f64, v[start].0 as f64);
            // Not falling: at worst a small dip.
            if cur + prev * LEAK_DIP_FRACTION < prev {
                break;
            }
            start -= 1;
        }
        let run = &v[start..];
        if run.len() < LEAK_MIN_CYCLES {
            return None;
        }
        let growth = run.last()?.0.saturating_sub(run.first()?.0);
        if growth < LEAK_MIN_GROWTH {
            return None;
        }
        let restarts = run.iter().filter(|c| c.1).count();
        Some(Leak {
            growth_bytes: growth,
            cycles: run.len() - 1,
            kind: if restarts * 2 > run.len() {
                "restarts"
            } else {
                "reloads"
            },
        })
    }

    /// New run of atlas: history is not carried over.
    pub fn clear(&mut self) {
        *self = Self::with_debounce(self.debounce_s);
    }
}

/// Does `line` say the dev server just reloaded its code? One anchored pattern per common watcher:
/// bun `--hot` (`[bun] reloaded`), vite (`[vite] hmr update`, `page reload`, `server restarted`),
/// webpack/next (`compiled successfully`, `✓ Compiled`), nodemon (`[nodemon] restarting due to
/// changes`), tsx/ts-node-dev (`Restarting: ... has changed`), air (`building...`) and cargo-watch
/// (`[Running 'cargo run']`). An optional leading `HH:MM:SS` stamp is ignored. A line that merely mentions "reload" is not one.
pub fn is_reload_line(line: &str) -> bool {
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)^(?:\d{1,2}:\d{2}:\d{2}(?:[.,]\d+)?\s+)?\W*(\[bun\]\s+reloaded|\[vite\]\s+(hmr update|page reload|server restarted)|(\S+\s+)?hmr update\b|page reload\b|\[nodemon\]\s+restarting|(✓\s+)?compiled\b.*\b(successfully|in\s+\d)|restarting:?\s.*(has changed|changed)|\[running '.*'\]|building\.\.\.$)",
        )
        .expect("static regex")
    });
    RE.is_match(crate::runtime::text::strip_ansi(line).trim())
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
