//! The Machine view (#129): why the machine is slow and what each agent runs, in one report.
//! Load and memory with the heavy gates' thresholds, the heavy-job queue and its run history, the
//! services using the most memory, and the processes of every terminal (agents included).
//!
//! `aggregate` is pure (every input is a parameter), so the tests pin each rule without a
//! process table or a queue directory; `machine_report` only gathers the inputs.

use std::collections::HashMap;
use std::sync::Arc;

use atlas_core::heavy::{self, Dir, Gates, LabelRuns, Status};
use atlas_core::sys;
use atlas_tui::runtime::usage;
use atlas_tui::status::{self, ServiceRow};
use serde::Serialize;
use serde_json::{Value, json};

use crate::host::{HostHub, LOCAL};
use crate::registry::Entry;

/// Services listed (the heaviest first): the view is a "who is eating the machine" list, not an
/// inventory.
const TOP_SERVICES: usize = 8;

/// The 1, 5 and 15 minute load averages.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Load {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

/// The heavy gates' thresholds, so the gauges can draw the line a job waits at.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateView {
    /// A job starts only while the 5-minute load is below this; `null`: gate off.
    pub max_load: Option<f64>,
    /// A job starts only while at least this percent of memory is free; `null`: gate off.
    pub min_mem_pct: Option<u8>,
    /// What holds a job back right now (`load` | `memory`), `null` when the gates are open.
    pub holding: Option<&'static str>,
}

/// One service of one project, with what the runner says about its memory (#131).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceUse {
    pub project: String,
    pub name: String,
    pub state: String,
    /// Bytes of the process tree, `null` while it is not running.
    pub memory_rss: Option<u64>,
    /// `rising` | `flat` | `falling`, `null` until the runner has enough samples.
    pub memory_trend: Option<String>,
    pub memory_trend_mb_min: Option<f64>,
    /// `ok` | `warn` | `over` against the service's `memory_warn` / `max_memory`.
    pub budget_state: String,
    pub memory_leak: Option<String>,
    /// Percent of one core (can exceed 100).
    pub cpu: Option<u64>,
}

/// A terminal's process tree (a shell, an agent, whatever runs in a pane).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TermUse {
    pub pty: String,
    pub pid: u32,
    pub cwd: String,
    /// The program line (`omp --auto-approve`), the title when the terminal has no argv.
    pub command: String,
    pub created_at: u64,
    /// Bytes of the tree, `null` when the process could not be sampled.
    pub mem: Option<u64>,
    /// Percent of one core (can exceed 100).
    pub cpu: Option<f64>,
}

/// One heavy label's run history.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunHistory {
    pub label: String,
    /// Seconds per run, oldest first.
    pub secs: Vec<u64>,
    pub avg_s: u64,
    pub last_s: u64,
    pub max_s: u64,
}

/// Everything the Machine view shows; one poll, one payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Unix milliseconds the report was taken.
    pub at: u64,
    pub cores: usize,
    /// `null` where the OS has no load average (Windows).
    pub load: Option<Load>,
    /// Percent of memory free, `null` when unreadable.
    pub mem_available_pct: Option<u8>,
    /// Installed memory in bytes, `null` when unreadable.
    pub mem_total_bytes: Option<u64>,
    pub gates: GateView,
    pub heavy: Status,
    pub history: Vec<RunHistory>,
    /// Heaviest first, at most [`TOP_SERVICES`].
    pub services: Vec<ServiceUse>,
    /// Heaviest first.
    pub terminals: Vec<TermUse>,
}

/// What the report is built from.
pub struct Inputs {
    pub at: u64,
    pub cores: usize,
    pub load: Option<(f64, f64, f64)>,
    pub mem_available_pct: Option<u8>,
    pub mem_total_bytes: Option<u64>,
    pub gates: Gates,
    pub heavy: Status,
    pub runs: Vec<LabelRuns>,
    /// `(project name, rows)` of every project with a live runner.
    pub projects: Vec<(String, Vec<ServiceRow>)>,
    pub terminals: Vec<TermUse>,
}

