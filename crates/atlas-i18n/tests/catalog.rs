//! The deterministic check of the catalogs: what `cargo test -p atlas-i18n` says about
//! `locales/`. Faster feedback on the same files, with lock files, glossary and pt-PT words,
//! comes from `bun run i18n:check`; this file owns what needs the Rust source (ids used).
//!
//! Every failure names the file and id so an agent can fix it without reading the catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

fn locales_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("locales")
}

/// `lang -> area -> (source text)`.
fn load() -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for lang in fs::read_dir(locales_dir()).unwrap().flatten() {
        if !lang.path().is_dir() {
            continue;
        }
        let mut areas = BTreeMap::new();
        for f in fs::read_dir(lang.path()).unwrap().flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "ftl") {
                let area = p.file_stem().unwrap().to_string_lossy().into_owned();
                areas.insert(area, fs::read_to_string(&p).unwrap());
            }
        }
        out.insert(lang.file_name().to_string_lossy().into_owned(), areas);
    }
    out
}

#[derive(Debug)]
struct Message {
    id: String,
    vars: BTreeSet<String>,
    has_comment: bool,
}

fn collect_inline(e: &InlineExpression<&str>, vars: &mut BTreeSet<String>) {
    match e {
        // `$os` is ambient (the i18n layer adds it to every call), so a language may select on it
        // or not without the variable sets differing.
        InlineExpression::VariableReference { id } => {
            if id.name != "os" {
                vars.insert(id.name.to_string());
            }
        }
        InlineExpression::Placeable { expression } => collect_expr(expression, vars),
        InlineExpression::FunctionReference { arguments, .. } => {
            for a in &arguments.positional {
                collect_inline(a, vars);
            }
            for n in &arguments.named {
                collect_inline(&n.value, vars);
            }
        }
        _ => {}
    }
}

fn collect_expr(e: &Expression<&str>, vars: &mut BTreeSet<String>) {
    match e {
        Expression::Inline(i) => collect_inline(i, vars),
        Expression::Select { selector, variants } => {
            collect_inline(selector, vars);
            for v in variants {
                collect_pattern(&v.value, vars);
            }
        }
    }
}

fn collect_pattern(p: &Pattern<&str>, vars: &mut BTreeSet<String>) {
    for el in &p.elements {
        if let PatternElement::Placeable { expression } = el {
            collect_expr(expression, vars);
        }
    }
}

fn messages(file: &str, src: &str) -> Vec<Message> {
    let res = match fluent_syntax::parser::parse(src) {
        Ok(r) => r,
        Err((_, errs)) => panic!("{file}: does not parse: {errs:?}"),
    };
    res.body
        .iter()
        .filter_map(|e| match e {
            Entry::Message(m) => {
                let mut vars = BTreeSet::new();
                if let Some(v) = &m.value {
                    collect_pattern(v, &mut vars);
                }
                for a in &m.attributes {
                    collect_pattern(&a.value, &mut vars);
                }
                Some(Message {
                    id: m.id.name.to_string(),
                    vars,
                    has_comment: m.comment.is_some(),
                })
            }
            _ => None,
        })
        .collect()
}

/// `lang -> id -> (area, message)`; fails on a duplicate id inside one language.
fn index() -> BTreeMap<String, BTreeMap<String, (String, Message)>> {
    let mut out = BTreeMap::new();
    for (lang, areas) in load() {
        let mut ids: BTreeMap<String, (String, Message)> = BTreeMap::new();
        for (area, src) in areas {
            let file = format!("locales/{lang}/{area}.ftl");
            for m in messages(&file, &src) {
                if let Some((other, _)) = ids.get(&m.id) {
                    panic!(
                        "{file}: id `{}` is also defined in locales/{lang}/{other}.ftl",
                        m.id
                    );
                }
                ids.insert(m.id.clone(), (area.clone(), m));
            }
        }
        out.insert(lang, ids);
    }
    out
}

#[test]
fn locale_directories_are_exactly_the_supported_languages() {
    let found: BTreeSet<String> = load().into_keys().collect();
    let supported: BTreeSet<String> = atlas_core::lang::SUPPORTED
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(
        found, supported,
        "locales/ directories and atlas_core::lang::SUPPORTED must list the same languages"
    );
    assert_eq!(
        atlas_i18n::locales().collect::<BTreeSet<_>>(),
        supported
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        "the embedded catalogs must be the ones on disk"
    );
}

#[test]
fn every_file_parses_and_ids_are_unique_within_a_language() {
    // `index` panics with the file and id on junk or a duplicate.
    assert!(index().get("en").is_some_and(|m| !m.is_empty()));
}

