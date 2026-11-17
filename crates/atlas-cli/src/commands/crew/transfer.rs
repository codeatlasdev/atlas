//! `atlas crew export|import` and the data guard of `uninstall` (issue #202): the scripts that run on
//! the server and the decisions around them, as pure functions so every case is tested without ssh.
//!
//! The archive itself is built and read by `atlas hostd --crew-export|--crew-import` on the server, as
//! the crew user, over the same files the daemon uses (see `atlas_host::crew::archive`). It is plain
//! tar.gz, so an archive moves between x86_64 and arm64 servers.

use anyhow::{Result, bail};

use super::layout::{INSTALL_DIR, WHICH_USER};
use crate::commands::deploy::remote::shq;

/// The error text an `atlas` without `--crew-export` prints (clap), to tell a server that needs the
/// new binary from any other failure.
const UNKNOWN_FLAG: &str = "unexpected argument";

/// Shell that streams the archive to stdout, as the crew user, in its data dir. Crewmate ids are
/// quoted: they come from the command line.
pub fn export_script(crewmates: &[String], with_secrets: bool) -> String {
    let mut s = format!(
        "u=$({WHICH_USER}); sudo -n -u \"$u\" {INSTALL_DIR}/atlas hostd --crew-export --dir \"/var/lib/$u/.atlas\""
    );
    for id in crewmates {
        s.push_str(&format!(" --crewmate {}", shq(id)));
    }
    if with_secrets {
        s.push_str(" --with-secrets");
    }
    s
}

/// Shell that reads an archive from stdin into the crew user's data dir.
pub fn import_script(rename: bool) -> String {
    let mut s = format!(
        "u=$({WHICH_USER}); sudo -n -u \"$u\" {INSTALL_DIR}/atlas hostd --crew-import --dir \"/var/lib/$u/.atlas\""
    );
    if rename {
        s.push_str(" --rename");
    }
    s
}

/// Shell that lists what an uninstall would delete: the crew home and what is in it. Prints
/// `home <path>`, `crewmates <n>`, `size <du -sh>` (nothing for a host without crew data).
pub const INVENTORY_SCRIPT: &str = "u=$(if id atlas-crew >/dev/null 2>&1; then echo atlas-crew; elif id atlas-bot >/dev/null 2>&1; then echo atlas-bot; else echo ''; fi); \
[ -n \"$u\" ] || exit 0; h=/var/lib/$u; echo \"home $h\"; \
n=$(sudo -n sh -c \"ls -d $h/.atlas/crew/*/ $h/.atlas/bots/*/ 2>/dev/null\" | wc -l | tr -d ' '); echo \"crewmates $n\"; \
echo \"size $(sudo -n du -sh $h 2>/dev/null | cut -f1)\"";

/// What [`INVENTORY_SCRIPT`] printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    pub home: Option<String>,
    pub crewmates: usize,
    pub size: Option<String>,
}

pub fn parse_inventory(out: &str) -> Inventory {
    let mut inv = Inventory::default();
    for line in out.lines() {
        match line.split_once(' ') {
            Some(("home", v)) => inv.home = Some(v.trim().to_string()),
            Some(("crewmates", v)) => inv.crewmates = v.trim().parse().unwrap_or(0),
            Some(("size", v)) if !v.trim().is_empty() => inv.size = Some(v.trim().to_string()),
            _ => {}
        }
    }
    inv
}

/// What uninstall tells the person before it deletes anything. Printed for `--yes` too: a script that
/// passes `--yes` still leaves a record of where the data was and that it is gone.
pub fn uninstall_notice(target: &str, inv: &Inventory) -> String {
    let home = inv.home.as_deref().unwrap_or("(no crew user found)");
    let mut s = format!("Uninstalling the crew server on {target} deletes:\n");
    s.push_str("  - the atlas-crew service and /usr/local/lib/atlas-crew\n");
    s.push_str(&format!("  - the crew user and its home {home}"));
    if inv.home.is_some() {
        s.push_str(&format!(
            " ({} crewmate(s){}): their definitions, memory, task history, secrets and checkouts\n",
            inv.crewmates,
            inv.size
                .as_deref()
                .map(|z| format!(", {z}"))
                .unwrap_or_default()
        ));
    } else {
        s.push('\n');
    }
    s.push_str("  - agent sign-ins stored in that home\n");
    s
}

/// What `--yes` prints after the delete: where the data was, and that it cannot be recovered.
pub fn gone_notice(target: &str, inv: &Inventory) -> String {
    match &inv.home {
        Some(h) => format!(
            "Deleted {h} on {target} ({} crewmate(s)). It is gone; to keep a copy, run `atlas crew export {target}` before uninstalling.",
            inv.crewmates
        ),
        None => format!("No crew data was found on {target}."),
    }
}

/// The server name uninstall asks to be typed: the host part of the target (`ubuntu@box` → `box`).
pub fn server_name(target: &str) -> &str {
    super::install::host_from_target(target)
}

