//! Cross-field checks on the merged config (dependencies, cycles, ports,
//! keys, deploy rules) and non-fatal warnings. Every problem has a `code`
//! (`validate::code`) and the `args` behind its English message.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Value, json};

use super::{BUILTIN_KEYS, ConfigError, code, suggest};
use crate::config::project::{Condition, ProjectConfig};

/// Services by name: sorted for deterministic output (`HashMap` iteration
/// order is random).
fn sorted_services(config: &ProjectConfig) -> BTreeMap<&str, &crate::config::project::ServiceDef> {
    config
        .services
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect()
}

/// Cross-field checks on the merged config. Disabled services still count
/// (they exist in the file; a typo there is still a typo), except for port
/// uniqueness where a disabled service holds no port.
pub fn semantic(config: &ProjectConfig) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    let err = |path: String, code: &'static str, args: Value, message: String| {
        ConfigError::new(path, code, args, message)
    };
    let names: BTreeSet<&str> = config.services.keys().map(String::as_str).collect();
    let sorted = sorted_services(config);

    for (name, def) in &sorted {
        if let Some(r) = &def.remote {
            let path = format!("services.{name}.remote");
            if def.command.is_some() {
                errors.push(err(
                    path.clone(),
                    code::REMOTE_COMMAND,
                    json!({ "service": name }),
                    "a remote service runs no local command: remove `command`".into(),
                ));
            }
            if def.port.is_none() {
                errors.push(err(
                    path.clone(),
                    code::REMOTE_PORT_MISSING,
                    json!({ "service": name }),
                    "needs the service's local `port` (where atlas forwards it)".into(),
                ));
            }
            // Both go on an ssh command line: one plain word each, never an option (`-o…`).
            let word = |s: &str| {
                !s.is_empty()
                    && !s.starts_with('-')
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-@:[]".contains(c))
            };
            if !word(&r.ssh) {
                errors.push(err(
                    format!("{path}.ssh"),
                    code::REMOTE_SSH_INVALID,
                    json!({ "service": name, "ssh": r.ssh }),
                    format!(
                        "'{}' is not an SSH destination (a host alias or user@host)",
                        r.ssh
                    ),
                ));
            }
            if let Some(h) = &r.host
                && !word(h)
            {
                errors.push(err(
                    format!("{path}.host"),
                    code::REMOTE_HOST_INVALID,
                    json!({ "service": name, "host": h }),
                    format!("'{h}' is not a host name"),
                ));
            }
        }
        for (dep, cond) in def.deps() {
            let path = format!("services.{name}.depends_on.{dep}");
            if dep == *name {
                errors.push(err(
                    path,
                    code::DEPENDENCY_SELF,
                    json!({ "service": name }),
                    "a service cannot depend on itself".into(),
                ));
                continue;
            }
            let Some(target) = sorted.get(dep.as_str()) else {
                errors.push(unknown_service(
                    path,
                    &dep,
                    json!({ "service": name }),
                    &names,
                ));
                continue;
            };
            match cond {
                Condition::LogReady if target.ready_log.is_none() => errors.push(err(
                    path,
                    code::CONDITION_LOG_READY,
                    json!({ "service": name, "dependency": dep }),
                    format!("condition 'log_ready' requires services.{dep}.ready_log"),
                )),
                Condition::CompletedSuccessfully if target.command.is_none() => errors.push(err(
                    path,
                    code::CONDITION_COMPLETED,
                    json!({ "service": name, "dependency": dep }),
                    format!("condition 'completed_successfully' needs services.{dep}.command (nothing to run to completion)"),
                )),
                _ => {}
            }
        }
        fn size(v: &Option<String>) -> Option<(&str, Option<u64>)> {
            v.as_deref()
                .map(|m| (m, crate::runtime::usage::parse_size(m)))
        }
        let (max, warn) = (size(&def.max_memory), size(&def.memory_warn));
        for (key, v) in [("max_memory", max), ("memory_warn", warn)] {
            if let Some((m, None)) = v {
                errors.push(err(
                    format!("services.{name}.{key}"),
                    code::SIZE_INVALID,
                    json!({ "service": name, "key": key, "value": m }),
                    format!("'{m}' is not a size (use e.g. 512M or 4G)"),
                ));
            }
        }
        // The soft budget must be reachable before the hard one.
        if let (Some((max_text, Some(max_b))), Some((_, Some(warn_b)))) = (max, warn)
            && warn_b >= max_b
        {
            errors.push(err(
                format!("services.{name}.memory_warn"),
                code::MEMORY_WARN_ABOVE_MAX,
                json!({ "service": name, "max_memory": max_text }),
                format!("must be below max_memory ({max_text})"),
            ));
        }
        if let Some(rx) = &def.ready_log
            && let Err(e) = regex::Regex::new(rx)
        {
            let detail = e.to_string();
            let detail = detail.lines().last().unwrap_or("");
            errors.push(err(
                format!("services.{name}.ready_log"),
                code::READY_LOG_REGEX,
                json!({ "service": name, "detail": detail }),
                format!("invalid regex: {detail}"),
            ));
        }
        if let Some(r) = &def.ready
            && r.http.is_some() == r.command.is_some()
        {
            errors.push(err(
                format!("services.{name}.ready"),
                code::READY_EXCLUSIVE,
                json!({ "service": name }),
                "set exactly one of `http` (path or URL) or `command`".into(),
            ));
        }
    }

    if let Some(cycle) = find_cycle(&sorted, &names) {
        let path = cycle.join(" -> ");
        errors.push(err(
            "services".into(),
            code::DEPENDENCY_CYCLE,
            json!({ "cycle": path, "services": cycle }),
            format!("dependency cycle: {path}"),
        ));
    }

    // Ports: two enabled services on the same port can never both come up.
    let mut by_port: BTreeMap<u16, Vec<&str>> = BTreeMap::new();
    for (name, def) in &sorted {
        if def.is_enabled()
            && let Some(p) = def.port
        {
            by_port.entry(p).or_default().push(name);
        }
    }
    for (port, owners) in by_port.iter().filter(|(_, o)| o.len() > 1) {
        errors.push(err(
            format!("services.{}.port", owners[1]),
            code::PORT_DUPLICATE,
            json!({ "service": owners[1], "port": port, "other": owners[0] }),
            format!("port {port} is also used by '{}'", owners[0]),
        ));
    }

    // Profiles: every member must be a known service.
    for (profile, members) in &config.profiles {
        for (i, member) in members.iter().enumerate() {
            if names.contains(member.as_str()) {
                continue;
            }
            errors.push(unknown_service(
                format!("profiles.{profile}[{i}]"),
                member,
                json!({ "profile": profile }),
                &names,
            ));
        }
    }

    // Action keys.
    let mut seen: HashMap<char, &str> = HashMap::new();
    let open_keys: HashMap<char, &str> = sorted
        .iter()
        .filter_map(|(n, d)| d.open_key.map(|k| (k, *n)))
        .collect();
    for (i, a) in config.actions.iter().enumerate() {
        let path = format!("actions[{i}].key");
        if BUILTIN_KEYS.contains(&a.key) {
            errors.push(err(
                path,
                code::KEY_BUILTIN,
                json!({ "key": a.key.to_string() }),
                format!("key '{}' is a built-in dashboard key", a.key),
            ));
        } else if let Some(svc) = open_keys.get(&a.key) {
            errors.push(err(
                path,
                code::ACTION_KEY_OPEN_KEY,
                json!({ "key": a.key.to_string(), "service": svc }),
                format!("key '{}' is already the open_key of service '{svc}'", a.key),
            ));
        } else if let Some(prev) = seen.insert(a.key, &a.label) {
            errors.push(err(
                path,
                code::ACTION_KEY_DUPLICATE,
                json!({ "key": a.key.to_string(), "action": prev }),
                format!("key '{}' is already used by action '{prev}'", a.key),
            ));
        }
    }
    for (k, svc) in &open_keys {
        if BUILTIN_KEYS.contains(k) {
            errors.push(err(
                format!("services.{svc}.open_key"),
                code::KEY_BUILTIN,
                json!({ "key": k.to_string(), "service": svc }),
                format!("key '{k}' is a built-in dashboard key"),
            ));
        }
    }

    // `{{action: X}}` in `worktree.setup` must name an action.
    if let Some(setup) = config.worktree.as_ref().and_then(|w| w.setup.as_deref()) {
        for reference in crate::config::worktree_setup::unknown_refs(setup, &config.actions) {
            errors.push(err(
                "worktree.setup".into(),
                code::WORKTREE_SETUP_UNKNOWN_ACTION,
                json!({ "action": reference }),
                format!(
                    "mentions action '{reference}', which is not an action label or key in `actions`"
                ),
            ));
        }
    }

    // `open_tunnel: true` has nothing to open without a `tunnel:` block.
    if config.tunnel.is_none() {
        for (i, a) in config.actions.iter().enumerate() {
            if a.open_tunnel {
                errors.push(err(
                    format!("actions[{i}].open_tunnel"),
                    code::OPEN_TUNNEL_WITHOUT_TUNNEL,
                    json!({ "action": a.label }),
                    format!(
                        "action '{}' sets open_tunnel: true but there is no `tunnel:` block",
                        a.label
                    ),
                ));
            }
        }
    }

    if let Some(d) = &config.deploy {
        deploy_rules(d, &mut errors);
    }
    if let Some(a) = &config.affected {
        affected_rules(a, config, &mut errors);
    }

    errors
}

