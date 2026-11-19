//! The codes of the config problems, through the real producers: real YAML into `load_checked` and `warnings`,
//! never a hand-built [`ConfigError`].

use std::collections::BTreeSet;

use serde_json::Value;
use tempfile::TempDir;

use super::samples::samples;
use super::{ConfigError, code, render_errors_in};
use crate::config::{load_checked, warnings};

const EN: &str = atlas_core::lang::DEFAULT;
const PT: &str = "pt-BR";

/// The problems (errors, then warnings) the real validators report for `yaml`, and the errors alone.
fn produced(yaml: &str) -> (Vec<ConfigError>, Vec<ConfigError>) {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
    let (config, errors) = load_checked(dir.path()).unwrap();
    let mut all = errors.clone();
    all.extend(warnings(&config));
    (all, errors)
}

/// Every sample with the problems of its own code (a sample may trigger neighbours: a cycle also earns the
/// `healthy` warnings).
fn emitted() -> Vec<(&'static str, String, Vec<ConfigError>)> {
    samples()
        .into_iter()
        .map(|(code, yaml)| {
            let (all, _) = produced(&yaml);
            let own: Vec<ConfigError> = all.into_iter().filter(|e| e.code == code).collect();
            assert!(
                !own.is_empty(),
                "the sample for {code} produced no problem with that code:\n{yaml}"
            );
            (code, yaml, own)
        })
        .collect()
}

fn catalog_key(code: &str) -> String {
    format!(
        "tui_config_problems_{}",
        code.strip_prefix("config.").unwrap().replace('.', "_")
    )
}

/// Every code a validator source names must be in `code::ALL`: a code added to schema.rs or semantic.rs without
/// a constant, a sample and a catalog message fails here.
#[test]
fn every_code_the_validators_can_emit_is_listed_sampled_and_in_the_catalog() {
    for (name, source) in [
        ("schema.rs", include_str!("schema.rs")),
        ("semantic.rs", include_str!("semantic.rs")),
    ] {
        assert!(
            !source.contains("\"config."),
            "{name} spells a code as a literal: use a constant of `validate::code`"
        );
        for line in source.lines().filter(|l| l.contains("code::")) {
            for word in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == ':' || c == '_')) {
                if let Some(constant) = word.strip_prefix("code::") {
                    assert!(
                        constant.is_empty()
                            || constant.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                        "{name}: `{word}`"
                    );
                }
            }
        }
    }

    let listed: BTreeSet<&str> = code::ALL.iter().copied().collect();
    assert_eq!(listed.len(), code::ALL.len(), "a code is listed twice");

    let sampled: BTreeSet<&str> = emitted().into_iter().map(|(c, _, _)| c).collect();
    assert_eq!(
        sampled, listed,
        "every code needs a real YAML sample that makes the validators emit it, and no sample may name another"
    );

    for (code, yaml, errors) in emitted() {
        for e in errors {
            for tag in [EN, PT] {
                assert!(
                    e.catalog_text(tag).is_some(),
                    "the {tag} catalog has no message {} for the code {code} (args {:?})\n{yaml}",
                    catalog_key(code),
                    e.args
                );
            }
        }
    }
}

/// The English of the catalog is the English `message` of the producer, for the args the producer wrote.
#[test]
fn the_english_catalog_renders_the_producers_message_for_every_code() {
    for (code, yaml, errors) in emitted() {
        for e in errors {
            assert_eq!(
                e.catalog_text(EN).as_deref(),
                Some(e.message.as_str()),
                "{code}: the catalog's English is not the message for args {:?}\n{yaml}",
                e.args
            );
            assert_eq!(e.render_in(EN), e.to_string(), "{code}");
        }
    }
}

/// Whether `message` writes `value` as a value (quoted, in parentheses, after a colon or a dot path, after "port"),
/// not as a letter inside a word: the arg `a` is not in "a service".
fn mentions(message: &str, value: &str) -> bool {
    [
        format!("'{value}'"),
        format!("\"{value}\""),
        format!("({value})"),
        format!(".{value}."),
        format!(": {value}"),
        format!("port {value} "),
    ]
    .iter()
    .any(|form| message.contains(form))
}

