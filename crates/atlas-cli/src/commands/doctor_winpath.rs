//! Windows half of the `atlas doctor` "new terminal" check (#340). A new terminal there takes its
//! `PATH` from the registry, not from a shell rc: the system `Path` (`HKLM\SYSTEM\CurrentControlSet\
//! Control\Session Manager\Environment`) followed by the user's (`HKCU\Environment`), each a
//! `REG_SZ` or `REG_EXPAND_SZ` that may carry `%USERPROFILE%`-style references. This module reads
//! both with `reg.exe`, expands them and resolves `atlas.exe` the way the terminal will. The
//! parsing and resolving are pure strings, so they are tested on every OS with real value shapes.

/// `reg query` key of the system environment.
const MACHINE_KEY: &str = r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
/// `reg query` key of the user environment.
const USER_KEY: &str = r"HKCU\Environment";

/// The value of `name` in `reg query <key> /v <name>` output: lines such as
/// `    Path    REG_EXPAND_SZ    %USERPROFILE%\.atlas\bin;C:\Windows`. `None` when the value is not
/// there or is not a string type.
pub(crate) fn parse_reg_query(out: &str, name: &str) -> Option<String> {
    out.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix_ci(name)?;
        // The name is followed by whitespace, never more name (`PathExt` is another value).
        let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
        let (kind, data) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        matches!(kind, "REG_SZ" | "REG_EXPAND_SZ").then(|| data.trim().to_owned())
    })
}

/// `str::strip_prefix` that ignores ASCII case (registry value names do).
trait StripPrefixCi {
    fn strip_prefix_ci(&self, prefix: &str) -> Option<&str>;
}

impl StripPrefixCi for str {
    fn strip_prefix_ci(&self, prefix: &str) -> Option<&str> {
        let head = self.get(..prefix.len())?;
        head.eq_ignore_ascii_case(prefix)
            .then(|| &self[prefix.len()..])
    }
}