/// `unknown service 'x' (did you mean 'y'?)` at `path`: a dependency or a profile member. `context` is the
/// extra args that say where the name was written (`service` or `profile`).
fn unknown_service(
    path: String,
    name: &str,
    context: Value,
    names: &BTreeSet<&str>,
) -> ConfigError {
    let mut args = context;
    args["name"] = json!(name);
    match suggest(name, names.iter().copied()) {
        Some(s) => {
            args["suggestion"] = json!(s);
            ConfigError::new(
                path,
                code::UNKNOWN_SERVICE_SUGGEST,
                args,
                format!("unknown service '{name}' (did you mean '{s}'?)"),
            )
        }
        None => ConfigError::new(
            path,
            code::UNKNOWN_SERVICE,
            args,
            format!("unknown service '{name}'"),
        ),
    }
}

/// `affected:` — a rule must say which files it is about, with globs that parse, and may only
/// name actions that exist (labels match case-insensitively, like MCP).
fn affected_rules(
    a: &crate::config::AffectedConfig,
    config: &ProjectConfig,
    errors: &mut Vec<ConfigError>,
) {
    let mut e = |path: String, code: &'static str, args: Value, message: String| {
        errors.push(ConfigError::new(path, code, args, message));
    };
    if a.command.as_deref().is_some_and(|c| c.trim().is_empty()) {
        e(
            "affected.command".into(),
            code::AFFECTED_COMMAND_BLANK,
            json!({}),
            "is blank: give a command or remove it".into(),
        );
    }
    for (i, r) in a.rules.iter().enumerate() {
        let path = format!("affected.rules[{i}]");
        if r.matches.is_empty() {
            e(
                format!("{path}.match"),
                code::AFFECTED_RULE_NO_MATCH,
                json!({ "rule": i }),
                "needs at least one glob (e.g. 'crates/**')".into(),
            );
        } else if let Some((code, args, message)) = bad_globs(&r.matches, i) {
            e(format!("{path}.match"), code, args, message);
        }
        for t in &r.tasks {
            if !config
                .actions
                .iter()
                .any(|x| x.label.eq_ignore_ascii_case(t))
            {
                e(
                    format!("{path}.tasks"),
                    code::AFFECTED_TASK_UNKNOWN,
                    json!({ "rule": i, "task": t }),
                    format!("'{t}' is not the label of any action"),
                );
            }
        }
    }
}

