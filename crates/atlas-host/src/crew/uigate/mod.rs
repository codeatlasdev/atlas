//! The fast UI gate of a crewmate (#216): the `ui_gate` tool plans which scenarios a change needs
//! (a URL in a matrix of engine x width x theme), captures them, remembers the verdict per task so a
//! rerun repeats only what failed, and replays every UI bug the project ever fixed.
//!
//! | file             | what it owns                                                            |
//! |------------------|-------------------------------------------------------------------------|
//! | `scenario.rs`    | the unit: a URL and its matrix, its stable id                           |
//! | `plan.rs`        | the change's routes (`atlas_core::affected`) + regressions -> scenarios |
//! | `regressions.rs` | `.atlas/ui-regressions.json`: load, add, clear errors on a bad file     |
//! | `results.rs`     | per task: scenario -> pass/fail + capture names (`ui-gate.json`)        |
//! | `check.rs`       | the [`Check`] trait, and the UI detectors (#215, temporary) as one check     |
//! | this file        | running a plan through the task cache and a [`Capturer`], the tool entry |
//!
//! **Cache.** A scenario's key is the task cache's key (`atlas_core::taskcache`) of "ui-gate, this
//! scenario id, these checks" over the bytes of the project's files, uncommitted edits included. A
//! scenario that passed over the same bytes is a hit and the capturer is not called. Only passes are
//! stored, so a failing scenario is always captured again. What the cache cannot see is the running
//! app's data: `ui_gate {run, force: true}` captures everything regardless.

pub mod check;
pub mod plan;
pub mod regressions;
pub mod results;
pub mod scenario;
pub mod tree;

use std::path::{Path, PathBuf};

use atlas_core::taskcache::{Cache, Chunk, Run, Spec as CacheSpec, Stream};
use serde_json::{Value, json};

pub use check::{Check, Finding};
pub use plan::Planned;
pub use results::{Outcome, Source, TaskResults, Verdict};
pub use scenario::{Scenario, Why};
pub use tree::Tree;

use super::capture::{self, Captured, Spec};
use super::store::{CrewStore, Crewmate};

/// Something that captures a scenario: the real one runs the Playwright runner, a test counts calls.
#[allow(async_fn_in_trait)]
pub trait Capturer {
    async fn capture(&self, spec: &Spec, out_dir: &Path) -> Result<Captured, String>;
}

/// The real capturer: `crew::capture::run` on this crew server.
pub struct Playwright<'a>(pub &'a Path, pub Option<capture::Reach>);

impl Capturer for Playwright<'_> {
    async fn capture(&self, spec: &Spec, out_dir: &Path) -> Result<Captured, String> {
        // Whatever scenario a project file names, the page is the asking crewmate's to reach, not the server's.
        let mut spec = spec.clone();
        spec.reach = self.1;
        capture::run(self.0, &spec, out_dir).await
    }
}

/// Everything a run needs besides the plan.
pub struct Ctx<'a, C: Capturer> {
    pub capturer: &'a C,
    pub cache: &'a Cache,
    pub checks: &'a [Box<dyn Check>],
    /// The project checkout the change is in.
    pub root: &'a Path,
    /// Whose it is, and so who opens what in it (see [`Tree`]).
    pub tree: &'a Tree,
    /// Where this task keeps its captures and `ui-gate.json`.
    pub task_dir: &'a Path,
    /// Scratch for the runner, removed when the run ends.
    pub scratch: &'a Path,
    /// Skip the cache: capture everything.
    pub force: bool,
}

fn cache_spec(ctx_root: &Path, id: &str, checks: &[Box<dyn Check>]) -> CacheSpec {
    let mut argv = vec!["atlas-ui-gate".to_string(), id.to_string()];
    argv.extend(checks.iter().map(|c| format!("check:{}", c.name())));
    CacheSpec {
        cwd: ctx_root.to_path_buf(),
        argv,
        inputs: vec![],
        outputs: vec![],
        env: vec![],
        tools: vec![],
    }
}

/// The cached pass of `planned`, when the cache has one and its screenshots are still in the
/// task's dir (a hit that names files that are gone is a miss).
fn cached_pass(
    cache: &Cache,
    tree: &Tree,
    spec: &CacheSpec,
    task_dir: &Path,
) -> Option<Vec<String>> {
    let key = tree.cache_key(spec).ok()?;
    let entry = cache.lookup(&key)?;
    let mut text = Vec::new();
    cache.replay(&entry, &mut text, &mut std::io::sink()).ok()?;
    let names: Vec<String> = serde_json::from_slice(&text).ok()?;
    names
        .iter()
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        .all(|n| task_dir.join(n).is_file())
        .then_some(names)
}

