//! Config validation: unknown-key detection (schema-driven, with "did you
//! mean" suggestions) and semantic checks (dependencies, cycles, ports, keys).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use serde_json::Value as Json;
use serde_yaml::Value as Yaml;

use super::loader::LocalOverride;
use super::project::{Condition, ProjectConfig};

/// Keys bound by the dashboard (`tui/app/input.rs`); an action using one would
/// never fire (built-ins win) or would shadow navigation.
pub const BUILTIN_KEYS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'j', 'k', 'g', 'G', 'f', 'v', 'E', 'M', 'L',
    'P', 'q', 'r', 's', 'S', 't', 'm', 'i', '?', ':', '/', 'n', 'N', 'W', 'T', 'V', 'J', 'z', '-',
    'D', '[', ']', '{', '}',
];

/// One problem found in the config. `path` is the dotted key path
/// (`services.web.helth`), `message` the human explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: String,
    pub path: String,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// One line per error, prefixed with the file it came from.
pub fn render_errors(errors: &[ConfigError], default_file: &str) -> String {
    let mut out = format!(
        "{} config error{}:",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" }
    );
    for e in errors {
        let file = if e.file.is_empty() {
            default_file
        } else {
            &e.file
        };
        out.push_str(&format!("\n  {file}: {e}"));
    }
    out
}

/// Which document shape to validate keys against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// atlas.yaml → `ProjectConfig`.
    Project,
    /// atlas.local.yaml → `LocalOverride`.
    Local,
}

/// Draft-07 (`definitions`, `anyOf` for options): what editors and [`walk`] expect.
fn generator() -> schemars::SchemaGenerator {
    schemars::generate::SchemaSettings::draft07().into_generator()
}

/// JSON Schema (draft-07) for atlas.yaml, pretty-printed.
pub fn json_schema() -> String {
    let schema = generator().into_root_schema_for::<ProjectConfig>();
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}

fn schema_json(target: Target) -> Json {
    let root = match target {
        Target::Project => generator().into_root_schema_for::<ProjectConfig>(),
        Target::Local => generator().into_root_schema_for::<LocalOverride>(),
    };
    root.to_value()
}

// ── unknown keys ─────────────────────────────────────────────────────────

/// Report every key in `doc` the schema doesn't know, with a suggestion when a
/// known key is within edit distance.
pub fn unknown_keys(doc: &Yaml, target: Target, file: &str) -> Vec<ConfigError> {
    let root = schema_json(target);
    let mut errors = Vec::new();
    walk(&root, &root, doc, "", file, &mut errors);
    errors
}

/// Follow `$ref: "#/definitions/Name"` until a concrete schema.
fn resolve<'a>(root: &'a Json, mut schema: &'a Json) -> &'a Json {
    while let Some(r) = schema.get("$ref").and_then(Json::as_str) {
        let name = r.rsplit('/').next().unwrap_or_default();
        match root.get("definitions").and_then(|d| d.get(name)) {
            Some(next) => schema = next,
            None => break,
        }
    }
    schema
}