/// Why `globs` do not parse, from the core's `check_globs` (the same call and message as before, also read by
/// MCP): `(code, args, English message)`. The core words a bad glob `bad glob "<glob>": <reason>`. Its other
/// sentence, `bad glob set [<globs>]: <reason>` (every glob parses alone but the set does not compile), has no
/// input of its own (60000 globs still compile): it keeps the code and the message and carries the whole list as
/// `glob`.
fn bad_globs(globs: &[String], rule: usize) -> Option<(&'static str, Value, String)> {
    let message = atlas_core::affected::check_globs(globs).err()?;
    let (glob, reason) = globs
        .iter()
        .find_map(|g| {
            let reason = message.strip_prefix(&format!("bad glob {g:?}: "))?;
            Some((g.clone(), reason.to_string()))
        })
        .unwrap_or_else(|| (globs.join(", "), message.clone()));
    let args = json!({ "rule": rule, "glob": glob, "reason": reason });
    Some((code::AFFECTED_BAD_GLOB, args, message))
}

fn deploy_rules(d: &crate::config::project::DeployConfig, errors: &mut Vec<ConfigError>) {
    use crate::config::project::DeployKind;
    let mut e = |path: String, code: &'static str, args: Value, message: &str| {
        errors.push(ConfigError::new(path, code, args, message))
    };
    if let Some(app) = &d.app
        && (app.is_empty()
            || !app
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
    {
        e(
            "deploy.app".into(),
            code::DEPLOY_APP_PATTERN,
            json!({ "app": app }),
            "must match ^[a-z0-9-]+$",
        );
    }
    if d.targets.is_empty() {
        e(
            "deploy.targets".into(),
            code::DEPLOY_TARGETS_EMPTY,
            json!({}),
            "at least one target is required",
        );
    }
    let targets: BTreeMap<_, _> = d.targets.iter().collect();
    for (t, tg) in targets {
        let p = |f: &str| format!("deploy.targets.{t}.{f}");
        if tg.services.is_empty() {
            e(
                p("services"),
                code::DEPLOY_SERVICES_EMPTY,
                json!({ "target": t }),
                "at least one service is required",
            );
        }
        if tg.keep.is_some_and(|k| k < 2) {
            e(
                p("keep"),
                code::DEPLOY_KEEP_MIN,
                json!({ "target": t, "keep": tg.keep }),
                "must be >= 2 (rollback needs a previous release)",
            );
        }
        if tg.dir.as_deref().is_some_and(|s| !s.starts_with('/')) {
            e(
                p("dir"),
                code::DEPLOY_PATH_ABSOLUTE,
                json!({ "target": t, "field": "dir" }),
                "must be an absolute path",
            );
        }
        if tg
            .pause_marker
            .as_deref()
            .is_some_and(|s| !s.starts_with('/'))
        {
            e(
                p("pause_marker"),
                code::DEPLOY_PATH_ABSOLUTE,
                json!({ "target": t, "field": "pause_marker" }),
                "must be an absolute path",
            );
        }
        let links: BTreeMap<_, _> = tg.link.iter().flatten().collect();
        for (k, v) in links {
            if k.is_empty() || k.starts_with('/') || k.split('/').any(|c| c == "..") {
                e(
                    p(&format!("link.{k}")),
                    code::DEPLOY_LINK_KEY,
                    json!({ "target": t, "link": k }),
                    "key must be a release-relative path without '..'",
                );
            }
            if !v.starts_with('/') {
                e(
                    p(&format!("link.{k}")),
                    code::DEPLOY_LINK_VALUE,
                    json!({ "target": t, "link": k, "value": v }),
                    "value must be an absolute path",
                );
            }
        }
        let mut seen = BTreeSet::new();
        for (i, s) in tg.services.iter().enumerate() {
            let sp = |f: &str| p(&format!("services[{i}].{f}"));
            if !seen.insert(s.name.as_str()) {
                e(
                    sp("name"),
                    code::DEPLOY_SERVICE_DUPLICATE,
                    json!({ "target": t, "name": s.name }),
                    &format!("duplicate service '{}'", s.name),
                );
            }
            if s.kind == DeployKind::Command && s.restart.is_none() {
                e(
                    sp("restart"),
                    code::DEPLOY_RESTART_REQUIRED,
                    json!({ "target": t, "name": s.name }),
                    "kind 'command' requires restart",
                );
            }
            if s.kind != DeployKind::Command && s.restart.is_some() {
                e(
                    sp("restart"),
                    code::DEPLOY_RESTART_ONLY_COMMAND,
                    json!({ "target": t, "name": s.name }),
                    "only valid for kind 'command'",
                );
            }
            if s.kind != DeployKind::Compose && s.compose_file.is_some() {
                e(
                    sp("compose_file"),
                    code::DEPLOY_COMPOSE_FILE_ONLY_COMPOSE,
                    json!({ "target": t, "name": s.name }),
                    "only valid for kind 'compose'",
                );
            }
        }
    }
}