fn remember_pass(
    cache: &Cache,
    tree: &Tree,
    spec: &CacheSpec,
    planned: &Planned,
    names: &[String],
    ms: u64,
) {
    let Ok(key) = tree.cache_key(spec) else {
        return;
    };
    let run = Run {
        project: spec
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        label: format!("ui-gate {}", planned.id),
        command: format!("ui-gate {}", planned.scenario.url),
        exit: 0,
        duration_ms: ms,
        log: vec![Chunk {
            stream: Stream::Stdout,
            bytes: serde_json::to_vec(names).unwrap_or_default(),
        }],
    };
    // A cache that cannot write only costs a repeat capture next time.
    if let Err(e) = cache.record(&key, spec, &run) {
        // i18n-ignore: log
        eprintln!("atlas ui gate: could not store the result: {e}");
    }
}

/// Capture one scenario and run the checks on every cell. `Ok` carries the capture names, `Err`
/// every problem found.
async fn capture_one<C: Capturer>(
    ctx: &Ctx<'_, C>,
    planned: &Planned,
) -> Result<Vec<String>, Vec<String>> {
    let spec = planned
        .scenario
        .spec(check::wants_detectors(ctx.checks))
        .map_err(|e| vec![e])?;
    let out = ctx.scratch.join(&planned.id);
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    std::fs::create_dir_all(&out)
        .map_err(|e| vec![format!("cannot make {}: {e}", out.display())])?;
    let captured = ctx
        .capturer
        .capture(&spec, &out)
        .await
        .map_err(|e| vec![e])?;
    let mut problems = vec![];
    for (combo, png) in &captured.files {
        for check in ctx.checks {
            match check.run(*combo, png) {
                Ok(found) => problems.extend(
                    found
                        .into_iter()
                        .map(|f| format!("{combo}: {}: {}", f.check, f.message)),
                ),
                Err(e) => problems.push(format!("{combo}: {} could not run: {e}", check.name())),
            }
        }
    }
    // Keep the shots under the scenario's id so two scenarios never share a file name.
    let mut names = vec![];
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    std::fs::create_dir_all(ctx.task_dir)
        .map_err(|e| vec![format!("cannot make {}: {e}", ctx.task_dir.display())])?;
    for (combo, png) in &captured.files {
        let name = format!(
            "{}-{}-{}-{}.png",
            planned.id,
            combo.engine.as_str(),
            combo.width,
            combo.theme.as_str()
        );
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        std::fs::copy(png, ctx.task_dir.join(&name))
            .map_err(|e| vec![format!("cannot keep {name}: {e}")])?;
        names.push(name);
    }
    if problems.is_empty() {
        Ok(names)
    } else {
        Err(problems)
    }
}

/// Run `plan` into `results`: a scenario with a cached pass is a hit, the rest are captured, and
/// every verdict is saved before the next one starts (a killed run keeps what it learned).
pub async fn execute<C: Capturer>(
    ctx: &Ctx<'_, C>,
    plan: Vec<Planned>,
    results: &mut TaskResults,
) -> Result<(), String> {
    for planned in plan {
        let spec = cache_spec(ctx.root, &planned.id, ctx.checks);
        let hit = if ctx.force {
            None
        } else {
            let (cache, tree, s, dir) = (
                ctx.cache.clone(),
                ctx.tree.clone(),
                spec.clone(),
                ctx.task_dir.to_path_buf(),
            );
            tokio::task::spawn_blocking(move || cached_pass(&cache, &tree, &s, &dir))
                .await
                .map_err(|e| e.to_string())?
        };
        let outcome = if let Some(captures) = hit {
            Outcome {
                scenario: planned.scenario.clone(),
                why: planned.why,
                verdict: Verdict::Pass,
                source: Source::Cached,
                captures,
                problems: vec![],
            }
        } else {
            let started = std::time::Instant::now();
            match capture_one(ctx, &planned).await {
                Ok(captures) => {
                    let ms = started.elapsed().as_millis() as u64;
                    let (cache, tree, s, p, n) = (
                        ctx.cache.clone(),
                        ctx.tree.clone(),
                        spec,
                        planned.clone(),
                        captures.clone(),
                    );
                    tokio::task::spawn_blocking(move || {
                        remember_pass(&cache, &tree, &s, &p, &n, ms)
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    Outcome {
                        scenario: planned.scenario.clone(),
                        why: planned.why,
                        verdict: Verdict::Pass,
                        source: Source::Captured,
                        captures,
                        problems: vec![],
                    }
                }
                Err(problems) => Outcome {
                    scenario: planned.scenario.clone(),
                    why: planned.why,
                    verdict: Verdict::Fail,
                    source: Source::Captured,
                    captures: vec![],
                    problems,
                },
            }
        };
        results.scenarios.insert(planned.id, outcome);
        results.save(ctx.task_dir)?;
    }
    Ok(())
}

/// What the agent reads back.
pub fn report(results: &TaskResults, ran: &[String]) -> Value {
    let scenarios: Vec<Value> = results
        .scenarios
        .iter()
        .map(|(id, o)| {
            json!({
                "id": id, "url": o.scenario.url, "why": o.why, "verdict": o.verdict,
                "source": o.source, "captures": o.captures, "problems": o.problems,
                "ran_now": ran.contains(id),
            })
        })
        .collect();
    json!({
        "passed": results.passed(),
        "failed": results.failed(),
        "scenarios": scenarios,
    })
}

/// Which action of the tool.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Run {
        base_url: String,
        /// Replaces the routes the change implies (the agent knows which pages it touched).
        routes: Option<Vec<String>>,
        /// Replaces the changed files `git` reports.
        files: Option<Vec<String>>,
        force: bool,
    },
    RerunFailed,
    AddRegression {
        scenario: Scenario,
        note: String,
    },
}