/// `ExpandEnvironmentStrings` for a registry value: every `%NAME%` whose name `lookup` knows is
/// replaced (names are case-insensitive, `lookup` gets them as written); an unknown one, a lone
/// `%` and `%%` stay as they are, like Windows leaves them.
pub(crate) fn expand_env(value: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('%') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('%') {
            Some(close) if close > 0 => match lookup(&after[..close]) {
                Some(v) => {
                    out.push_str(&v);
                    rest = &after[close + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            },
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The directories of a Windows `Path` value: split on `;`, entries unquoted, empty ones dropped.
pub(crate) fn split_win_path(path: &str) -> Vec<String> {
    path.split(';')
        .map(|d| d.trim().trim_matches('"').trim())
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The first `name` + extension (tried in `exts` order, e.g. `.COM;.EXE`) that `exists` along
/// `path`: what `where.exe` and `cmd` answer.
pub(crate) fn resolve_on_path(
    path: &str,
    name: &str,
    exts: &[String],
    exists: impl Fn(&std::path::Path) -> bool,
) -> Option<std::path::PathBuf> {
    split_win_path(path).into_iter().find_map(|dir| {
        let dir = dir.trim_end_matches(['\\', '/']);
        exts.iter().find_map(|ext| {
            let candidate = std::path::PathBuf::from(format!("{dir}\\{name}{ext}"));
            exists(&candidate).then_some(candidate)
        })
    })
}

/// `PATHEXT` as a list of lower-case extensions (`.com`, `.exe`, …; Windows ignores the case and
/// the files are `atlas.exe`, so doctor prints the name as it is on disk); Windows' default when
/// it is unset or empty.
pub(crate) fn path_exts(pathext: Option<&str>) -> Vec<String> {
    let list: Vec<String> = pathext
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_lowercase)
        .collect();
    if list.is_empty() {
        [".com", ".exe", ".bat", ".cmd"].map(String::from).to_vec()
    } else {
        list
    }
}

/// The `PATH` a new terminal gets: the system `Path` then the user's, each expanded with
/// `lookup`. A missing value contributes nothing.
pub(crate) fn terminal_path(
    machine: Option<&str>,
    user: Option<&str>,
    lookup: impl Fn(&str) -> Option<String> + Copy,
) -> String {
    [machine, user]
        .into_iter()
        .flatten()
        .map(|v| expand_env(v, lookup))
        .collect::<Vec<_>>()
        .join(";")
}

/// Whether `found` is `dir` or below it, the way Windows compares paths: case and `/` vs `\`
/// do not matter.
pub(crate) fn is_under_win(found: &str, dir: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let (found, dir) = (norm(found), norm(dir));
    let dir = dir.trim_end_matches('\\');
    found
        .strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

/// One value of the registry through `reg.exe`: `None` when the key or value does not exist.
fn reg_value(key: &str, name: &str) -> Option<String> {
    use atlas_core::sys::CommandExt as _;
    let out = std::process::Command::new("reg")
        .args(["query", key, "/v", name])
        .no_window()
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| parse_reg_query(&String::from_utf8_lossy(&out.stdout), name))?
}

/// The system and the user `Path` as stored (unexpanded).
pub(crate) fn registry_paths() -> (Option<String>, Option<String>) {
    (reg_value(MACHINE_KEY, "Path"), reg_value(USER_KEY, "Path"))
}

/// What a new terminal sees: its `PATH` and the `atlas` it resolves, from the registry. `None`
/// when neither `Path` can be read.
pub(crate) fn terminal_view() -> Option<(String, Option<std::path::PathBuf>)> {
    let (machine, user) = registry_paths();
    if machine.is_none() && user.is_none() {
        return None;
    }
    let path = terminal_path(machine.as_deref(), user.as_deref(), |n| {
        std::env::var(n).ok()
    });
    let exts = path_exts(std::env::var("PATHEXT").ok().as_deref());
    let atlas = resolve_on_path(&path, "atlas", &exts, |p| p.is_file());
    Some((path, atlas))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `reg query HKCU\Environment /v Path` prints on Windows 11 (CRLF, a blank first line,
    /// four spaces between the columns), for an account where Atlas's installer ran.
    const USER_QUERY: &str = "\r\nHKEY_CURRENT_USER\\Environment\r\n    Path    REG_EXPAND_SZ    %USERPROFILE%\\AppData\\Local\\Microsoft\\WindowsApps;%USERPROFILE%\\.atlas\\bin;C:\\Program Files\\nodejs\\\r\n\r\n";

    /// The system one: mixed-case references, a trailing backslash, spaces in directories.
    const MACHINE_QUERY: &str = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment\r\n    Path    REG_EXPAND_SZ    %SystemRoot%\\system32;%SystemRoot%;%SystemRoot%\\System32\\Wbem;%SYSTEMROOT%\\System32\\WindowsPowerShell\\v1.0\\;C:\\Program Files\\Git\\cmd\r\n\r\n";

    fn env(n: &str) -> Option<String> {
        match n.to_ascii_lowercase().as_str() {
            "userprofile" => Some(r"C:\Users\Ana Souza".into()),
            "systemroot" => Some(r"C:\Windows".into()),
            _ => None,
        }
    }

    #[test]
    fn a_reg_expand_sz_path_is_read_with_its_references_unexpanded() {
        let v = parse_reg_query(USER_QUERY, "Path").unwrap();
        assert_eq!(
            v,
            r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;%USERPROFILE%\.atlas\bin;C:\Program Files\nodejs\"
        );
    }

    #[test]
    fn a_plain_reg_sz_and_an_empty_value_parse_and_other_types_do_not() {
        assert_eq!(
            parse_reg_query(
                "HKEY_CURRENT_USER\\Environment\r\n    Path    REG_SZ    C:\\a;C:\\b\r\n",
                "Path"
            )
            .as_deref(),
            Some(r"C:\a;C:\b")
        );
        assert_eq!(
            parse_reg_query("    Path    REG_SZ    \r\n", "Path").as_deref(),
            Some("")
        );
        // A binary or numeric value is not a Path, and `PathExt` is not `Path`.
        assert_eq!(
            parse_reg_query("    Path    REG_DWORD    0x1\r\n", "Path"),
            None
        );
        assert_eq!(
            parse_reg_query("    PATHEXT    REG_SZ    .COM;.EXE\r\n", "Path"),
            None
        );
        assert_eq!(
            parse_reg_query(
                "ERROR: unable to find the specified registry key or value\r\n",
                "Path"
            ),
            None
        );
    }

    #[test]
    fn the_value_name_is_matched_whatever_its_case() {
        assert_eq!(
            parse_reg_query("    PATH    REG_SZ    C:\\a\r\n", "Path").as_deref(),
            Some(r"C:\a")
        );
    }

    #[test]
    fn references_expand_whatever_their_case_and_unknown_ones_stay() {
        assert_eq!(
            expand_env(
                r"%SYSTEMROOT%\System32;%UserProfile%\bin;%NOPE%\x;100%;%%",
                env
            ),
            r"C:\Windows\System32;C:\Users\Ana Souza\bin;%NOPE%\x;100%;%%"
        );
    }

    #[test]
    fn a_new_terminal_path_is_the_system_path_then_the_users() {
        let p = terminal_path(
            parse_reg_query(MACHINE_QUERY, "Path").as_deref(),
            parse_reg_query(USER_QUERY, "Path").as_deref(),
            env,
        );
        let dirs = split_win_path(&p);
        assert_eq!(dirs[0], r"C:\Windows\system32");
        assert_eq!(dirs[3], r"C:\Windows\System32\WindowsPowerShell\v1.0\");
        assert_eq!(
            dirs[5],
            r"C:\Users\Ana Souza\AppData\Local\Microsoft\WindowsApps"
        );
        assert_eq!(dirs[6], r"C:\Users\Ana Souza\.atlas\bin");
        // Without a user Path (a fresh account) the system one alone is the PATH.
        assert_eq!(terminal_path(Some(r"C:\a"), None, env), r"C:\a");
    }

    #[test]
    fn entries_are_unquoted_and_blank_ones_dropped() {
        assert_eq!(
            split_win_path(r#"C:\a;;"C:\Program Files\b" ; ;C:\c"#),
            [r"C:\a", r"C:\Program Files\b", r"C:\c"]
        );
    }

    #[test]
    fn atlas_resolves_along_the_expanded_path_in_order_and_by_extension() {
        let path = terminal_path(
            parse_reg_query(MACHINE_QUERY, "Path").as_deref(),
            parse_reg_query(USER_QUERY, "Path").as_deref(),
            env,
        );
        let exts = path_exts(None);
        let has = |files: &'static [&'static str]| {
            move |p: &std::path::Path| files.contains(&p.to_str().unwrap())
        };
        // The installer's copy, found through `%USERPROFILE%`.
        assert_eq!(
            resolve_on_path(
                &path,
                "atlas",
                &exts,
                has(&[r"C:\Users\Ana Souza\.atlas\bin\atlas.exe"])
            ),
            Some(r"C:\Users\Ana Souza\.atlas\bin\atlas.exe".into())
        );
        // Another atlas earlier on the system Path wins over it.
        assert_eq!(
            resolve_on_path(
                &path,
                "atlas",
                &exts,
                has(&[
                    r"C:\Program Files\Git\cmd\atlas.exe",
                    r"C:\Users\Ana Souza\.atlas\bin\atlas.exe"
                ])
            ),
            Some(r"C:\Program Files\Git\cmd\atlas.exe".into())
        );
        // A directory with a trailing backslash does not double it.
        assert_eq!(
            resolve_on_path(
                &path,
                "atlas",
                &exts,
                has(&[r"C:\Program Files\nodejs\atlas.cmd"])
            ),
            Some(r"C:\Program Files\nodejs\atlas.cmd".into())
        );
        assert_eq!(resolve_on_path(&path, "atlas", &exts, has(&[])), None);
    }

    #[test]
    fn pathext_comes_from_the_environment_or_the_windows_default() {
        assert_eq!(path_exts(Some(".EXE;;.PS1 ")), [".exe", ".ps1"]);
        assert_eq!(path_exts(Some("")), [".com", ".exe", ".bat", ".cmd"]);
    }

    #[test]
    fn paths_compare_the_way_windows_does() {
        assert!(is_under_win(
            r"C:\Users\Ana\.atlas\bin\atlas.exe",
            r"c:\users\ana\.atlas\bin"
        ));
        assert!(is_under_win(
            "C:/Users/Ana/.atlas/bin/atlas.exe",
            r"C:\Users\Ana\.atlas\bin\"
        ));
        assert!(!is_under_win(
            r"C:\Users\Ana\.atlas\binary\atlas.exe",
            r"C:\Users\Ana\.atlas\bin"
        ));
        assert!(!is_under_win(
            r"C:\Windows\atlas.exe",
            r"C:\Users\Ana\.atlas\bin"
        ));
    }

    /// The real registry of the Windows machine this runs on (CI's runner included): reading
    /// `HKLM` and `HKCU` with `reg.exe` agrees with what PowerShell's own reader says about the user
    /// Path, and a program every Windows has resolves along the result.
    #[cfg(windows)]
    #[test]
    fn the_real_registry_paths_read_and_resolve() {
        let (machine, user) = registry_paths();
        let machine = machine.expect("HKLM Path");
        assert!(
            machine.to_lowercase().contains("system32"),
            "system Path: {machine}"
        );
        let (path, _) = terminal_view().expect("the registry's Path values");
        let exts = path_exts(std::env::var("PATHEXT").ok().as_deref());
        let cmd =
            resolve_on_path(&path, "cmd", &exts, |p| p.is_file()).expect("cmd along the Path");
        assert!(
            cmd.to_string_lossy()
                .to_lowercase()
                .ends_with(r"system32\cmd.exe"),
            "{cmd:?}"
        );
        assert!(!path.contains('%'), "unexpanded reference in {path}");
        // The user Path as `reg.exe` + our expansion gives it is the one .NET (PowerShell) reads and expands itself.
        let ours = split_win_path(&terminal_path(None, user.as_deref(), |n| {
            std::env::var(n).ok()
        }));
        let ps = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::Out.Write([Environment]::GetEnvironmentVariable('Path', 'User'))",
            ])
            .output()
            .expect("powershell");
        let theirs = split_win_path(&String::from_utf8_lossy(&ps.stdout));
        let lower = |v: Vec<String>| v.into_iter().map(|d| d.to_lowercase()).collect::<Vec<_>>();
        assert_eq!(
            lower(ours),
            lower(theirs),
            "HKCU Path: reg.exe vs PowerShell"
        );
    }
}