/// Non-fatal findings: the config loads and runs, but something is likely
/// not what the author meant. `atlas config check` prints them and still exits 0.
pub fn warnings(config: &ProjectConfig) -> Vec<ConfigError> {
    let sorted = sorted_services(config);
    let mut out = Vec::new();
    let warn = |path: String, code: &'static str, args: Value, message: String| {
        ConfigError::new(path, code, args, message)
    };
    for (name, def) in &sorted {
        if !def.is_enabled() {
            continue;
        }
        for (dep, cond) in def.deps() {
            let Some(target) = sorted.get(dep.as_str()).filter(|t| t.is_enabled()) else {
                continue;
            };
            let path = format!("services.{name}.depends_on.{dep}");
            // A dependent of a hand-started service waits until someone starts it.
            if !target.is_autostart() && def.is_autostart() {
                out.push(warn(
                    path.clone(),
                    code::WARN_AUTOSTART_FALSE,
                    json!({ "service": name, "dependency": dep }),
                    format!(
                        "'{dep}' has autostart: false, so '{name}' waits until '{dep}' is started by hand"
                    ),
                ));
            }
            // Command-less services are infra probed by port; only a spawned
            // command with nothing to probe makes `healthy` meaningless.
            if cond == Condition::Healthy && target.command.is_some() && target.has_no_probe() {
                out.push(warn(
                    path,
                    code::WARN_HEALTHY_NO_PROBE,
                    json!({ "service": name, "dependency": dep }),
                    format!(
                        "'healthy' on '{dep}' only waits for it to stay alive for 2s: \
                         '{dep}' has no port, health or ready_log"
                    ),
                ));
            }
        }
    }
    out
}