impl Action {
    pub fn from_args(args: &Value) -> Result<Self, String> {
        let list = |k: &str| -> Result<Option<Vec<String>>, String> {
            match args.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => serde_json::from_value(v.clone())
                    .map(Some)
                    .map_err(|_| format!("ui_gate: '{k}' must be a list of strings")),
            }
        };
        match args["action"].as_str() {
            Some("run") => Ok(Self::Run {
                base_url: args["base_url"]
                    .as_str()
                    .ok_or("ui_gate run: 'base_url' is required (where the app is served, e.g. http://localhost:5173)")?
                    .to_string(),
                routes: list("routes")?,
                files: list("files")?,
                force: args["force"].as_bool().unwrap_or(false),
            }),
            Some("rerun_failed") => Ok(Self::RerunFailed),
            Some("add_regression") => {
                let url = args["url"]
                    .as_str()
                    .ok_or("ui_gate add_regression: 'url' is required")?;
                let mut scenario = Scenario::for_url(url);
                if let Some(w) = args.get("widths").filter(|v| !v.is_null()) {
                    scenario.widths = serde_json::from_value(w.clone())
                        .map_err(|e| format!("ui_gate: bad widths: {e}"))?;
                }
                if let Some(t) = args.get("themes").filter(|v| !v.is_null()) {
                    scenario.themes = serde_json::from_value(t.clone())
                        .map_err(|e| format!("ui_gate: bad themes: {e}"))?;
                }
                if let Some(e) = args.get("engines").filter(|v| !v.is_null()) {
                    scenario.engines = serde_json::from_value(e.clone())
                        .map_err(|e| format!("ui_gate: bad engines: {e}"))?;
                }
                Ok(Self::AddRegression {
                    scenario,
                    note: args["note"].as_str().unwrap_or("").to_string(),
                })
            }
            _ => Err("ui_gate: 'action' must be run, rerun_failed or add_regression".into()),
        }
    }
}

/// One tool call against `root`: the pure core of [`handle`], so a test drives it with a fake
/// capturer and a temp cache.
pub async fn dispatch<C: Capturer>(ctx: &Ctx<'_, C>, action: Action) -> Result<Value, String> {
    match action {
        Action::AddRegression { scenario, note } => {
            let id = scenario.id()?;
            let all = regressions::add(ctx.tree, ctx.root, scenario, &note)?;
            Ok(json!({
                "added": id,
                "file": regressions::FILE,
                "regressions": all.len(),
                "hint": "commit this file with your fix: it is how every other worktree replays it",
            }))
        }
        Action::Run {
            base_url,
            routes,
            files,
            force,
        } => {
            let routes = match routes {
                Some(r) => r,
                None => {
                    let files = match files {
                        Some(f) => f,
                        None => {
                            // git runs the crewmate's own `core.fsmonitor`: off the async threads.
                            let (tree, root) = (ctx.tree.clone(), ctx.root.to_path_buf());
                            crate::crew::proc::blocking(move || tree.changed_files(&root)).await??
                        }
                    };
                    plan::routes_of(ctx.tree, ctx.root, &files)?
                }
            };
            let regs = regressions::load(ctx.tree, ctx.root)?;
            let planned = plan::plan(&base_url, &routes, &regs)?;
            if planned.is_empty() {
                return Ok(json!({
                    "passed": true, "failed": [], "scenarios": [],
                    "note": "this change touches no route and there are no regressions: nothing to capture",
                }));
            }
            let ctx = Ctx {
                force,
                ..ctx.reborrow()
            };
            let ran: Vec<String> = planned.iter().map(|p| p.id.clone()).collect();
            // A new plan replaces the task's record: it is what this change needs now.
            let mut results = TaskResults::default();
            execute(&ctx, planned, &mut results).await?;
            Ok(report(&results, &ran))
        }
        Action::RerunFailed => {
            let mut results = TaskResults::load(ctx.task_dir)?;
            let failed = results.failed();
            if results.scenarios.is_empty() {
                return Err(
                    "ui_gate rerun_failed: nothing has run on this task yet; use `run`".into(),
                );
            }
            let planned: Vec<Planned> = failed
                .iter()
                .map(|id| {
                    let o = &results.scenarios[id];
                    Planned {
                        id: id.clone(),
                        scenario: o.scenario.clone(),
                        why: o.why,
                    }
                })
                .collect();
            // A failed scenario is never in the cache (only passes are stored), so this
            // captures it; the passed ones are not touched.
            execute(ctx, planned, &mut results).await?;
            Ok(report(&results, &failed))
        }
    }
}

