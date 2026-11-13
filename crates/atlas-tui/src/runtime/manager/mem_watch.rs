//! A service's memory over time, applied to the manager: feed each sample into the series,
//! the budget machine and the leak detector (`MemWatch`), and surface the result (status file,
//! wire, MCP, sidebar) through one set of accessors. The rules themselves are pure and live in
//! `runtime::memory`.

use super::ServiceManager;
use super::types::MemWatch;
use crate::runtime::memory::{self, Budget, Leak, Trend};
use crate::runtime::service::ServiceState;

/// A leak is announced again only after this many more cycles of growth (no alert per reload).
const LEAK_REALERT_CYCLES: usize = 4;

/// What one sample changed, for the manager to act on.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Outcome {
    /// The confirmed budget state just moved here.
    pub(crate) entered: Option<Budget>,
    /// A leak was just suspected (or got `LEAK_REALERT_CYCLES` worse).
    pub(crate) leak: Option<Leak>,
}

impl MemWatch {
    /// One reading of the service tree at unix second `t`; `reloads` are the reload lines seen
    /// since the previous sample (unix seconds).
    pub(crate) fn on_sample(&mut self, t: u64, rss: u64, reloads: &[u64]) -> Outcome {
        self.series.push(t, rss);
        self.last_rss = Some(rss);
        for &at in reloads {
            self.leak.reload(at, rss);
        }
        let entered = self.budget.observe(rss, self.limits);
        let suspect = self.leak.suspect();
        let leak = match (&suspect, self.alerted_cycles) {
            (Some(l), None) => Some(l.clone()),
            (Some(l), Some(n)) if l.cycles >= n + LEAK_REALERT_CYCLES => Some(l.clone()),
            _ => None,
        };
        match (&leak, &suspect) {
            (Some(l), _) => self.alerted_cycles = Some(l.cycles),
            // The run of growth ended: the next suspicion is news again.
            (None, None) => self.alerted_cycles = None,
            (None, Some(_)) => {}
        }
        Outcome { entered, leak }
    }

    /// The process is being replaced: close a restart cycle with what the old one reached, and
    /// let the new one earn its own budget standing. The series keeps going (one timeline).
    pub(crate) fn on_respawn(&mut self) {
        if let Some(rss) = self.last_rss.take() {
            self.leak.restart(rss);
        }
        self.budget.reset();
    }

    /// `using 2.1 GB, over memory_warn 1.5 GB` while the confirmed state is `warn`.
    pub(crate) fn warn_reason(&self) -> Option<String> {
        let (warn, rss) = (self.limits.warn?, self.series.last()?.rss);
        (self.budget.state() == Budget::Warn).then(|| memory::warn_reason(rss, warn))
    }

    /// `using 5.0 GB, over max_memory 4.0 GB` while the confirmed state is `over`.
    pub(crate) fn over_reason(&self) -> Option<String> {
        let (max, rss) = (self.limits.max?, self.series.last()?.rss);
        (self.budget.state() == Budget::Over).then(|| {
            format!(
                "using {}, over max_memory {}",
                memory::fmt_size(rss),
                memory::fmt_size(max)
            )
        })
    }
}

/// What every consumer (status file, runner wire, MCP, sidebar) is told about a service's memory.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryFacts {
    /// Bytes of the process tree, `None` while it is not running.
    pub rss: Option<u64>,
    pub trend: Option<Trend>,
    pub budget: Budget,
    pub leak: Option<String>,
}

impl MemoryFacts {
    /// Read the keys [`ServiceManager::memory_facts_json`] writes (status file row or wire entry).
    /// A row from an older atlas (no keys) reads as nothing known.
    pub fn from_json(v: &serde_json::Value) -> Self {
        let direction = match v["memory_trend"].as_str() {
            Some("rising") => Some(memory::Direction::Rising),
            Some("flat") => Some(memory::Direction::Flat),
            Some("falling") => Some(memory::Direction::Falling),
            _ => None,
        };
        Self {
            rss: v["memory_rss"].as_u64(),
            trend: direction.map(|direction| Trend {
                direction,
                mb_per_min: v["memory_trend_mb_min"].as_f64().unwrap_or(0.0),
            }),
            budget: match v["budget_state"].as_str() {
                Some("warn") => Budget::Warn,
                Some("over") => Budget::Over,
                _ => Budget::Ok,
            },
            leak: v["memory_leak"].as_str().map(str::to_string),
        }
    }

    /// The `memory_*` / `budget_state` keys of the status file and the runner wire, the inverse
    /// of [`Self::from_json`]: one producer, so a consumer never sees two spellings. `memory_rss`
    /// is bytes rounded to whole MB and the trend to whole MB/min: the runner re-sends `rows`
    /// whenever they change, and finer values would change on every sample (`atlas_metrics` has
    /// the exact series). Pass `None` for a service nothing is known about.
    pub fn to_json(f: Option<&Self>) -> serde_json::Value {
        let trend = f.and_then(|f| f.trend);
        serde_json::json!({
            "memory_rss": f.and_then(|f| f.rss).map(|b| (b >> 20) << 20),
            "memory_trend": trend.map(|t| t.direction.name()),
            "memory_trend_mb_min": trend.map(|t| t.mb_per_min.round()),
            "budget_state": f.map_or("ok", |f| f.budget.name()),
            "memory_leak": f.and_then(|f| f.leak.as_deref()),
        })
    }
}

