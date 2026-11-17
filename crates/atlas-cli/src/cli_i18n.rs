//! Translated `--help` for the commands that have been migrated, without leaving clap's derive.
//!
//! clap has no i18n (clap#380), so the English doc comments stay the source and the tree is
//! rewritten after it is built: for a command inside [`MIGRATED`], `about`, `long_about` and each
//! argument's `help` come from `crates/atlas-i18n/locales/<lang>/cli.ftl` by command path
//! (`atlas config lang` -> `cli-config-lang-about`, `cli-config-lang-arg-tag`), and the words clap
//! prints itself (`Usage:`, `Commands:`, `Options:`, the help flag) from `cli-chrome.ftl`.
//! Anything the active language lacks keeps its English text, so a half-translated tree is safe.
//!
//! English is never rewritten: `en` runs the stock clap tree. `locales/en/cli.ftl` is a golden
//! file generated from the tree by [`golden`]; `UPDATE_FIXTURES=1 cargo test -p atlas-cli
//! cli_i18n` rewrites it, and an agent that adds a flag with a doc comment inside a migrated
//! command fails the test until it does, then adds the pt-BR line.
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

fn cmd_id(path: &[String], what: &str) -> String {
    format!("cli-{}-{what}", path.join("-"))
}

fn arg_id(path: &[String], arg: &str) -> String {
    format!("cli-{}-arg-{arg}", path.join("-"))
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
    if let Some(long) = atlas_i18n::lookup(lang, &cmd_id(path, "long-about"), None) {
        cmd = cmd.long_about(long);
    }
    let arguments = chrome(lang, "cli-chrome-arguments");
    let options = chrome(lang, "cli-chrome-options");
    let ids: Vec<String> = cmd
        .get_arguments()
        .map(|a| a.get_id().as_str().to_owned())
        .collect();
    for id in ids {
        let positional = cmd
            .get_arguments()
            .any(|a| a.get_id() == id.as_str() && a.is_positional());
        let (help, heading) = match id.as_str() {
            "help" => (chrome(lang, "cli-chrome-help-flag"), options.clone()),
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
        if let Some(h) = chrome(lang, "cli-chrome-commands") {
            cmd = cmd.subcommand_help_heading(h);
        }
        let about = chrome(lang, "cli-chrome-help-subcommand");
        let arg = chrome(lang, "cli-chrome-help-subcommand-arg");
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
    if let Some(usage) = chrome(lang, "cli-chrome-usage") {
        // clap's default template with its `{usage-heading}` replaced by the translated word.
        cmd = cmd.help_template(format!(
            "{{before-help}}{{about-with-newline}}\n{usage} {{usage}}\n\n{{all-args}}{{after-help}}"
        ));
    }
    cmd
}

/// The `locales/en/cli.ftl` content for the migrated commands of `cmd` (the stock tree, not
/// built). One message per command (`about`, `long-about` when there is one) and per argument
/// that has help text, each with the description comment the catalog check requires.
#[cfg(test)]
pub fn golden(cmd: &Command) -> String {
    let mut out = String::from(
        "# Generated from the clap tree by `UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n`.\n\
         # Do not edit by hand: change the doc comment in the Rust source and regenerate.\n\n",
    );
    dump(cmd, &mut Vec::new(), &mut out);
    out
}

#[cfg(test)]
fn dump(cmd: &Command, path: &mut Vec<String>, out: &mut String) {
    if !path.is_empty() && migrated(path) {
        let shown = format!("atlas {}", path.join(" "));
        if let Some(about) = cmd.get_about() {
            message(
                out,
                &format!("Short description of `{shown}` in `--help`."),
                &cmd_id(path, "about"),
                &about.to_string(),
            );
        }
        if let Some(long) = cmd.get_long_about() {
            message(
                out,
                &format!("Long description of `{shown}` in `--help`."),
                &cmd_id(path, "long-about"),
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
                message(
                    out,
                    &format!("Help line of the `{id}` argument of `{shown}`."),
                    &arg_id(path, id),
                    &help.to_string(),
                );
            }
        }
    }
    for sub in cmd.get_subcommands() {
        path.push(sub.get_name().to_owned());
        dump(sub, path, out);
        path.pop();
    }
}

/// One Fluent message. `{` and `}` in help text are escaped as string literals; a text with
/// several lines becomes a block value.
#[cfg(test)]
fn message(out: &mut String, description: &str, id: &str, text: &str) {
    let escaped: String = text
        .chars()
        .map(|c| match c {
            '{' => "{ \"{\" }".to_owned(),
            '}' => "{ \"}\" }".to_owned(),
            c => c.to_string(),
        })
        .collect();
    out.push_str(&format!("# {description}\n{id} ="));
    if escaped.contains('\n') {
        out.push('\n');
        for line in escaped.lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    } else {
        out.push(' ');
        out.push_str(&escaped);
        out.push('\n');
    }
    out.push('\n');
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

    // The golden is the single source the English catalog is checked against: a doc comment
    // edited or added in a migrated command, without regenerating, fails here.
    #[test]
    fn the_english_catalog_is_the_dump_of_the_clap_tree() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../atlas-i18n/locales/en/cli.ftl"
        );
        let want = golden(&tree());
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(path, &want).unwrap();
        }
        let have = std::fs::read_to_string(path)
            .expect("locales/en/cli.ftl missing: run with UPDATE_FIXTURES=1 to create it");
        assert_eq!(
            have, want,
            "stale locales/en/cli.ftl: re-run with UPDATE_FIXTURES=1"
        );
    }

    // Every golden message round-trips through the Fluent parser to the exact clap text, so the
    // escaping and block layout cannot drift from what pt-BR sees as the source.
    #[test]
    fn golden_messages_round_trip_to_the_clap_text() {
        let config = tree();
        let sub = config.find_subcommand("config").unwrap();
        let lang = sub.find_subcommand("lang").unwrap();
        let about = lang.get_about().unwrap().to_string();
        assert_eq!(
            atlas_i18n::lookup("en", "cli-config-lang-about", None).as_deref(),
            Some(about.as_str())
        );
        let tag = lang.get_arguments().find(|a| a.get_id() == "tag").unwrap();
        assert_eq!(
            atlas_i18n::lookup("en", "cli-config-lang-arg-tag", None).as_deref(),
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