fn walk(
    root: &Json,
    schema: &Json,
    value: &Yaml,
    path: &str,
    file: &str,
    out: &mut Vec<ConfigError>,
) {
    let schema = resolve(root, schema);

    // Unions (Option<T> → anyOf [T, null]; untagged enums → anyOf/oneOf): descend
    // into every branch that can describe this value's shape; a key is unknown
    // only if no branch accepts it, so keep each branch's errors separate and
    // report the branch with the fewest.
    for key in ["anyOf", "oneOf"] {
        if let Some(branches) = schema.get(key).and_then(Json::as_array) {
            let mut best: Option<Vec<ConfigError>> = None;
            for b in branches {
                if !shape_matches(root, b, value) {
                    continue;
                }
                let mut errs = Vec::new();
                walk(root, b, value, path, file, &mut errs);
                if best.as_ref().is_none_or(|prev| errs.len() < prev.len()) {
                    best = Some(errs);
                }
            }
            out.extend(best.unwrap_or_default());
            return;
        }
    }
    if let Some(all) = schema.get("allOf").and_then(Json::as_array) {
        for b in all {
            walk(root, b, value, path, file, out);
        }
        return;
    }

    match value {
        Yaml::Mapping(map) => {
            if let Some(props) = schema.get("properties").and_then(Json::as_object) {
                for (k, v) in map {
                    let Some(key) = k.as_str() else { continue };
                    let child_path = join(path, key);
                    match props.get(key) {
                        Some(child_schema) => walk(root, child_schema, v, &child_path, file, out),
                        None if schema.get("additionalProperties") == Some(&Json::Bool(false)) => {
                            out.push(unknown(
                                file,
                                &child_path,
                                key,
                                props.keys().map(String::as_str),
                            ));
                        }
                        None => {}
                    }
                }
            } else if let Some(extra) = schema.get("additionalProperties").filter(|a| a.is_object())
            {
                // Map<String, T>: every value follows `extra`.
                for (k, v) in map {
                    let Some(key) = k.as_str() else { continue };
                    walk(root, extra, v, &join(path, key), file, out);
                }
            }
        }
        Yaml::Sequence(items) => {
            if let Some(item_schema) = schema.get("items").filter(|i| i.is_object()) {
                for (i, v) in items.iter().enumerate() {
                    walk(root, item_schema, v, &format!("{path}[{i}]"), file, out);
                }
            }
        }
        _ => {}
    }
}

/// Cheap structural check: does this branch's declared `type` fit the YAML node?
fn shape_matches(root: &Json, schema: &Json, value: &Yaml) -> bool {
    let schema = resolve(root, schema);
    let ty = match schema.get("type") {
        Some(Json::String(t)) => vec![t.as_str()],
        Some(Json::Array(a)) => a.iter().filter_map(Json::as_str).collect(),
        _ => return true,
    };
    ty.iter().any(|t| {
        matches!(
            (*t, value),
            ("object", Yaml::Mapping(_))
                | ("array", Yaml::Sequence(_))
                | ("null", Yaml::Null)
                | ("string", Yaml::String(_))
                | ("boolean", Yaml::Bool(_))
                | ("integer" | "number", Yaml::Number(_))
        )
    })
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn unknown<'a>(
    file: &str,
    path: &str,
    key: &str,
    known: impl Iterator<Item = &'a str>,
) -> ConfigError {
    let suggestion = suggest(key, known);
    let message = match suggestion {
        Some(s) => format!("unknown field (did you mean '{s}'?)"),
        None => "unknown field".to_string(),
    };
    ConfigError {
        file: file.to_string(),
        path: path.to_string(),
        message,
    }
}

/// Closest candidate within a distance that scales with the key length.
pub fn suggest<'a>(key: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let max = (key.chars().count() / 3).clamp(1, 3);
    candidates
        .map(|c| (levenshtein(key, c), c))
        .filter(|(d, _)| *d <= max)
        .min_by_key(|(d, c)| (*d, *c))
        .map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

// ── semantic checks ──────────────────────────────────────────────────────

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
    // Sorted for deterministic output (HashMap iteration order is random).
    let sorted: BTreeMap<&str, &super::project::ServiceDef> = config
        .services
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();

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
        if let Some(m) = &def.max_memory
            && crate::runtime::usage::parse_size(m).is_none()
        {
            errors.push(err(
                format!("services.{name}.max_memory"),
                format!("'{m}' is not a size (use e.g. 512M or 4G)"),
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

fn deploy_rules(d: &super::project::DeployConfig, errors: &mut Vec<ConfigError>) {
    use super::project::DeployKind;
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
    let sorted: BTreeMap<&str, &super::project::ServiceDef> = config
        .services
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();
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
    services: &BTreeMap<&str, &super::project::ServiceDef>,
    names: &BTreeSet<&str>,
) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }
    fn visit<'a>(
        n: &'a str,
        services: &BTreeMap<&'a str, &'a super::project::ServiceDef>,
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