/// First dependency cycle (as a closed path `a -> b -> a`), if any. Edges to
/// unknown services are ignored (reported separately).
fn find_cycle(
    services: &BTreeMap<&str, &crate::config::project::ServiceDef>,
    names: &BTreeSet<&str>,
) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn visit<'a>(
        n: &'a str,
        services: &BTreeMap<&'a str, &'a crate::config::project::ServiceDef>,
        names: &BTreeSet<&'a str>,
        marks: &mut HashMap<&'a str, Mark>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        match marks.get(n) {
            Some(Mark::Done) => return None,
            Some(Mark::Visiting) => {
                let at = stack.iter().position(|s| *s == n).unwrap_or(0);
                let mut cycle: Vec<String> = stack[at..].iter().map(|s| s.to_string()).collect();
                cycle.push(n.to_string());
                return Some(cycle);
            }
            None => {}
        }
        marks.insert(n, Mark::Visiting);
        stack.push(n);
        if let Some(def) = services.get(n) {
            for (dep, _) in def.deps() {
                let Some(dep) = names.get(dep.as_str()).copied() else {
                    continue;
                };
                if dep == n {
                    continue; // self-dependency reported on its own
                }
                if let Some(c) = visit(dep, services, names, marks, stack) {
                    return Some(c);
                }
            }
        }
        stack.pop();
        marks.insert(n, Mark::Done);
        None
    }
    let mut marks = HashMap::new();
    for n in names {
        if let Some(c) = visit(n, services, names, &mut marks, &mut Vec::new()) {
            return Some(c);
        }
    }
    None
}