/// pt-BR is not the English sentence, and it keeps every value of the sentence: each arg the English message
/// mentions is in the Portuguese one (a key path and a regex engine's own words are the author's, not ours).
#[test]
fn the_portuguese_catalog_differs_from_english_and_keeps_every_arg() {
    for (code, yaml, errors) in emitted() {
        for e in errors {
            let pt = e
                .catalog_text(PT)
                .unwrap_or_else(|| panic!("{code}: no pt-BR message"));
            assert_ne!(
                pt, e.message,
                "{code}: pt-BR is still the English sentence\n{yaml}"
            );
            for (name, value) in &e.args {
                let text = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => continue,
                };
                if mentions(&e.message, &text) {
                    assert!(
                        pt.contains(&text),
                        "{code}: pt-BR dropped {name}={text:?}: {pt}"
                    );
                }
            }
        }
    }
}

#[test]
fn an_error_without_a_catalog_entry_renders_its_english_message() {
    let mut e = produced("name: t\nservices:\n  a:\n    command: x\n    helth: y\n")
        .1
        .remove(0);
    e.code = "config.not_in_any_catalog";
    assert_eq!(e.text_in(PT), e.message);
    assert_eq!(e.render_in(PT), e.to_string());
}

/// What a program reads keeps its bytes: `Display`, `message`, the English list and the header, for every sample, equal
/// what origin/main produced (`testdata/config/problems-en.json`, captured from the validators before the codes).
#[test]
fn english_bytes_are_identical_to_the_validators_before_the_codes() {
    let golden: Value = serde_json::from_str(include_str!(
        "../../../../../testdata/config/problems-en.json"
    ))
    .unwrap();
    let golden = golden.as_object().unwrap();
    let samples = samples();
    assert_eq!(
        golden.len(),
        samples.len(),
        "a sample was added: capture its old bytes first"
    );
    for (i, (code, yaml)) in samples.iter().enumerate() {
        let want = &golden[&format!("{i:02} {code}")];
        let (all, errors) = produced(yaml);
        assert_eq!(
            render_errors_in(EN, &errors, "atlas.yaml"),
            want["errors"].as_str().unwrap(),
            "{code}\n{yaml}"
        );
        let display: Vec<String> = all.iter().map(ToString::to_string).collect();
        assert_eq!(
            Value::from(display),
            want["display"],
            "{code}: Display of the problems changed\n{yaml}"
        );
    }
}

#[test]
fn the_header_counts_in_each_language() {
    let (_, errors) =
        produced("name: t\nservices:\n  a:\n    command: x\n    helth: y\n    comand: z\n");
    assert_eq!(errors.len(), 2);
    let one = &errors[..1];
    assert!(render_errors_in(EN, one, "atlas.yaml").starts_with("1 config error:\n"));
    assert!(render_errors_in(EN, &errors, "atlas.yaml").starts_with("2 config errors:\n"));
    assert!(render_errors_in(PT, one, "atlas.yaml").starts_with("1 erro de configuração:\n"));
    assert!(render_errors_in(PT, &errors, "atlas.yaml").starts_with("2 erros de configuração:\n"));
    assert_eq!(
        render_errors_in(PT, &[], "atlas.yaml"),
        "Nenhum erro de configuração:"
    );
}

/// A file named on the error wins over the default, in either language.
#[test]
fn render_errors_names_the_file_each_problem_came_from() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: t\nservices:\n  a:\n    command: x\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("atlas.local.yaml"),
        "services:\n  a:\n    comand: y\n",
    )
    .unwrap();
    let (_, errors) = load_checked(dir.path()).unwrap();
    assert_eq!(
        render_errors_in(EN, &errors, "atlas.yaml"),
        "1 config error:\n  atlas.local.yaml: services.a.comand: unknown field (did you mean 'command'?)"
    );
    assert_eq!(
        render_errors_in(PT, &errors, "atlas.yaml"),
        "1 erro de configuração:\n  atlas.local.yaml: services.a.comand: campo desconhecido (você quis dizer 'command'?)"
    );
}

/// The loader's failure text is the catalog's in the language it is asked for, and the English one is the old one.
#[test]
fn the_loader_words_its_failure_in_the_requested_language() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "name: t\nservices:\n  api:\n    command: y\n    depends_on: [nope]\n",
    )
    .unwrap();
    let en = crate::config::load_or_detect_in(dir.path(), EN)
        .unwrap_err()
        .to_string();
    assert_eq!(
        en,
        "1 config error:\n  atlas.yaml: services.api.depends_on.nope: unknown service 'nope'"
    );
    let pt = crate::config::load_or_detect_in(dir.path(), PT)
        .unwrap_err()
        .to_string();
    assert_eq!(
        pt,
        "1 erro de configuração:\n  atlas.yaml: services.api.depends_on.nope: serviço 'nope' desconhecido"
    );
}