/// Whether `typed` confirms deleting `target`: exactly the server name, after trimming whitespace.
pub fn confirms_server_name(target: &str, typed: &str) -> bool {
    !typed.trim().is_empty() && typed.trim() == server_name(target)
}

/// Where an export goes when `-o` is not given: `atlas-crew-<host>-<yyyymmdd-hhmmss>.tar.gz`, with the
/// host reduced to characters safe in a file name.
pub fn default_export_name(target: &str, now: std::time::SystemTime) -> String {
    let host: String = server_name(target)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stamp = atlas_core::time::utc_compact(now);
    format!("atlas-crew-{host}-{}-{}.tar.gz", &stamp[..8], &stamp[8..])
}

/// A failed export/import on the server, in words. A server whose `atlas` predates the feature
/// answers with clap's unknown-argument error; the fix is to reinstall (which updates the binary).
pub fn explain_failure(target: &str, what: &str, code: i32, stderr: &str) -> String {
    let why = stderr.trim();
    if code == 255 {
        return format!("could not connect to {target} over SSH: {why}");
    }
    if why.contains(UNKNOWN_FLAG) {
        return format!(
            "the atlas on {target} is too old to {what}; run `atlas crew install {target}` to update it, then try again"
        );
    }
    if why.contains("a password is required") {
        return format!(
            "sudo on {target} asks for a password: grant the SSH user passwordless sudo to {what}"
        );
    }
    format!("could not {what} on {target}: {why}")
}

/// How an uninstall proceeds, decided from the flags and the terminal alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// `--yes`: print the notice, delete, print the gone notice. No questions.
    Unattended,
    /// A terminal and no `--yes`: offer the export, then require the server name.
    Ask,
    /// No `--yes` and nobody to ask (a pipe, cron): refuse; deleting needs a decision.
    Refuse,
}

pub fn flow(yes: bool, interactive: bool) -> Flow {
    match (yes, interactive) {
        (true, _) => Flow::Unattended,
        (false, true) => Flow::Ask,
        (false, false) => Flow::Refuse,
    }
}