impl<C: Capturer> Ctx<'_, C> {
    fn reborrow(&self) -> Ctx<'_, C> {
        Ctx {
            capturer: self.capturer,
            cache: self.cache,
            checks: self.checks,
            root: self.root,
            tree: self.tree,
            task_dir: self.task_dir,
            scratch: self.scratch,
            force: self.force,
        }
    }
}

/// A test seam, set under `capture::TEST_LOCK`: a cache in a temp dir in place of the machine's.
#[cfg(test)]
pub(crate) static TEST_CACHE: parking_lot::Mutex<Option<PathBuf>> = parking_lot::Mutex::new(None);

/// The machine's task cache (`~/.atlas/cache`, shared with every worktree and `atlas run --cache`).
fn cache() -> Cache {
    #[cfg(test)]
    if let Some(dir) = TEST_CACHE.lock().clone() {
        return Cache::at(dir, 1 << 28);
    }
    Cache::default_cache()
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The checkout the crewmate's change is in: its own work dir, else its project's.
fn root_of(hostd_dir: &Path, store: &CrewStore, crewmate: &Crewmate) -> PathBuf {
    crewmate
        .workdir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            super::project::work_dir_for(
                hostd_dir,
                &crewmate.id,
                store.isolated(&crewmate.id).as_ref(),
            )
        })
}

/// What the `ui_gate` hook route does: `Err` is the message the agent sees as the tool's error.
pub async fn handle(
    hostd_dir: &Path,
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    args: &Value,
) -> Result<Value, String> {
    let action = Action::from_args(args)?;
    let crewmate = store
        .load_crewmate(crewmate_id)
        .map_err(|e| format!("ui_gate: cannot read the crewmate: {e}"))?;
    let root = root_of(hostd_dir, store, &crewmate);
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    if !root.is_dir() {
        return Err(format!(
            "ui_gate: the work folder {} does not exist yet",
            root.display()
        ));
    }
    let task_dir = gate_dir(store, crewmate_id, task_id)?;
    let scratch = capture::capture_dir(hostd_dir)
        .join("runs")
        .join(crate::paths::new_id());
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("ui_gate: cannot make {}: {e}", scratch.display()))?;
    let _cleanup = RemoveOnDrop(scratch.clone());
    let cache = cache();
    let checks = check::registered();
    let capturer = Playwright(hostd_dir, capture::Reach::of(store, crewmate_id)?);
    // An isolated crewmate's tree is the crewmate's: git runs as it, and no file of it is opened through a link.
    let tree = match store.isolated(crewmate_id) {
        Some(_) => Tree::Isolated(store.git_for_checkout(crewmate_id, &root)),
        None => Tree::Own,
    };
    let ctx = Ctx {
        capturer: &capturer,
        cache: &cache,
        checks: &checks,
        root: &root,
        tree: &tree,
        task_dir: &task_dir,
        scratch: &scratch,
        force: false,
    };
    dispatch(&ctx, action).await
}

#[cfg(all(test, unix))]
mod tests;

/// The gate's own state for one task: its shots and `ui-gate.json`. Not the task's attachments:
/// proof a crewmate shows goes through `capture` (stored as agent attachments); the gate keeps
/// what it needs to rerun only failures and to tell a cached pass from one whose shots are gone.
pub(crate) fn gate_dir(
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
) -> Result<PathBuf, String> {
    if !crate::crew::pane::is_plain_name(task_id) {
        return Err(format!("ui_gate: {task_id:?} is not a task id"));
    }
    Ok(store
        .crewmate_dir(crewmate_id)
        .join("ui-gate")
        // tree-access: task_id is one plain name, checked just above (pane::is_plain_name)
        .join(task_id))
}