pub fn aggregate(i: Inputs) -> Report {
    let holding = i
        .gates
        .hold(i.load.map(|l| l.1), i.mem_available_pct)
        .map(|h| match h {
            heavy::Hold::Load { .. } => "load",
            heavy::Hold::Memory { .. } => "memory",
        });
    let mut services: Vec<ServiceUse> = i
        .projects
        .into_iter()
        .flat_map(|(project, rows)| {
            rows.into_iter()
                .filter_map(move |r| service_use(&project, r))
        })
        .collect();
    // Heaviest first; a tie keeps the project/service order the runner reported (stable sort).
    services.sort_by_key(|s| std::cmp::Reverse(s.memory_rss));
    services.truncate(TOP_SERVICES);

    let mut terminals = i.terminals;
    terminals.sort_by_key(|t| std::cmp::Reverse(t.mem));

    Report {
        at: i.at,
        cores: i.cores,
        load: i
            .load
            .map(|(one, five, fifteen)| Load { one, five, fifteen }),
        mem_available_pct: i.mem_available_pct,
        mem_total_bytes: i.mem_total_bytes,
        gates: GateView {
            max_load: i.gates.max_load,
            min_mem_pct: i.gates.min_mem_pct,
            holding,
        },
        heavy: i.heavy,
        history: i.runs.into_iter().filter_map(run_history).collect(),
        services,
        terminals,
    }
}

/// A row worth listing: one that is running (a stopped service uses nothing).
fn service_use(project: &str, r: ServiceRow) -> Option<ServiceUse> {
    let rss = r.memory.rss.or(r.mem_mb.map(|m| m << 20))?;
    Some(ServiceUse {
        project: project.to_string(),
        name: r.name,
        state: r.state,
        memory_rss: Some(rss),
        memory_trend: r.memory.trend.map(|t| t.direction.name().to_string()),
        memory_trend_mb_min: r.memory.trend.map(|t| t.mb_per_min),
        budget_state: r.memory.budget.name().to_string(),
        memory_leak: r.memory.leak,
        cpu: r.cpu,
    })
}

fn run_history(r: LabelRuns) -> Option<RunHistory> {
    let last_s = *r.secs.last()?;
    Some(RunHistory {
        avg_s: r.secs.iter().sum::<u64>() / r.secs.len() as u64,
        max_s: r.secs.iter().copied().max().unwrap_or(last_s),
        last_s,
        label: r.label,
        secs: r.secs,
    })
}

/// The program line of a `pty_list` entry: argv, else the title, else `terminal`.
fn command_of(p: &Value) -> String {
    let argv: Vec<&str> = p["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if argv.is_empty() {
        p["title"].as_str().unwrap_or("terminal").to_string()
    } else {
        argv.join(" ")
    }
}

