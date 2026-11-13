//! Cross-field checks on the merged config (dependencies, cycles, ports,
//! keys, deploy rules) and non-fatal warnings.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{BUILTIN_KEYS, ConfigError, suggest};
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
    let err = |path: String, message: String| ConfigError {
        file: String::new(),
        path,
        message,
    };
    let names: BTreeSet<&str> = config.services.keys().map(String::as_str).collect();
    let sorted = sorted_services(config);

    for (name, def) in &sorted {
        for (dep, cond) in def.deps() {
            let path = format!("services.{name}.depends_on.{dep}");
            if dep == *name {
                errors.push(err(path, "a service cannot depend on itself".into()));
                continue;
            }
            let Some(target) = sorted.get(dep.as_str()) else {
                let hint = suggest(&dep, names.iter().copied())
                    .map(|s| format!(" (did you mean '{s}'?)"))
                    .unwrap_or_default();
                errors.push(err(path, format!("unknown service '{dep}'{hint}")));
                continue;
            };
            match cond {
                Condition::LogReady if target.ready_log.is_none() => errors.push(err(
                    path,
                    format!("condition 'log_ready' requires services.{dep}.ready_log"),
                )),
                Condition::CompletedSuccessfully if target.command.is_none() => errors.push(err(
                    path,
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
                format!("must be below max_memory ({max_text})"),
            ));
        }
        if let Some(rx) = &def.ready_log
            && let Err(e) = regex::Regex::new(rx)
        {
            errors.push(err(
                format!("services.{name}.ready_log"),
                format!(
                    "invalid regex: {}",
                    e.to_string().lines().last().unwrap_or("")
                ),
            ));
        }
        if let Some(r) = &def.ready
            && r.http.is_some() == r.command.is_some()
        {
            errors.push(err(
                format!("services.{name}.ready"),
                "set exactly one of `http` (path or URL) or `command`".into(),
            ));
        }
    }

    if let Some(cycle) = find_cycle(&sorted, &names) {
        errors.push(err(
            "services".into(),
            format!("dependency cycle: {}", cycle.join(" -> ")),
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
            format!("port {port} is also used by '{}'", owners[0]),
        ));
    }

    // Profiles: every member must be a known service.
    for (profile, members) in &config.profiles {
        for (i, member) in members.iter().enumerate() {
            if names.contains(member.as_str()) {
                continue;
            }
            let hint = suggest(member, names.iter().copied())
                .map(|s| format!(" (did you mean '{s}'?)"))
                .unwrap_or_default();
            errors.push(err(
                format!("profiles.{profile}[{i}]"),
                format!("unknown service '{member}'{hint}"),
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
                format!("key '{}' is a built-in dashboard key", a.key),
            ));
        } else if let Some(svc) = open_keys.get(&a.key) {
            errors.push(err(
                path,
                format!("key '{}' is already the open_key of service '{svc}'", a.key),
            ));
        } else if let Some(prev) = seen.insert(a.key, &a.label) {
            errors.push(err(
                path,
                format!("key '{}' is already used by action '{prev}'", a.key),
            ));
        }
    }
    for (k, svc) in &open_keys {
        if BUILTIN_KEYS.contains(k) {
            errors.push(err(
                format!("services.{svc}.open_key"),
                format!("key '{k}' is a built-in dashboard key"),
            ));
        }
    }

    // `open_tunnel: true` has nothing to open without a `tunnel:` block.
    if config.tunnel.is_none() {
        for (i, a) in config.actions.iter().enumerate() {
            if a.open_tunnel {
                errors.push(err(
                    format!("actions[{i}].open_tunnel"),
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

    errors
}

fn deploy_rules(d: &crate::config::project::DeployConfig, errors: &mut Vec<ConfigError>) {
    use crate::config::project::DeployKind;
    let mut e = |path: String, message: &str| {
        errors.push(ConfigError {
            file: String::new(),
            path,
            message: message.into(),
        })
    };
    if let Some(app) = &d.app
        && (app.is_empty()
            || !app
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
    {
        e("deploy.app".into(), "must match ^[a-z0-9-]+$");
    }
    if d.targets.is_empty() {
        e("deploy.targets".into(), "at least one target is required");
    }
    let targets: BTreeMap<_, _> = d.targets.iter().collect();
    for (t, tg) in targets {
        let p = |f: &str| format!("deploy.targets.{t}.{f}");
        if tg.services.is_empty() {
            e(p("services"), "at least one service is required");
        }
        if tg.keep.is_some_and(|k| k < 2) {
            e(
                p("keep"),
                "must be >= 2 (rollback needs a previous release)",
            );
        }
        if tg.dir.as_deref().is_some_and(|s| !s.starts_with('/')) {
            e(p("dir"), "must be an absolute path");
        }
        if tg
            .pause_marker
            .as_deref()
            .is_some_and(|s| !s.starts_with('/'))
        {
            e(p("pause_marker"), "must be an absolute path");
        }
        let links: BTreeMap<_, _> = tg.link.iter().flatten().collect();
        for (k, v) in links {
            if k.is_empty() || k.starts_with('/') || k.split('/').any(|c| c == "..") {
                e(
                    p(&format!("link.{k}")),
                    "key must be a release-relative path without '..'",
                );
            }
            if !v.starts_with('/') {
                e(p(&format!("link.{k}")), "value must be an absolute path");
            }
        }
        let mut seen = BTreeSet::new();
        for (i, s) in tg.services.iter().enumerate() {
            let sp = |f: &str| p(&format!("services[{i}].{f}"));
            if !seen.insert(s.name.as_str()) {
                e(sp("name"), &format!("duplicate service '{}'", s.name));
            }
            if s.kind == DeployKind::Command && s.restart.is_none() {
                e(sp("restart"), "kind 'command' requires restart");
            }
            if s.kind != DeployKind::Command && s.restart.is_some() {
                e(sp("restart"), "only valid for kind 'command'");
            }
            if s.kind != DeployKind::Compose && s.compose_file.is_some() {
                e(sp("compose_file"), "only valid for kind 'compose'");
            }
        }
    }
}

/// Non-fatal findings: the config loads and runs, but something is likely
/// not what the author meant. `atlas config check` prints them and still exits 0.
pub fn warnings(config: &ProjectConfig) -> Vec<ConfigError> {
    let sorted = sorted_services(config);
    let mut out = Vec::new();
    let warn = |path: String, message: String| ConfigError {
        file: String::new(),
        path,
        message,
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
