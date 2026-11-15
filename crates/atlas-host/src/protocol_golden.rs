//! The wire protocol's shape, pinned (#302).
//!
//! `testdata/hostd/commands.json` lists every command of [`HostCmd`] with its argument names (required
//! and optional), next to the [`PROTO`] it was written for. The list is read from the real enum's
//! source and each entry is checked against the real deserializer, so it cannot drift from the types.
//! A change to it fails the test unless `PROTO` moved too:
//!
//! - an ADDITIVE change (a new command, a new optional argument): regenerate the fixture
//!   (`UPDATE_FIXTURES=1 cargo test -p atlas-host protocol_golden`), `PROTO` stays;
//! - a NON-ADDITIVE change (a command or argument removed or renamed, a required argument added, a
//!   changed meaning): bump `PROTO` in `protocol.rs` and `kAppProto` / `kMinHostProto` in the phone
//!   (`apps/mobile/lib/core/protocol/protocol_version.dart`), then regenerate.
//!
//! The same rule pins the keys of `info` (`server.rs`, `testdata/hostd/info.json`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::protocol::{PROTO, Request};

pub(crate) const FAIL_HELP: &str = "the hostd wire shape changed. Additive changes (a new command, a new optional argument or \
     info key): regenerate the fixture with UPDATE_FIXTURES=1. Non-additive changes (something removed or \
     renamed, a required argument added): bump PROTO in protocol.rs (and the phone's kAppProto / \
     kMinHostProto), then regenerate.";

pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/hostd")
        .join(name)
}

/// Compares `live` (which carries the `proto` it describes) with the committed fixture `name`.
///
/// Equal: pass. Different with the same `proto`: the shape moved without a bump, so only an additive
/// change may be accepted by regenerating; the test fails and says which of the two applies. A
/// different `proto` with a stale fixture fails the same way, so the fixture always names the
/// protocol it describes.
pub(crate) fn check_golden(name: &str, live: &Value) {
    let path = fixture(name);
    let live = canonical(live);
    let rendered = serde_json::to_string_pretty(&live).unwrap() + "\n";
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &rendered).unwrap();
    }
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{name} is missing: UPDATE_FIXTURES=1 cargo test -p atlas-host protocol_golden")
        }))
        .unwrap();
    let stored = canonical(&stored);
    if stored == live {
        return;
    }
    let was = stored["proto"].as_u64().unwrap_or(0);
    let now = live["proto"].as_u64().unwrap_or(0);
    let diff = diff_lines(&stored, &live);
    if was == now {
        panic!("{name} is stale at PROTO {now}: {FAIL_HELP}\n{diff}");
    }
    panic!(
        "{name} describes PROTO {was} but PROTO is {now}: regenerate it (UPDATE_FIXTURES=1).\n{diff}"
    );
}

/// `v` with every object's keys in sorted order. serde_json keeps insertion order only when its
/// `preserve_order` feature is on, and feature unification turns that on in some builds and not in
/// others (the Windows full-workspace build has it, a plain `-p atlas-host` build does not): without
/// this the same protocol compared unequal on Windows and the fixture's bytes depended on who wrote it.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), canonical(&m[k])))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

fn diff_lines(old: &Value, new: &Value) -> String {
    let a = serde_json::to_string_pretty(old).unwrap();
    let b = serde_json::to_string_pretty(new).unwrap();
    let a: Vec<&str> = a.lines().collect();
    let b: Vec<&str> = b.lines().collect();
    let mut out = String::new();
    for l in a.iter().filter(|l| !b.contains(l)) {
        out += &format!("- {l}\n");
    }
    for l in b.iter().filter(|l| !a.contains(l)) {
        out += &format!("+ {l}\n");
    }
    out
}

/// One command: its wire name and its argument names.
#[derive(Debug, Default, PartialEq)]
struct Cmd {
    required: Vec<String>,
    optional: Vec<String>,
}

fn snake(camel: &str) -> String {
    let mut s = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                s.push('_');
            }
            s.push(c.to_ascii_lowercase());
        } else {
            s.push(c);
        }
    }
    s
}

