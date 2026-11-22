//! Translated `--help` for the commands that have been migrated, without leaving clap's derive.
//!
//! clap has no i18n (clap#380), so the English doc comments stay the source and the tree is
//! rewritten after it is built: for a command inside [`MIGRATED`], `about`, `long_about` and each
//! argument's `help` come from the catalog (`i18n/messages/cli_<command>/`) by command path
//! (`atlas config lang` -> `cli_config_lang_about`, `cli_config_lang_arg_tag`), and the words clap
//! prints itself (`Usage:`, `Commands:`, `Options:`, the help flag) from `cli_chrome`.
//! Anything the active language lacks keeps its English text, so a half-translated tree is safe.
//!
//! English is never rewritten: `en` runs the stock clap tree. `i18n/messages/cli_<command>/en.json`
//! and `meta.json` are a golden generated from the tree by [`golden`]; `UPDATE_FIXTURES=1 cargo
//! test -p atlas-cli cli_i18n` rewrites them, and an agent that adds
//! a flag with a doc comment inside a migrated command fails the test until it does, then adds the
//! pt-BR line.
//!
//! Never translated: subcommand names, flag names, value names, env var names (they are API).
//! Not translated yet: clap's own parse errors and the `[default: ..]`/`[possible values: ..]`
//! suffixes, which clap does not let a program override.

use clap::{Arg, Command};

/// Command paths whose whole subtree has a catalog. Grow it one command per PR, with the golden.
pub const MIGRATED: &[&[&str]] = &[&["config"]];

fn migrated(path: &[String]) -> bool {
    MIGRATED
        .iter()
        .any(|m| path.len() >= m.len() && m.iter().zip(path).all(|(a, b)| a == b))
}

/// A command or argument name as a catalog key part: `-` is not valid in an identifier.
fn id_part(s: &str) -> String {
    s.replace('-', "_")
}

fn path_part(path: &[String]) -> String {
    path.iter()
        .map(|p| id_part(p))
        .collect::<Vec<_>>()
        .join("_")
}

/// `cli_<path>_<what>`: `atlas config lang` and `about` give `cli_config_lang_about`.
fn cmd_id(path: &[String], what: &str) -> String {
    format!("cli_{}_{what}", path_part(path))
}

fn arg_id(path: &[String], arg: &str) -> String {
    format!("cli_{}_arg_{}", path_part(path), id_part(arg))
}

/// `cmd` with the help of every migrated command in `lang`. A no-op for English.
pub fn localize(mut cmd: Command, lang: &str) -> Command {
    if lang == atlas_core::lang::DEFAULT {
        return cmd;
    }
    // Build first so clap's own `help` flag and subcommand exist and can be reworded.
    cmd.build();
    walk(cmd, &mut Vec::new(), lang)
}

fn walk(cmd: Command, path: &mut Vec<String>, lang: &str) -> Command {
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_owned())
        .filter(|n| n != "help")
        .collect();
    let mut cmd = cmd;
    for name in names {
        path.push(name.clone());
        cmd = cmd.mut_subcommand(&name, |sub| walk(sub, path, lang));
        path.pop();
    }
    if !path.is_empty() && migrated(path) {
        cmd = translate_node(cmd, path, lang);
    }
    cmd
}

fn chrome(lang: &str, id: &str) -> Option<String> {
    atlas_i18n::lookup(lang, id, None)
}

