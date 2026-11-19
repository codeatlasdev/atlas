//! The MCP tools `atlas_affected` and `atlas_estimate`: let an agent decide what to run without
//! starting a process. Both answer from data (the project's declared graph, git, the task cache
//! and the heavy queue's history); neither runs the project's tasks. The logic lives in
//! `atlas_core::affected`, this module is argument parsing, the atlas.yaml lookup and the JSON
//! shapes.

use std::path::{Component, Path, PathBuf};

use atlas_core::affected::{self, ActionInputs};
use atlas_core::heavy::{self, Dir};
use atlas_core::taskcache::wrap::{self, Declared};
use atlas_core::taskcache::{Cache, Spec};
use serde_json::{Value, json};

use super::tools::{ToolResult, opt_str};
use crate::config::{self, ActionConfig, ProjectConfig};

fn opt_str_list(args: &Value, k: &str) -> Result<Option<Vec<String>>, String> {
    match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("'{k}' must be a list of strings"))
            })
            .collect::<Result<_, _>>()
            .map(Some),
        Some(_) => Err(format!("'{k}' must be a list of strings")),
    }
}

fn load(root: &Path) -> Result<ProjectConfig, String> {
    config::load_or_detect_in(root, atlas_core::lang::DEFAULT)
        .map(|(c, _)| c)
        .map_err(|e| format!("{e:#}"))
}

fn json_out(v: &Value) -> ToolResult {
    serde_json::to_string_pretty(v).map_err(|e| e.to_string())
}

// ---- atlas_affected ----

/// A path the agent passed, as a project-relative `/` path: an absolute path inside `root` is
/// made relative, one that leaves the project is an error.
fn relative(root: &Path, f: &str) -> Result<String, String> {
    let p = Path::new(f);
    let rel: PathBuf = if p.is_absolute() {
        p.strip_prefix(root)
            .map_err(|_| format!("'{f}' is outside the project ({})", root.display()))?
            .to_path_buf()
    } else {
        p.to_path_buf()
    };
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("'{f}' is outside the project ({})", root.display()));
    }
    Ok(rel
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

/// `atlas_affected` for the project at `root`.
pub(super) fn affected_in(root: &Path, args: &Value) -> ToolResult {
    let files = opt_str_list(args, "files")?;
    let base = opt_str(args, "base")?;
    let (files, files_from) = match files {
        Some(f) => (
            f.iter()
                .map(|f| relative(root, f))
                .collect::<Result<Vec<_>, _>>()?,
            "args",
        ),
        None => (affected::changed_files(root, base)?, "git"),
    };
    let cfg = load(root)?;
    let actions: Vec<ActionInputs> = cfg
        .actions
        .iter()
        .filter_map(|a| {
            a.cache.as_ref().map(|c| ActionInputs {
                label: a.label.clone(),
                inputs: c.inputs.clone(),
            })
        })
        .collect();
    let declared = cfg.affected.as_ref().map(|a| a.declared());
    let (source, impact) = affected::resolve(declared.as_ref(), &actions, root, &files)?;
    json_out(&json!({
        "source": source,
        "base": base,
        "files_from": files_from,
        "files": files,
        "tasks": impact.tasks,
        "tests": impact.tests,
        "routes": impact.routes,
        "builds": impact.builds,
    }))
}

// ---- atlas_estimate ----

/// What the estimate is about: the label and command the cache and the queue know it by, and what
/// its result depends on.
struct Target {
    label: String,
    command: String,
    /// `None` for an action without a `cache:` block: it always runs, there is nothing to key.
    spec: Option<Spec>,
    /// The `atlas run --cache …` line that runs it (or the hint when it cannot be cached).
    run_with: Option<String>,
    note: Option<String>,
}

fn action_target(root: &Path, a: &ActionConfig) -> Target {
    let spec = a.cache.as_ref().map(|c| Spec {
        cwd: root.to_path_buf(),
        argv: vec![a.command.clone()],
        inputs: c.inputs.clone(),
        outputs: c.outputs.clone(),
        env: c.env.clone(),
        tools: c.tools.clone(),
    });
    let run_with = a.cache.as_ref().map(|c| {
        wrap::cached(
            &a.label,
            &a.command,
            a.heavy,
            Declared {
                inputs: &c.inputs,
                outputs: &c.outputs,
                env: &c.env,
                tools: &c.tools,
            },
        )
    });
    Target {
        label: a.label.clone(),
        command: a.command.clone(),
        note: spec.is_none().then(|| {
            "this action has no `cache:` block, so it always runs; add `cache: { inputs: [...] }` \
             in atlas.yaml to let identical runs be answered from the cache"
                .to_string()
        }),
        spec,
        run_with,
    }
}

fn find_action<'a>(cfg: &'a ProjectConfig, name: &str) -> Option<&'a ActionConfig> {
    let mut one = name.chars();
    let key = one.next().filter(|_| one.next().is_none());
    cfg.actions
        .iter()
        .find(|a| a.label.eq_ignore_ascii_case(name) || key == Some(a.key))
}