/// The running terminals of a `pty_list` reply with the usage of their process trees.
/// A terminal that has exited, or has no pid, is not listed.
fn terminals_of(list: &Value, usage: &HashMap<String, usage::Usage>) -> Vec<TermUse> {
    list["ptys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["exited"].is_null())
        .filter_map(|p| {
            let pty = p["pty"].as_str()?.to_string();
            let u = usage.get(&pty);
            Some(TermUse {
                pid: p["pid"].as_u64()? as u32,
                cwd: p["cwd"].as_str().unwrap_or_default().to_string(),
                command: command_of(p),
                created_at: p["created_at"].as_u64().unwrap_or(0),
                mem: u.map(|u| u.mem),
                cpu: u.map(|u| f64::from(u.cpu)),
                pty,
            })
        })
        .collect()
}

/// Name of a registry entry's project for the list (the folder, as the sidebar shows it).
fn project_name(e: &Entry) -> String {
    std::path::Path::new(&e.root)
        .file_name()
        .map_or_else(|| e.root.clone(), |n| n.to_string_lossy().into_owned())
}

/// The machine: load, memory, the heavy queue and its history, services by memory, terminals by
/// memory. Cheap enough to poll every few seconds while the view is open (a `ps`, a few small
/// files), and run off the async runtime: reading the registry and status files blocks.
#[tauri::command]
pub async fn machine_report(hub: tauri::State<'_, Arc<HostHub>>) -> Result<Report, String> {
    // The local host's terminals; a host that cannot be asked is an empty list here (the view is
    // about this machine, and the rest of the report is still worth showing).
    let list = match hub.client(LOCAL).await {
        Ok(c) => c.call("pty_list", json!({})).await.unwrap_or(json!({})),
        Err(_) => json!({}),
    };
    let roots: Vec<(String, u32)> = list["ptys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["exited"].is_null())
        .filter_map(|p| Some((p["pty"].as_str()?.to_string(), p["pid"].as_u64()? as u32)))
        .collect();
    let usage = usage::sample(&roots).await;
    tauri::async_runtime::spawn_blocking(move || gather(&terminals_of(&list, &usage)))
        .await
        .map_err(|e| e.to_string())
}

fn gather(terminals: &[TermUse]) -> Report {
    let dir = Dir::default_dir();
    let projects: Vec<(String, Vec<ServiceRow>)> = crate::registry::load()
        .unwrap_or_default()
        .iter()
        .filter(|e| !e.is_remote())
        .filter_map(|e| {
            let snap = status::live_snapshot(std::path::Path::new(&e.root)).ok()?;
            Some((project_name(e), snap.services))
        })
        .collect();
    aggregate(Inputs {
        at: heavy::now_ms(),
        cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
        load: sys::load_avg(),
        mem_available_pct: sys::mem_available_pct(),
        mem_total_bytes: sys::mem_total_bytes(),
        gates: Gates::from_env(),
        heavy: dir.status(heavy::slots()),
        runs: dir.runs(),
        projects,
        terminals: terminals.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::heavy::{Entry as QueueEntry, Job, State};
    use atlas_tui::runtime::manager::MemoryFacts;

    const MB: u64 = 1 << 20;

    fn row(
        name: &str,
        state: &str,
        rss_mb: Option<u64>,
        budget: &str,
        trend: Option<(&str, f64)>,
    ) -> ServiceRow {
        let mut v = json!({
            "state": state,
            "memory_rss": rss_mb.map(|m| m * MB),
            "budget_state": budget,
        });
        if let Some((d, r)) = trend {
            v["memory_trend"] = json!(d);
            v["memory_trend_mb_min"] = json!(r);
        }
        ServiceRow {
            name: name.into(),
            state: state.into(),
            port: None,
            health: None,
            exit_code: None,
            restarts: 0,
            uptime_s: None,
            mem_mb: None,
            cpu: Some(12),
            started_at: None,
            note: None,
            off: false,
            port_conflict: None,
            ready: None,
            probe_error: None,
            unhealthy_since: None,
            unhealthy_misses: None,
            unhealthy_lines: Vec::new(),
            memory: MemoryFacts::from_json(&v),
        }
    }

    fn queue(running: usize, queued: usize) -> Status {
        let job = |n: usize, state: State| QueueEntry {
            job: Job {
                id: format!("{n:013}-4242-0000"),
                pid: 4242,
                label: "gate".into(),
                command: "bun run check".into(),
                cwd: "/work/atlas-wt-a".into(),
                project: "atlas-wt-a".into(),
                state,
                queued_at: 1_763_040_000_000,
                started_at: (state == State::Running).then_some(1_763_040_004_000),
                slot: (state == State::Running).then_some(0),
            },
            age_s: 61,
            avg_s: Some(240),
            eta_s: Some(179),
        };
        Status {
            slots: 2,
            running: (0..running).map(|n| job(n, State::Running)).collect(),
            queued: (0..queued).map(|n| job(100 + n, State::Queued)).collect(),
            load: Some(3.5),
            mem_available_pct: Some(42),
        }
    }

    fn pty(id: &str, pid: u64, argv: &[&str], exited: bool) -> Value {
        json!({
            "pty": id, "pid": pid, "cwd": "/work/atlas-wt-a", "argv": argv, "title": "zsh",
            "created_at": 1_763_040_000, "exited": exited.then(|| json!({"code": 0})),
        })
    }

    fn inputs() -> Inputs {
        let list = json!({"ptys": [
            pty("p1", 100, &["omp", "--auto-approve"], false),
            pty("p2", 200, &[], false),
            pty("p3", 300, &["zsh"], true),
        ]});
        let usage = HashMap::from([
            (
                "p1".to_string(),
                usage::Usage {
                    mem: 900 * MB,
                    cpu: 85.5,
                },
            ),
            (
                "p2".to_string(),
                usage::Usage {
                    mem: 40 * MB,
                    cpu: 0.0,
                },
            ),
        ]);
        Inputs {
            at: 1_763_040_065_000,
            cores: 12,
            load: Some((3.5, 4.0, 2.5)),
            mem_available_pct: Some(42),
            mem_total_bytes: Some(24 << 30),
            gates: Gates::from_inputs(None, None, None, None, 12),
            heavy: queue(1, 2),
            runs: vec![LabelRuns {
                label: "gate".into(),
                secs: vec![240, 300, 270],
            }],
            projects: vec![
                (
                    "atlas".into(),
                    vec![
                        row("web", "running", Some(315), "ok", Some(("flat", 0.0))),
                        row(
                            "api",
                            "degraded",
                            Some(1360),
                            "warn",
                            Some(("rising", 360.0)),
                        ),
                        row("lab", "stopped", None, "ok", None),
                    ],
                ),
                (
                    "ui".into(),
                    vec![row("docs", "running", Some(600), "ok", None)],
                ),
            ],
            terminals: terminals_of(&list, &usage),
        }
    }

    #[test]
    fn services_are_ranked_by_memory_and_a_stopped_one_is_left_out() {
        let r = aggregate(inputs());
        let names: Vec<_> = r
            .services
            .iter()
            .map(|s| (s.project.as_str(), s.name.as_str()))
            .collect();
        assert_eq!(names, [("atlas", "api"), ("ui", "docs"), ("atlas", "web")]);
        let api = &r.services[0];
        assert_eq!(api.memory_rss, Some(1360 * MB));
        assert_eq!(api.memory_trend.as_deref(), Some("rising"));
        assert_eq!(api.budget_state, "warn");
        // No trend yet reads as none, and a budget never reported reads as ok.
        assert_eq!(r.services[1].memory_trend, None);
        assert_eq!(r.services[1].budget_state, "ok");
    }

    #[test]
    fn only_the_heaviest_services_are_listed() {
        let mut i = inputs();
        let many: Vec<ServiceRow> = (0..20)
            .map(|n| row(&format!("s{n}"), "running", Some(100 + n), "ok", None))
            .collect();
        i.projects = vec![("big".into(), many)];
        let r = aggregate(i);
        assert_eq!(r.services.len(), TOP_SERVICES);
        assert_eq!(r.services[0].name, "s19", "heaviest first");
    }

    #[test]
    fn exited_terminals_are_dropped_and_the_rest_ranked_by_memory() {
        let r = aggregate(inputs());
        let t: Vec<_> = r
            .terminals
            .iter()
            .map(|t| (t.pty.as_str(), t.command.as_str()))
            .collect();
        // p3 exited; p2 has no argv, so its command is the title.
        assert_eq!(t, [("p1", "omp --auto-approve"), ("p2", "zsh")]);
        assert_eq!(r.terminals[0].mem, Some(900 * MB));
        assert_eq!(r.terminals[0].cpu, Some(85.5));
    }

    #[test]
    fn a_terminal_that_could_not_be_sampled_has_no_numbers_and_ranks_last() {
        let list = json!({"ptys": [pty("a", 1, &["a"], false), pty("b", 2, &["b"], false)]});
        let usage = HashMap::from([("b".to_string(), usage::Usage { mem: MB, cpu: 1.0 })]);
        let mut i = inputs();
        i.terminals = terminals_of(&list, &usage);
        let r = aggregate(i);
        assert_eq!(r.terminals[0].pty, "b");
        assert_eq!((r.terminals[1].mem, r.terminals[1].cpu), (None, None));
    }

    #[test]
    fn the_gate_view_names_what_holds_a_job_back() {
        // Defaults on 12 cores: load < 9, at least 20 percent free.
        let ok = aggregate(inputs());
        assert_eq!(
            (ok.gates.max_load, ok.gates.min_mem_pct, ok.gates.holding),
            (Some(9.0), Some(20), None)
        );

        let mut busy = inputs();
        busy.load = Some((9.5, 9.5, 9.5));
        assert_eq!(aggregate(busy).gates.holding, Some("load"));

        // Memory is reported before load: it freezes a machine, a busy CPU only slows it.
        let mut both = inputs();
        both.load = Some((9.5, 9.5, 9.5));
        both.mem_available_pct = Some(10);
        assert_eq!(aggregate(both).gates.holding, Some("memory"));

        // A reading the OS cannot give never holds a job.
        let mut blind = inputs();
        blind.load = None;
        blind.mem_available_pct = None;
        let r = aggregate(blind);
        assert_eq!((r.gates.holding, r.load.is_none()), (None, true));
    }

    #[test]
    fn run_history_summarises_each_label_and_skips_an_empty_one() {
        let mut i = inputs();
        i.runs.push(LabelRuns {
            label: "ghost".into(),
            secs: vec![],
        });
        let r = aggregate(i);
        assert_eq!(
            r.history.len(),
            1,
            "a label with no runs has nothing to show"
        );
        let h = &r.history[0];
        assert_eq!(
            (h.label.as_str(), h.avg_s, h.last_s, h.max_s),
            ("gate", 270, 270, 300)
        );
        assert_eq!(h.secs, [240, 300, 270]);
    }

    // Contract: `apps/desktop/src/lib/__fixtures__/machine-report.json` is the REAL serialization of
    // what the `machine_report` command returns; `panel/machine.test.ts` feeds it to the view's own
    // logic. `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn machine_report_fixture_is_the_real_serialization() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/machine-report.json");
        let fresh = serde_json::to_value(aggregate(inputs())).unwrap();
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
        )
        .unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