impl ServiceManager {
    /// What a sample changed, made real: alerts, the restart/notify at `max_memory`, and the
    /// `running` ⇄ `degraded` flip. The one place both the sampler and the tests go through.
    pub(crate) async fn apply_memory_outcome(&mut self, name: &str, used: u64, o: &Outcome) {
        let state = self.get_state(name).unwrap_or(ServiceState::Stopped);
        self.announce_memory(name, o, &state);
        if o.entered == Some(Budget::Over) {
            self.on_over_max(name, used).await;
        }
        self.refresh_memory_state(name);
    }

    /// Test hook: feed one reading through the same path a real sample takes (series, budget,
    /// leak, state), with `reloads` seen since the last one. Returns what the sample changed.
    #[cfg(test)]
    pub(crate) async fn test_sample(
        &mut self,
        name: &str,
        t: u64,
        rss: u64,
        reloads: &[u64],
    ) -> Outcome {
        let svc = self.services.get_mut(name).expect("service");
        svc.usage = Some(crate::runtime::usage::Usage { mem: rss, cpu: 0.0 });
        let o = svc.mem.on_sample(t, rss, reloads);
        self.apply_memory_outcome(name, rss, &o).await;
        o
    }
}

impl ServiceManager {
    /// The memory facts of `name` (`None` for an unknown service).
    pub fn memory_facts(&self, name: &str) -> Option<MemoryFacts> {
        if self.mirror.is_some() {
            return self.mirror_facts(name)?.memory.clone();
        }
        let s = self.services.get(name)?;
        let up = s.child.is_some();
        Some(MemoryFacts {
            rss: s.usage.filter(|_| up).map(|u| u.mem),
            trend: up.then(|| memory::trend(&s.mem.series)).flatten(),
            budget: if up { s.mem.budget() } else { Budget::Ok },
            leak: s.mem.leak.suspect().map(|l| l.text()),
        })
    }

    /// The `memory_*` / `budget_state` keys of the status file and the runner wire
    /// ([`MemoryFacts::to_json`]).
    pub(crate) fn memory_facts_json(&self, name: &str) -> serde_json::Value {
        MemoryFacts::to_json(self.memory_facts(name).as_ref())
    }

    /// Whether `name` is the kind of service the budget can degrade: alive and (by the
    /// caller's check) not already worse than `running`.
    pub(crate) fn memory_degrade_reason(&self, name: &str) -> Option<String> {
        self.services.get(name)?.mem.warn_reason()
    }

    /// Write `.logs/.tui-metrics.json`: the RSS series of every service (`[unix_s, bytes]`
    /// pairs, oldest first) with its trend, budget and leak facts, for `atlas_metrics`.
    pub(crate) fn write_metrics_file(&self) {
        if self.mirror.is_some() {
            return;
        }
        let services: serde_json::Map<String, serde_json::Value> = self
            .order
            .iter()
            .filter_map(|name| {
                let s = self.services.get(name)?;
                let mut v = self.memory_facts_json(name);
                v["samples"] = s
                    .mem
                    .series
                    .tail(None)
                    .iter()
                    .map(|p| serde_json::json!([p.t, p.rss]))
                    .collect();
                v["interval_s"] = self.usage_every.as_secs().into();
                v["cycles"] = s.mem.leak.cycles().into();
                Some((name.clone(), v))
            })
            .collect();
        let payload = serde_json::json!({
            "updated_at": crate::runtime::clock::now_secs(),
            "services": services,
        });
        if let Ok(text) = serde_json::to_string(&payload) {
            super::health_check::write_atomic(&self.log_dir, memory::METRICS_FILE, &text);
        }
    }

    /// Make the budget outcome visible: alerts for `warn` / `over` / leak, and for `warn` the
    /// state flips through the next `apply_probe`; clearing re-evaluates there too.
    pub(crate) fn announce_memory(&self, name: &str, o: &Outcome, alive_state: &ServiceState) {
        let alert = |message: String| {
            let _ = self.event_tx.send(super::ManagerEvent::CriticalAlert {
                service: name.to_string(),
                message: message.clone(),
            });
            self.note(name, message);
        };
        match o.entered {
            Some(Budget::Warn) => {
                if let Some(r) = self.memory_degrade_reason(name) {
                    self.note(name, r);
                }
            }
            Some(Budget::Ok) if *alive_state == ServiceState::Degraded => {
                self.note(name, "memory back under budget".into());
            }
            _ => {}
        }
        if let Some(leak) = &o.leak {
            alert(leak.text());
        }
    }
}
