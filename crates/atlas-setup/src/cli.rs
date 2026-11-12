//! Command line of Atlas-Setup.exe.
//!
//! ```text
//! Atlas-Setup.exe                      install (or update) Atlas, then open it
//! Atlas-Setup.exe --uninstall          remove Atlas; asks what to do with ~/.atlas
//! Atlas-Setup.exe --uninstall --keep-data | --remove-data     decide without asking
//! Atlas-Setup.exe --silent             no window, same steps (exit code 0 = done)
//! Atlas-Setup.exe --no-launch          do not open Atlas when the install is done
//! ```
//!
//! `/S` is accepted as `--silent`: it is what NSIS installers take, and what people type.

use std::ffi::OsString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Install,
    Uninstall,
}

/// What an uninstall does with the user's data (`~/.atlas`: settings, projects, sessions).
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

/// Parse `argv` without the program name. Unknown arguments are an error naming them, so a typo
/// never silently installs.
pub fn parse<I: IntoIterator<Item = OsString>>(argv: I) -> Result<Args, String> {
    let mut a = Args::default();
    let mut keep = false;
    let mut remove = false;
    for raw in argv {
        let s = raw.to_string_lossy().to_string();
        match s.to_ascii_lowercase().as_str() {
            "--uninstall" | "/uninstall" => a.mode = Mode::Uninstall,
            "--silent" | "/s" | "-s" => a.silent = true,
            "--no-launch" => a.launch = false,
            "--keep-data" => keep = true,
            "--remove-data" => remove = true,
            _ => return Err(format!("unknown argument: {s}")),
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
        // Incident guard: a script running `Atlas-Setup.exe --uninstall --silent` must never wipe ~/.atlas.
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
        assert_eq!(
            p(&["--unistall"]).unwrap_err(),
            "unknown argument: --unistall"
        );
    }
}
