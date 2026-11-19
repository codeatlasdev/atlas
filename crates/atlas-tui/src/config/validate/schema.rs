//! Schema-driven unknown-key detection for atlas.yaml / atlas.local.yaml.

use serde_json::Value as Json;
use serde_json::json;
use serde_norway::Value as Yaml;

use super::{ConfigError, code, suggest};
use crate::config::loader::LocalOverride;
use crate::config::project::ProjectConfig;

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
    let (code, args, message) = match suggestion {
        Some(s) => (
            code::UNKNOWN_KEY_SUGGEST,
            json!({ "key": key, "suggestion": s }),
            format!("unknown field (did you mean '{s}'?)"),
        ),
        None => (
            code::UNKNOWN_KEY,
            json!({ "key": key }),
            "unknown field".to_string(),
        ),
    };
    let mut error = ConfigError::new(path, code, args, message);
    error.file = file.to_string();
    error
}