/// A local file already holding data is never overwritten by an export: the caller picks another `-o`.
pub fn check_output_free(path: &std::path::Path) -> Result<()> {
    if path.exists() {
        bail!(
            "{} already exists; choose another file with -o (an export never overwrites)",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_runs_the_servers_atlas_as_the_crew_user_in_its_dir_and_streams_to_stdout() {
        let sh = export_script(&[], false);
        assert!(sh.contains(WHICH_USER));
        assert!(
            sh.contains("sudo -n -u \"$u\" /usr/local/lib/atlas-crew/atlas hostd --crew-export")
        );
        assert!(sh.contains("--dir \"/var/lib/$u/.atlas\""));
        // Secrets are opt-in and a selection is optional: neither is in the default script.
        assert!(!sh.contains("--with-secrets") && !sh.contains("--crewmate"));
        // A failure must reach the caller.
        assert!(!sh.contains("|| true"));
    }

    #[test]
    fn export_passes_secrets_and_the_selection_and_quotes_every_id() {
        let sh = export_script(&["abc".into(), "x'; rm -rf /; '".into()], true);
        assert!(sh.ends_with("--with-secrets"), "{sh}");
        assert!(sh.contains("--crewmate 'abc'"));
        // The hostile id stays one quoted word.
        assert!(sh.contains(r"--crewmate 'x'\''; rm -rf /; '\'''"), "{sh}");
    }

    #[test]
    fn import_reads_stdin_as_the_crew_user_and_rename_is_opt_in() {
        let plain = import_script(false);
        assert!(plain.contains("hostd --crew-import --dir \"/var/lib/$u/.atlas\""));
        assert!(!plain.contains("--rename"));
        assert!(import_script(true).ends_with("--rename"));
    }

    #[test]
    fn the_inventory_reads_what_the_script_printed() {
        let inv = parse_inventory("home /var/lib/atlas-crew\ncrewmates 3\nsize 41M\n");
        assert_eq!(
            inv,
            Inventory {
                home: Some("/var/lib/atlas-crew".into()),
                crewmates: 3,
                size: Some("41M".into())
            }
        );
        // A host without a crew user prints nothing.
        assert_eq!(parse_inventory(""), Inventory::default());
        assert_eq!(parse_inventory("crewmates x\nsize \n").crewmates, 0);
    }

    #[test]
    fn the_inventory_script_finds_either_generation_of_the_crew_user() {
        for needle in [
            "atlas-crew",
            "atlas-bot",
            ".atlas/crew/",
            ".atlas/bots/",
            "du -sh",
        ] {
            assert!(INVENTORY_SCRIPT.contains(needle), "{needle}");
        }
        // It only reads.
        for w in ["rm ", "userdel", "systemctl"] {
            assert!(!INVENTORY_SCRIPT.contains(w), "{w}");
        }
    }

    // Incident (real-systemd end-to-end test): since isolation the crew home is 0700 for the crew user, and the
    // inventory counted its crewmates as the admin user, so uninstall told the person "0 crewmate(s)" about a
    // home that held one. Only the size went through sudo.
    #[test]
    fn the_inventory_counts_crewmates_with_sudo_because_the_crew_home_is_private() {
        let count = INVENTORY_SCRIPT
            .split(';')
            .find(|l| l.contains(".atlas/crew/"))
            .expect("the count line");
        assert!(count.contains("sudo -n"), "{count}");
    }

    // Uninstall prints what it deletes before it deletes anything, and names the home and the count.
    #[test]
    fn the_uninstall_notice_names_the_home_the_crewmates_and_the_size() {
        let inv = Inventory {
            home: Some("/var/lib/atlas-crew".into()),
            crewmates: 2,
            size: Some("1.2G".into()),
        };
        let n = uninstall_notice("me@box", &inv);
        for needle in [
            "me@box",
            "/var/lib/atlas-crew",
            "2 crewmate(s)",
            "1.2G",
            "memory",
            "secrets",
        ] {
            assert!(n.contains(needle), "{needle}: {n}");
        }
    }

    #[test]
    fn the_notice_still_works_for_a_host_with_no_crew_user() {
        let n = uninstall_notice("me@box", &Inventory::default());
        assert!(n.contains("no crew user found"), "{n}");
        assert_eq!(
            gone_notice("me@box", &Inventory::default()),
            "No crew data was found on me@box."
        );
    }

    // `--yes` alone keeps deleting, but says where the data was and that it is gone.
    #[test]
    fn the_gone_notice_says_where_the_data_was_and_that_it_cannot_come_back() {
        let inv = Inventory {
            home: Some("/var/lib/atlas-bot".into()),
            crewmates: 1,
            size: None,
        };
        let n = gone_notice("me@box", &inv);
        assert!(
            n.contains("/var/lib/atlas-bot") && n.contains("is gone"),
            "{n}"
        );
        assert!(n.contains("atlas crew export me@box"));
    }

    // Without --yes the person types the server name; anything else is a refusal.
    #[test]
    fn only_the_exact_server_name_confirms_an_uninstall() {
        assert_eq!(server_name("ubuntu@box.example.com"), "box.example.com");
        assert!(confirms_server_name(
            "ubuntu@box.example.com",
            "box.example.com"
        ));
        assert!(confirms_server_name("box", "  box \n"));
        for no in [
            "",
            "  ",
            "y",
            "yes",
            "Y",
            "BOX",
            "ubuntu@box.example.com",
            "box.example",
            "box.example.com.evil",
        ] {
            assert!(
                !confirms_server_name("ubuntu@box.example.com", no),
                "{no:?}"
            );
        }
    }

    #[test]
    fn the_default_file_name_has_the_host_and_the_time_and_no_unsafe_characters() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_794_647_107); // 2026-11-14T09:05:07Z
        assert_eq!(
            default_export_name("me@box.example.com", t),
            "atlas-crew-box.example.com-20261114-090507.tar.gz"
        );
        assert_eq!(
            default_export_name("me@[::1]", t),
            "atlas-crew-___1_-20261114-090507.tar.gz"
        );
        assert!(!default_export_name("a/b", t).contains('/'));
    }

    #[test]
    fn failures_say_what_to_do() {
        let old = explain_failure(
            "me@box",
            "export",
            2,
            "error: unexpected argument '--crew-export' found",
        );
        assert!(
            old.contains("too old") && old.contains("atlas crew install me@box"),
            "{old}"
        );
        assert!(explain_failure("me@box", "export", 255, "refused").contains("over SSH"));
        assert!(
            explain_failure("me@box", "import", 1, "sudo: a password is required")
                .contains("passwordless sudo")
        );
        let other = explain_failure(
            "me@box",
            "import",
            1,
            "crewmate id(s) already on this server: abc",
        );
        assert!(
            other.contains("could not import on me@box") && other.contains("abc"),
            "{other}"
        );
    }

    // The old prompt read `y` from a closed stdin as "no"; now no terminal and no --yes is a refusal that
    // says what to pass, and `--yes` never asks.
    #[test]
    fn uninstall_without_yes_and_without_a_terminal_is_refused() {
        assert_eq!(flow(false, false), Flow::Refuse);
        assert_eq!(flow(false, true), Flow::Ask);
        assert_eq!(flow(true, true), Flow::Unattended);
        assert_eq!(flow(true, false), Flow::Unattended);
    }

    #[test]
    fn an_export_never_overwrites_a_file() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.tar.gz");
        assert!(check_output_free(&p).is_ok());
        std::fs::write(&p, "x").unwrap();
        assert!(
            check_output_free(&p)
                .unwrap_err()
                .to_string()
                .contains("never overwrites")
        );
    }
}