/// The `HostCmd` enum as `protocol.rs` declares it: `wire name -> arguments`.
///
/// Reads the enum's own text (variants, fields, `#[serde(rename)]`, `#[serde(default)]`, `Option`), so a
/// new variant or field shows up without anyone remembering to list it. The one tuple variant
/// (`PtyCreate(PtyCreate)`) takes its arguments from the struct it wraps.
fn commands() -> BTreeMap<String, Cmd> {
    let src = include_str!("protocol.rs");
    let start = src.find("pub enum HostCmd {").expect("HostCmd enum");
    let body = &src[start..];
    let end = body.find("\n}\n").expect("end of HostCmd");
    let body = &body[..end];

    let mut out = BTreeMap::new();
    let mut rename: Option<String> = None;
    let mut cur: Option<(String, Cmd)> = None;
    let mut default = false;
    let mut field_rename: Option<String> = None;
    for raw in body.lines().skip(1) {
        let line = raw.trim();
        let indent = raw.len() - raw.trim_start().len();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if indent == 4 {
            if let Some(r) = line.strip_prefix("#[serde(rename = \"") {
                rename = Some(r.trim_end_matches("\")]").to_string());
                continue;
            }
            if line.starts_with("#[") {
                continue;
            }
            if line == "}," {
                let (name, cmd) = cur.take().expect("open variant");
                out.insert(name, cmd);
                continue;
            }
            let ident: String = line
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let rest = line[ident.len()..].trim();
            let name = rename.take().unwrap_or_else(|| snake(&ident));
            match rest {
                "{}," => {
                    out.insert(name, Cmd::default());
                }
                "{" => cur = Some((name, Cmd::default())),
                r if r.starts_with('(') => {
                    out.insert(name, Cmd::default()); // tuple variant: filled from its struct below
                }
                other => panic!("HostCmd variant not understood: {line} ({other})"),
            }
        } else if indent == 8 {
            if line.starts_with("#[serde(default") {
                default = true;
                continue;
            }
            if let Some(r) = line.strip_prefix("#[serde(rename = \"") {
                field_rename = Some(r.trim_end_matches("\")]").to_string());
                continue;
            }
            if line.starts_with("#[") {
                continue;
            }
            let (field, ty) = line.split_once(':').expect("a field line");
            let (field, ty) = (
                field_rename
                    .take()
                    .unwrap_or_else(|| field.trim().to_string()),
                ty.trim(),
            );
            let (_, cmd) = cur.as_mut().expect("field outside a variant");
            if default || ty.starts_with("Option<") {
                cmd.optional.push(field);
            } else {
                cmd.required.push(field);
            }
            default = false;
        }
        // deeper lines belong to a multi-line type or attribute: not a field of their own
    }
    assert!(cur.is_none(), "HostCmd parse ended inside a variant");

    // PtyCreate wraps a struct: its fields are the command's arguments.
    let pc = src
        .find("pub struct PtyCreate {")
        .expect("PtyCreate struct");
    let pc = &src[pc..];
    let pc = &pc[..pc.find("\n}\n").unwrap()];
    let mut cmd = Cmd::default();
    let mut default = false;
    let mut field_rename: Option<String> = None;
    for raw in pc.lines().skip(1) {
        let line = raw.trim();
        if line.starts_with("#[serde(default") {
            default = true;
        } else if let Some(r) = line.strip_prefix("#[serde(rename = \"") {
            field_rename = Some(r.trim_end_matches("\")]").to_string());
        } else if line.is_empty() || line.starts_with("//") || line.starts_with("#[") {
        } else if let Some((f, ty)) = line.split_once(':') {
            let f = field_rename
                .take()
                .unwrap_or_else(|| f.trim().trim_start_matches("pub ").to_string());
            if default || ty.trim().starts_with("Option<") {
                cmd.optional.push(f);
            } else {
                cmd.required.push(f);
            }
            default = false;
        }
    }
    out.insert("pty_create".into(), cmd);
    for c in out.values_mut() {
        c.required.sort();
        c.optional.sort();
    }
    out
}

#[test]
fn commands_fixture_matches_the_real_enum() {
    let cmds = commands();
    let live = json!({
        "proto": PROTO,
        "commands": cmds.iter().map(|(n, c)| (n.clone(), json!({
            "required": c.required,
            "optional": c.optional,
        }))).collect::<serde_json::Map<_, _>>(),
    });
    check_golden("commands.json", &live);
}

/// The golden is only worth something if it reads what serde reads. serde reports the first missing
/// required field of `args: {}`; supplying each reported field (as a string, which any String-typed
/// field takes, else the next candidate) until the command parses yields the real required set.
/// That set must equal the one the golden lists, for every command, and an unknown name must be refused.
#[test]
fn the_listed_commands_are_the_ones_the_deserializer_accepts() {
    let cmds = commands();
    assert!(
        cmds.len() > 100,
        "the parser found only {} commands",
        cmds.len()
    );
    let samples = [json!(""), json!(false), json!(0), json!([]), json!({})];
    for (name, c) in &cmds {
        let mut args = serde_json::Map::new();
        let mut required = Vec::new();
        loop {
            let line = json!({"id": 1, "cmd": name, "args": args}).to_string();
            let Err(e) = serde_json::from_str::<Request>(&line) else {
                break;
            };
            let msg = e.to_string();
            let field = msg
                .strip_prefix("missing field `")
                .and_then(|r| r.split('`').next())
                .unwrap_or_else(|| panic!("{name}: unexpected error {msg}"))
                .to_string();
            // A wrong type for this field is reported as "invalid type"; try the next sample.
            let fits = samples.iter().find(|s| {
                let mut probe = args.clone();
                probe.insert(field.clone(), (*s).clone());
                let line = json!({"id": 1, "cmd": name, "args": probe}).to_string();
                serde_json::from_str::<Request>(&line)
                    .err()
                    .is_none_or(|e| !e.to_string().contains("invalid type"))
            });
            args.insert(field.clone(), fits.cloned().unwrap_or(Value::Null));
            required.push(field);
        }
        required.sort();
        assert_eq!(&required, &c.required, "{name}: required arguments");
    }
    let nope = json!({"id": 1, "cmd": "definitely_not_a_command", "args": {}}).to_string();
    assert!(serde_json::from_str::<Request>(&nope).is_err());
}