fn translate_node(mut cmd: Command, path: &[String], lang: &str) -> Command {
    if let Some(about) = atlas_i18n::lookup(lang, &cmd_id(path, "about"), None) {
        cmd = cmd.about(about);
    }
    if let Some(long) = atlas_i18n::lookup(lang, &cmd_id(path, "long_about"), None) {
        cmd = cmd.long_about(long);
    }
    let arguments = chrome(lang, "cli_chrome_arguments");
    let options = chrome(lang, "cli_chrome_options");
    let ids: Vec<String> = cmd
        .get_arguments()
        .map(|a| a.get_id().as_str().to_owned())
        .collect();
    for id in ids {
        let positional = cmd
            .get_arguments()
            .any(|a| a.get_id() == id.as_str() && a.is_positional());
        let (help, heading) = match id.as_str() {
            "help" => (chrome(lang, "cli_chrome_help_flag"), options.clone()),
            _ => (
                atlas_i18n::lookup(lang, &arg_id(path, &id), None),
                if positional {
                    arguments.clone()
                } else {
                    options.clone()
                },
            ),
        };
        cmd = cmd.mut_arg(id.as_str(), |a: Arg| {
            let a = match help {
                Some(h) => a.help(h),
                None => a,
            };
            match heading {
                Some(h) => a.help_heading(h),
                None => a,
            }
        });
    }
    if cmd.has_subcommands() {
        if let Some(h) = chrome(lang, "cli_chrome_commands") {
            cmd = cmd.subcommand_help_heading(h);
        }
        let about = chrome(lang, "cli_chrome_help_subcommand");
        let arg = chrome(lang, "cli_chrome_help_subcommand_arg");
        cmd = cmd.mut_subcommand("help", |h| {
            let h = match about {
                Some(a) => h.about(a),
                None => h,
            };
            match arg {
                // Absent when clap expanded the help tree instead of giving `help` an argument.
                Some(text) if h.get_arguments().any(|a| a.get_id() == "subcommand") => {
                    h.mut_arg("subcommand", |a| a.help(text))
                }
                Some(_) => h,
                None => h,
            }
        });
    }
    if let Some(usage) = chrome(lang, "cli_chrome_usage") {
        // clap's default template with its `{usage-heading}` replaced by the translated word.
        cmd = cmd.help_template(format!(
            "{{before-help}}{{about-with-newline}}\n{usage} {{usage}}\n\n{{all-args}}{{after-help}}"
        ));
    }
    cmd
}

/// A generated catalog area: `cli_<command>` holds the help of `atlas <command>` and everything below.
#[cfg(test)]
pub struct Area {
    pub name: String,
    /// key -> English text (`en.json`).
    pub en: std::collections::BTreeMap<String, String>,
    /// key -> description (`meta.json`).
    pub meta: std::collections::BTreeMap<String, String>,
}

/// The English catalog areas for the migrated commands of `cmd` (the stock tree, not built): one
/// message per command (`about`, `long_about` when there is one) and per argument that has help
/// text, each with the description the catalog check requires.
#[cfg(test)]
pub fn golden(cmd: &Command) -> Vec<Area> {
    let mut areas = std::collections::BTreeMap::new();
    dump(cmd, &mut Vec::new(), &mut areas);
    areas.into_values().collect()
}

#[cfg(test)]
fn dump(
    cmd: &Command,
    path: &mut Vec<String>,
    areas: &mut std::collections::BTreeMap<String, Area>,
) {
    if !path.is_empty() && migrated(path) {
        let name = format!("cli_{}", id_part(&path[0]));
        let area = areas.entry(name.clone()).or_insert_with(|| Area {
            name,
            en: Default::default(),
            meta: Default::default(),
        });
        let shown = format!("atlas {}", path.join(" "));
        let mut add = |id: String, description: String, text: &str| {
            assert!(
                !text.contains(['{', '}']),
                "{id}: help text with a brace is not supported by the catalog format; reword the doc comment"
            );
            area.en.insert(id.clone(), text.to_owned());
            area.meta.insert(id, description);
        };
        if let Some(about) = cmd.get_about() {
            add(
                cmd_id(path, "about"),
                format!("Short description of `{shown}` in `--help`."),
                &about.to_string(),
            );
        }
        if let Some(long) = cmd.get_long_about() {
            add(
                cmd_id(path, "long_about"),
                format!("Long description of `{shown}` in `--help`."),
                &long.to_string(),
            );
        }
        for arg in cmd.get_arguments() {
            let id = arg.get_id().as_str();
            if matches!(
                arg.get_action(),
                clap::ArgAction::Help | clap::ArgAction::Version
            ) {
                continue;
            }
            if let Some(help) = arg.get_help() {
                add(
                    arg_id(path, id),
                    format!("Help line of the `{id}` argument of `{shown}`."),
                    &help.to_string(),
                );
            }
        }
    }
    for sub in cmd.get_subcommands() {
        path.push(sub.get_name().to_owned());
        dump(sub, path, areas);
        path.pop();
    }
}