#[test]
fn every_language_has_the_same_files_ids_and_variables_as_english() {
    let idx = index();
    let en = &idx["en"];
    let areas = |lang: &str| -> BTreeSet<String> { load()[lang].keys().cloned().collect() };
    let mut problems = Vec::new();
    for (lang, ids) in &idx {
        if lang == "en" {
            continue;
        }
        if areas(lang) != areas("en") {
            problems.push(format!(
                "locales/{lang}: files differ from locales/en: {:?} vs {:?}",
                areas(lang),
                areas("en")
            ));
        }
        for (id, (area, m)) in en {
            match ids.get(id) {
                None => problems.push(format!("locales/{lang}/{area}.ftl: missing `{id}`")),
                Some((a, t)) => {
                    if a != area {
                        problems.push(format!(
                            "locales/{lang}/{a}.ftl: `{id}` is in locales/en/{area}.ftl"
                        ));
                    }
                    if t.vars != m.vars {
                        problems.push(format!(
                            "locales/{lang}/{a}.ftl: `{id}` uses variables {:?}, English uses {:?}",
                            t.vars, m.vars
                        ));
                    }
                }
            }
        }
        for (id, (a, _)) in ids {
            if !en.contains_key(id) {
                problems.push(format!(
                    "locales/{lang}/{a}.ftl: `{id}` has no English message"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_english_message_has_a_description_comment() {
    let missing: Vec<String> = index()["en"]
        .iter()
        .filter(|(_, (_, m))| !m.has_comment)
        .map(|(id, (area, _))| {
            format!("locales/en/{area}.ftl: `{id}` has no `#` description above it")
        })
        .collect();
    assert!(missing.is_empty(), "\n{}", missing.join("\n"));
}

/// Every `.rs` file under `crates/*/src` except this crate's tests.
fn rust_sources() -> Vec<(PathBuf, String)> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = fs::read_dir(&crates)
        .unwrap()
        .flatten()
        .map(|e| e.path().join("src"))
        .filter(|p| p.is_dir())
        .collect();
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = fs::read_to_string(&p).unwrap();
                out.push((p, text));
            }
        }
    }
    out
}

/// The ids passed to `t!("...")` in `src`, with their line.
fn t_calls(src: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in src.lines().enumerate() {
        // Doc comments and examples mention `t!("id")` without calling it.
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut rest = line;
        let mut offset = 0;
        while let Some(i) = rest.find("t!(\"") {
            let before = line[..offset + i].chars().next_back();
            // `format!("` and `assert!("` end in `t!(`: only a standalone `t!` is ours.
            let standalone = before.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            let start = i + 4;
            if let Some(len) = rest[start..].find('"') {
                if standalone {
                    out.push((n + 1, rest[start..start + len].to_string()));
                }
                offset += start + len;
                rest = &rest[start + len..];
            } else {
                break;
            }
        }
    }
    out
}

#[test]
fn every_t_macro_id_exists_in_english() {
    let en = &index()["en"];
    let mut problems = Vec::new();
    for (path, text) in rust_sources() {
        for (line, id) in t_calls(&text) {
            if !en.contains_key(&id) {
                problems.push(format!(
                    "{}:{line}: t!(\"{id}\") has no message in locales/en",
                    path.display()
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

// An id nobody uses is dead weight in front of a translator. The `cli` area is generated from
// the clap tree and referenced by computed ids, and `terms` are shared words other messages
// reference as `{ term-x }` once screens migrate, so both are exempt.
#[test]
fn every_english_id_outside_the_computed_areas_is_used() {
    let sources = rust_sources();
    let unused: Vec<String> = index()["en"]
        .iter()
        .filter(|(_, (area, _))| area != "cli" && area != "terms")
        .filter(|(id, _)| {
            let quoted = format!("\"{id}\"");
            !sources.iter().any(|(_, t)| t.contains(&quoted))
        })
        .map(|(id, (area, _))| {
            format!("locales/en/{area}.ftl: `{id}` is not referenced from any crates/*/src file")
        })
        .collect();
    assert!(unused.is_empty(), "\n{}", unused.join("\n"));
}

#[test]
fn t_call_scanner_ignores_format_and_assert_macros() {
    let src = "// t!(\"in-a-comment\")\nlet a = format!(\"x\");\nlet b = t!(\"real-id\");\nassert!(\"y\".is_empty());\nlet c = t!(\"two\", n = 1) + &t!(\"three\");\n";
    let got: Vec<String> = t_calls(src).into_iter().map(|(_, id)| id).collect();
    assert_eq!(got, ["real-id", "two", "three"]);
}

// macOS reports the development region to the webview and native dialogs unless the bundle
// declares its languages, so Info.plist must list exactly the supported ones.
#[test]
fn desktop_info_plist_declares_the_supported_languages() {
    let plist = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../atlas-desktop/Info.plist"),
    )
    .unwrap();
    let after = plist
        .split("<key>CFBundleLocalizations</key>")
        .nth(1)
        .expect("CFBundleLocalizations key");
    let array = after.split("</array>").next().unwrap();
    let declared: BTreeSet<String> = array
        .split("<string>")
        .skip(1)
        .map(|s| s.split("</string>").next().unwrap().to_string())
        .collect();
    let supported: BTreeSet<String> = atlas_core::lang::SUPPORTED
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(
        declared, supported,
        "crates/atlas-desktop/Info.plist CFBundleLocalizations"
    );
}