fn command_target(root: &Path, args: &Value, command: &str) -> Result<Target, String> {
    let list = |k| opt_str_list(args, k).map(Option::unwrap_or_default);
    let (inputs, outputs, env, tools) = (
        list("inputs")?,
        list("outputs")?,
        list("env")?,
        list("tools")?,
    );
    affected::check_globs(&inputs)?;
    let label = opt_str(args, "label")?.map_or_else(|| command.to_string(), str::to_string);
    let run_with = wrap::cached(
        &label,
        command,
        false,
        Declared {
            inputs: &inputs,
            outputs: &outputs,
            env: &env,
            tools: &tools,
        },
    );
    Ok(Target {
        spec: Some(Spec {
            cwd: root.to_path_buf(),
            argv: vec![command.to_string()],
            inputs,
            outputs,
            env,
            tools,
        }),
        label,
        command: command.to_string(),
        run_with: Some(run_with),
        note: None,
    })
}

/// The queue as seen by a job that would start now.
fn queue_json(heavy: &Dir, label: &str) -> Value {
    let st = heavy.status(heavy::slots());
    // When a slot would free for a new job of this label: 0 with a slot free, else the soonest
    // running job's ETA (none when the history cannot tell).
    let free = st.slots.saturating_sub(st.running.len());
    let wait = if free > st.queued.len() {
        Some(0)
    } else {
        st.running.iter().filter_map(|e| e.eta_s).min()
    };
    json!({
        "slots": st.slots,
        "running": st.running.len(),
        "queued": st.queued.len(),
        "load": st.load,
        "mem_available_pct": st.mem_available_pct,
        "wait_eta_s": wait,
        "label": label,
    })
}

/// `atlas_estimate` for the project at `root`, against the given cache and heavy queue (the tool
/// uses the machine-wide ones).
pub(super) fn estimate_in(root: &Path, args: &Value, cache: &Cache, heavy: &Dir) -> ToolResult {
    let (action, command) = (opt_str(args, "action")?, opt_str(args, "command")?);
    let project = atlas_core::project_root::resolve(root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    let target = match (action, command) {
        (Some(_), Some(_)) => return Err("pass either 'action' or 'command', not both".into()),
        (None, None) => {
            return Err("'action' (an atlas.yaml action) or 'command' is required".into());
        }
        (Some(name), None) => {
            let cfg = load(root)?;
            let a = find_action(&cfg, name).ok_or_else(|| {
                let labels: Vec<&str> = cfg.actions.iter().map(|a| a.label.as_str()).collect();
                format!("no action '{name}' (actions: {})", labels.join(", "))
            })?;
            action_target(root, a)
        }
        (None, Some(c)) => command_target(root, args, c)?,
    };
    let e = affected::estimate::estimate(
        &target.label,
        &target.command,
        project.as_deref(),
        target.spec.as_ref(),
        cache,
        heavy,
    );
    let mut v = serde_json::to_value(&e).map_err(|x| x.to_string())?;
    let o = v.as_object_mut().expect("an estimate is an object");
    o.insert("name".into(), json!(target.label));
    o.insert("command".into(), json!(target.command));
    o.insert("project".into(), json!(project));
    o.insert("run_with".into(), json!(target.run_with));
    o.insert("note".into(), json!(target.note));
    o.insert("queue".into(), queue_json(heavy, &target.label));
    json_out(&v)
}