/// A catalog file: one key per line, sorted, tab indented, a trailing newline (what `i18n:fix` writes).
#[cfg(test)]
fn json(entries: &std::collections::BTreeMap<String, String>) -> String {
    let lines: Vec<String> = entries
        .iter()
        .map(|(k, v)| {
            format!(
                "\t{}: {}",
                serde_json::to_string(k).unwrap(),
                serde_json::to_string(v).unwrap()
            )
        })
        .collect();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn tree() -> Command {
        crate::Cli::command()
    }

    fn render(cmd: &Command, path: &[&str]) -> String {
        let mut c = cmd.clone();
        // Build the whole tree first: a subcommand built alone would not know its parent's name.
        c.build();
        let mut sub = &mut c;
        for p in path {
            sub = sub.find_subcommand_mut(p).expect("subcommand");
        }
        sub.render_long_help().to_string()
    }

    fn messages_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n/messages")
    }

    // The area declaration of a generated area: consumed by Rust, English regenerated by this test.
    const AREA_JSON: &str = "{\n\t\"generated\": \"UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n\",\n\t\"surface\": \"rust\"\n}\n";

    // The golden is the single source the English catalog is checked against: a doc comment
    // edited or added in a migrated command, without regenerating, fails here. It owns the
    // `i18n/messages/cli_<command>/{en,meta,area}.json` of every migrated command.
    #[test]
    fn the_english_catalog_is_the_dump_of_the_clap_tree() {
        let update = std::env::var_os("UPDATE_FIXTURES").is_some();
        let mut stale = Vec::new();
        for area in golden(&tree()) {
            for (file, want) in [
                ("en.json", json(&area.en)),
                ("meta.json", json(&area.meta)),
                ("area.json", AREA_JSON.to_string()),
            ] {
                let path = messages_dir().join(&area.name).join(file);
                if update {
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(&path, &want).unwrap();
                }
                if std::fs::read_to_string(&path).ok().as_deref() != Some(want.as_str()) {
                    stale.push(path.display().to_string());
                }
            }
        }
        assert!(
            stale.is_empty(),
            "stale or missing, re-run with UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n: {stale:?}"
        );
    }

    // Every golden message reaches the runtime catalog (embedded JSON) as the exact clap text.
    #[test]
    fn golden_messages_round_trip_to_the_clap_text() {
        let config = tree();
        let sub = config.find_subcommand("config").unwrap();
        let lang = sub.find_subcommand("lang").unwrap();
        let about = lang.get_about().unwrap().to_string();
        assert_eq!(
            atlas_i18n::lookup("en", "cli_config_lang_about", None).as_deref(),
            Some(about.as_str())
        );
        let tag = lang.get_arguments().find(|a| a.get_id() == "tag").unwrap();
        assert_eq!(
            atlas_i18n::lookup("en", "cli_config_lang_arg_tag", None).as_deref(),
            Some(tag.get_help().unwrap().to_string().as_str())
        );
    }

    #[test]
    fn english_help_is_stock_clap() {
        let stock = render(&tree(), &["config", "lang"]);
        let localized = render(&localize(tree(), "en"), &["config", "lang"]);
        assert_eq!(stock, localized);
        assert!(stock.contains("Usage:") && stock.contains("Options:"));
    }

    #[test]
    fn portuguese_help_is_translated_down_to_the_words_clap_prints() {
        let help = render(&localize(tree(), "pt-BR"), &["config", "lang"]);
        for must in [
            "Uso:",
            "Opções:",
            "Argumentos:",
            "Mostra a ajuda",
            "Idioma a salvar",
        ] {
            assert!(help.contains(must), "{must:?} missing from:\n{help}");
        }
        // Names are API and never translated.
        for kept in ["--unset", "--json", "[TAG]", "atlas config lang"] {
            assert!(help.contains(kept), "{kept:?} must stay as is:\n{help}");
        }
        for english in ["Usage:", "Options:", "Arguments:", "Print help"] {
            assert!(!help.contains(english), "{english:?} left in:\n{help}");
        }
    }

    #[test]
    fn portuguese_parent_help_translates_the_commands_heading_and_the_help_row() {
        let help = render(&localize(tree(), "pt-BR"), &["config"]);
        for must in [
            "Comandos:",
            "Mostra esta mensagem ou a ajuda do subcomando informado",
        ] {
            assert!(help.contains(must), "{must:?} missing from:\n{help}");
        }
        assert!(!help.contains("Commands:"), "{help}");
    }

    // The swap must not change what parses: same subcommand and flag names in every language.
    #[test]
    fn a_translated_tree_parses_the_same_command_line() {
        for lang in ["en", "pt-BR"] {
            let m = localize(tree(), lang)
                .try_get_matches_from(["atlas", "config", "lang", "pt-BR", "--unset"])
                .map(|_| ())
                .expect_err("--unset conflicts with a tag in every language");
            assert_eq!(m.kind(), clap::error::ErrorKind::ArgumentConflict, "{lang}");
        }
    }

    // A command that is not migrated keeps its English help in pt-BR: partial catalogs are safe.
    #[test]
    fn commands_outside_the_migrated_set_stay_english() {
        let stock = render(&tree(), &["down"]);
        let localized = render(&localize(tree(), "pt-BR"), &["down"]);
        assert_eq!(stock, localized);
    }
}
