//! `atlas install`, `atlas install --upgrade` and `atlas uninstall` speak the display language, against the real
//! binary in a throwaway HOME. The expected text is always the catalog's own value for the language
//! (`t_in!`), never a typed sentence, and what is not text (paths, versions, the order of the lines) is the same
//! in every language.

#![cfg(unix)]

use atlas_i18n::t_in;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

/// `atlas <args>` with `lang` as the display language, HOME at `home`, and nothing of the real machine's
/// shell or sudo environment.
fn atlas(home: &Path, lang: &str, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("ATLAS_LANG", lang)
        .env("SHELL", "/bin/sh")
        .env("PATH", "/usr/bin:/bin")
        .env_remove("SUDO_USER")
        .env_remove("SUDO_UID")
        .env_remove("ZDOTDIR")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("LANGUAGE")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .stdin(Stdio::null())
        .output()
        .expect("spawn atlas")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `~/.atlas` holding an installed CLI, a host socket, and the person's own worktrees.
fn installed_home() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    let atlas = t.path().join(".atlas");
    std::fs::create_dir_all(atlas.join("bin")).unwrap();
    std::fs::write(atlas.join("bin/atlas"), "cli").unwrap();
    std::fs::write(atlas.join("hostd.sock"), "").unwrap();
    std::fs::create_dir_all(atlas.join("worktrees")).unwrap();
    t
}

/// The list `atlas uninstall --list` prints names what goes and what stays in the language, with the same paths
/// in every language, and changes nothing.
#[test]
fn uninstall_list_is_in_the_display_language_with_the_same_paths() {
    let home = installed_home();
    let dot_atlas = home.path().join(".atlas");
    let mut seen = Vec::new();
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &["uninstall", "--list"]);
        assert!(out.status.success(), "{tag}: {}", text(&out.stderr));
        let stdout = text(&out.stdout);
        for want in [
            t_in!(tag, "cmd_install_uninstall_title"),
            t_in!(tag, "cmd_install_plan_remove"),
            t_in!(tag, "cmd_install_plan_keep"),
            t_in!(tag, "cmd_install_note_cli"),
            t_in!(tag, "cmd_install_note_host_socket"),
            dot_atlas.join("worktrees").display().to_string(),
        ] {
            assert!(
                stdout.contains(&want),
                "{tag}: {want:?} missing in:\n{stdout}"
            );
        }
        seen.push(stdout);
    }
    assert_ne!(seen[0], seen[1], "pt-BR is not English");
    assert!(
        !seen[1].contains(&t_in!("en", "cmd_install_plan_remove")),
        "pt-BR output still has the English heading:\n{}",
        seen[1]
    );
    assert!(
        dot_atlas.join("bin/atlas").exists(),
        "--list removes nothing"
    );
}

/// With no terminal and no `--yes` nothing is removed and the error says why and what to do, in the language,
/// after the translated `error:` prefix.
#[test]
fn uninstall_without_a_terminal_refuses_in_the_display_language() {
    let home = installed_home();
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &["uninstall"]);
        assert_eq!(out.status.code(), Some(1), "{tag}");
        let stderr = text(&out.stderr);
        assert!(
            stderr.contains(&t_in!(tag, "cmd_install_uninstall_needs_confirmation")),
            "{tag}: {stderr}"
        );
        assert!(
            stderr.contains(&t_in!(tag, "cli_chrome_error")),
            "{tag}: {stderr}"
        );
        assert!(
            home.path().join(".atlas/bin/atlas").exists(),
            "{tag}: nothing removed"
        );
    }
}

/// `atlas install --upgrade` prints one line, and with nothing installed it names the folder and the command.
#[test]
fn install_upgrade_says_what_it_found_in_the_display_language() {
    let home = tempfile::tempdir().unwrap();
    let dest = home.path().join(".atlas/bin/atlas").display().to_string();
    for tag in ["en", "pt-BR"] {
        let out = atlas(home.path(), tag, &["install", "--upgrade"]);
        assert!(out.status.success(), "{tag}: {}", text(&out.stderr));
        assert_eq!(
            text(&out.stdout),
            format!(
                "{}\n",
                t_in!(tag, "cmd_install_upgrade_absent", path = dest.clone())
            ),
            "{tag}"
        );
    }
}

/// A whole install into a throwaway HOME: the binary lands where the report says, the report is in the language,
/// and the lines a person acts on (the PATH line to run, the commands to start with) are the same in every language.
#[test]
fn install_reports_in_the_display_language_and_installs_the_same_files() {
    for tag in ["en", "pt-BR"] {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".atlas/bin");
        let out = atlas(
            home.path(),
            tag,
            &["install", "--dir", bin.to_str().unwrap()],
        );
        assert!(out.status.success(), "{tag}: {}", text(&out.stderr));
        let stdout = text(&out.stdout);
        let dest = bin.join("atlas");
        assert!(dest.is_file(), "{tag}: the binary is installed");
        for want in [
            t_in!(tag, "cmd_install_title"),
            t_in!(tag, "cmd_install_target", dir = bin.display().to_string()),
            t_in!(tag, "cmd_install_copied", path = dest.display().to_string()),
            t_in!(tag, "cmd_install_get_started"),
            t_in!(tag, "cmd_install_hint_dev"),
        ] {
            assert!(
                stdout.contains(&want),
                "{tag}: {want:?} missing in:\n{stdout}"
            );
        }
        // The command names of the Get started list are code, not text.
        for command in ["atlas dev", "atlas update", "atlas setup"] {
            assert!(stdout.contains(command), "{tag}: {command}");
        }
    }
}
