//! Command line of Atlas-Setup.exe.
//!
//! ```text
//! Atlas-Setup.exe                      install (or update) Atlas, then open it
//! Atlas-Setup.exe --uninstall          remove Atlas; asks what to do with %USERPROFILE%\.atlas
//! Atlas-Setup.exe --uninstall --keep-data | --remove-data     decide without asking
//! Atlas-Setup.exe --silent             no window, same steps (exit code 0 = done)
//! Atlas-Setup.exe --no-launch          do not open Atlas when the install is done
//! ```
//!
//! `/S` is accepted as `--silent`: it is what NSIS installers take, and what people type.
//!
//! The plain NSIS installer, when it upgrades over an install made by Atlas-Setup.exe, runs this
//! program as the registered uninstaller with NSIS's own flags (`"Atlas Uninstall.exe" --uninstall
//! /UPDATE /P _?=<dir>`, tauri-bundler's `installer.nsi`) and waits for it. `/UPDATE`, `/P` and
//! `_?=<dir>` are accepted and make it a silent uninstall that keeps the data: an upgrade must
//! never delete the user's `%USERPROFILE%\.atlas`.

use std::ffi::OsString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Install,
    Uninstall,
}

/// What an uninstall does with the user's data (`%USERPROFILE%\.atlas`: settings, project list,
/// sessions, worktrees Atlas created, the `atlas` command).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataChoice {
    /// Ask in the window; the default button is **keep**.
    Ask,
    Keep,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub mode: Mode,
    pub silent: bool,
    pub launch: bool,
    pub data: DataChoice,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            mode: Mode::Install,
            silent: false,
            launch: true,
            data: DataChoice::Ask,
        }
    }
}

/// Shown by the message box when the command line is wrong.
pub const USAGE: &str = "Use: Atlas-Setup.exe [--silent] [--no-launch]\n   or: Atlas-Setup.exe --uninstall [--keep-data | --remove-data] [--silent]";

/// Parse `argv` without the program name. Unknown arguments are an error naming them, so a typo
/// never silently installs.
pub fn parse<I: IntoIterator<Item = OsString>>(argv: I) -> Result<Args, String> {
    let mut a = Args::default();
    let mut keep = false;
    let mut remove = false;
    for raw in argv {
        let s = raw.to_string_lossy().to_string();
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "--uninstall" | "/uninstall" => a.mode = Mode::Uninstall,
            "--silent" | "/s" | "-s" => a.silent = true,
            "--no-launch" => a.launch = false,
            "--keep-data" => keep = true,
            "--remove-data" => remove = true,
            // NSIS's upgrade-time uninstall: nobody to ask, and the data stays.
            "/update" | "/p" => {
                a.silent = true;
                keep = true;
            }
            l if l.starts_with("_?=") => {}
            _ => return Err(format!("Unknown argument: {s}\n\n{USAGE}")),
        }
    }
    if keep && remove {
        return Err("--keep-data and --remove-data exclude each other".into());
    }
    if (keep || remove) && a.mode != Mode::Uninstall {
        return Err("--keep-data and --remove-data only apply to --uninstall".into());
    }
    a.data = match (keep, remove) {
        (true, _) => DataChoice::Keep,
        (_, true) => DataChoice::Remove,
        // Nobody to ask in a silent run: keeping the user's data is the safe answer.
        _ if a.silent => DataChoice::Keep,
        _ => DataChoice::Ask,
    };
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Args, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_installs_and_launches() {
        let a = p(&[]).unwrap();
        assert_eq!(a.mode, Mode::Install);
        assert!(a.launch && !a.silent);
    }

    #[test]
    fn nsis_style_silent_flag_is_accepted_in_any_case() {
        assert!(p(&["/S"]).unwrap().silent);
        assert!(p(&["/s"]).unwrap().silent);
        assert!(p(&["--SILENT"]).unwrap().silent);
    }

    #[test]
    fn silent_uninstall_keeps_the_data_unless_told_otherwise() {
        // Incident guard: a script running `Atlas-Setup.exe --uninstall --silent` must never wipe %USERPROFILE%\.atlas.
        assert_eq!(
            p(&["--uninstall", "--silent"]).unwrap().data,
            DataChoice::Keep
        );
        assert_eq!(
            p(&["--uninstall", "--silent", "--remove-data"])
                .unwrap()
                .data,
            DataChoice::Remove
        );
        assert_eq!(p(&["--uninstall"]).unwrap().data, DataChoice::Ask);
    }

    #[test]
    fn contradictory_or_misplaced_data_flags_are_refused() {
        assert!(p(&["--uninstall", "--keep-data", "--remove-data"]).is_err());
        assert!(
            p(&["--remove-data"])
                .unwrap_err()
                .contains("only apply to --uninstall")
        );
    }

    #[test]
    fn a_typo_is_an_error_not_an_install() {
        let e = p(&["--unistall"]).unwrap_err();
        assert!(e.starts_with("Unknown argument: --unistall"), "{e}");
        assert!(e.contains(USAGE), "the message shows how to call it: {e}");
    }

    /// What tauri's installer.nsi runs over an install made by Atlas-Setup.exe (the registered
    /// UninstallString plus ` /UPDATE /P _?=<dir>`), and waits for.
    #[test]
    fn the_nsis_upgrade_uninstall_is_a_silent_keep_data_uninstall() {
        let a = p(&[
            "--uninstall",
            "/UPDATE",
            "/P",
            r"_?=C:\Users\ana\AppData\Local\Atlas",
        ])
        .unwrap();
        assert_eq!(a.mode, Mode::Uninstall);
        assert!(a.silent);
        assert_eq!(a.data, DataChoice::Keep);
        // Either flag alone is enough, in any case, and never asks.
        for flags in [&["/p"][..], &["/update"][..]] {
            let mut argv = vec!["--uninstall"];
            argv.extend(flags);
            let a = p(&argv).unwrap();
            assert!(a.silent && a.data == DataChoice::Keep, "{flags:?}");
        }
    }

    #[test]
    fn an_upgrade_uninstall_never_removes_data_even_when_told_to() {
        // The flags exclude each other with --remove-data: an upgrade is not a place to delete data.
        assert!(p(&["--uninstall", "/UPDATE", "--remove-data"]).is_err());
    }
}
