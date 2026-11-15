//! The shell startup lines `atlas install` writes so `atlas` is on `PATH`, and the exact inverse
//! `atlas uninstall` applies. One module owns both halves so they cannot drift: what install
//! appends is what uninstall removes, and nothing else (a user's own line that merely mentions
//! `.atlas` is never touched).
//!
//! Every function takes the directories explicitly ([`ShellEnv`]) instead of reading `$HOME`, so
//! tests run against a temp dir without mutating the process environment.

use std::path::{Path, PathBuf};

/// First line of the block Atlas appends to an rc file (and of the fish and nushell files).
pub(crate) const MARKER: &str = "# Atlas CLI";

/// Where shells look for their startup files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellEnv {
    pub home: PathBuf,
    /// `$ZDOTDIR` (zsh reads `.zshenv` there instead of in `$HOME`).
    pub zdotdir: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME` (fish's `conf.d` lives under it).
    pub xdg_config: Option<PathBuf>,
}

impl ShellEnv {
    /// The real process environment. Empty values count as unset, like the shells do.
    pub(crate) fn current() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            home: atlas_core::paths::home_dir(),
            zdotdir: var("ZDOTDIR"),
            xdg_config: var("XDG_CONFIG_HOME"),
        }
    }

    /// `<xdg or ~/.config>/fish/conf.d`.
    pub(crate) fn fish_conf_d(&self) -> PathBuf {
        self.xdg_config
            .clone()
            .unwrap_or_else(|| self.home.join(".config"))
            .join("fish")
            .join("conf.d")
    }

    /// The file install writes for fish.
    pub(crate) fn fish_file(&self) -> PathBuf {
        self.fish_conf_d().join("atlas.fish")
    }

    /// nushell's user autoload dir (0.102+ loads every `*.nu` in it): under `$XDG_CONFIG_HOME`
    /// when set, else `~/.config/nushell` (`~/Library/Application Support/nushell` on macOS).
    pub(crate) fn nu_autoload(&self) -> PathBuf {
        let config = match &self.xdg_config {
            Some(x) => x.clone(),
            None if cfg!(target_os = "macos") => self.home.join("Library/Application Support"),
            None => self.home.join(".config"),
        };
        config.join("nushell").join("autoload")
    }

    /// The file install writes for nushell.
    pub(crate) fn nu_file(&self) -> PathBuf {
        self.nu_autoload().join("atlas.nu")
    }

    /// `~/.local/bin/atlas`: the link install makes when `~/.local/bin` is already on `PATH`, so
    /// `atlas` works at once, in non-interactive shells and in GUI-launched tools too.
    pub(crate) fn local_bin_link(&self) -> PathBuf {
        self.home
            .join(".local/bin")
            .join(atlas_core::paths::exe("atlas"))
    }

    /// `$ZDOTDIR/.zshenv`, else `~/.zshenv`: where install puts the zsh line.
    pub(crate) fn zsh_file(&self) -> PathBuf {
        self.zdotdir
            .clone()
            .unwrap_or_else(|| self.home.clone())
            .join(".zshenv")
    }

    /// Where install puts the bash line. macOS terminals start login shells, which read only the
    /// first of the login files that exists, so it goes there (none: nothing to write). Linux
    /// terminals start non-login interactive shells, which read only `~/.bashrc` (distros' login
    /// files source it), so it goes there, created when missing.
    pub(crate) fn bash_file(&self) -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            [".bash_profile", ".bash_login", ".bashrc"]
                .iter()
                .map(|f| self.home.join(f))
                .find(|p| p.is_file())
        } else {
            Some(self.home.join(".bashrc"))
        }
    }

    /// Every file that may hold an Atlas block: where install writes, plus the other places an
    /// earlier version or a hand edit could have left one. Only existing files.
    pub(crate) fn candidates(&self) -> Vec<PathBuf> {
        let mut v = vec![
            self.zsh_file(),
            self.home.join(".zshenv"),
            self.home.join(".zshrc"),
        ];
        v.extend(
            [".bash_profile", ".bash_login", ".bashrc", ".profile"]
                .iter()
                .map(|f| self.home.join(f)),
        );
        v.retain(|p| p.is_file());
        v.sort();
        v.dedup();
        v
    }
}

/// The `PATH` line for POSIX shells.
pub(crate) fn path_line(dir: &Path) -> String {
    format!("export PATH=\"{}:$PATH\"", dir.display())
}

/// The fish line. `-g` (this session) because the default is the *universal* `fish_user_paths`,
/// which fish keeps on disk: deleting the file would leave the dir on `PATH` forever. `-P`
/// (`--path`) changes `$PATH` directly: plain `-g` would create a global `fish_user_paths` that
/// shadows the universal one, and the user's own later `fish_add_path` would die with the shell.
/// Quoted with fish's double-quote escapes so a path with a space or `$` survives.
pub(crate) fn fish_line(dir: &Path) -> String {
    let d = dir.display().to_string();
    let q = d
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$");
    format!("fish_add_path -gP \"{q}\"")
}

/// The line install wrote up to 1.5.0-alpha.25 (universal, unquoted). A file with it is still
/// ours: install rewrites it, uninstall deletes it.
fn fish_line_legacy(dir: &Path) -> String {
    format!("fish_add_path {}", dir.display())
}

/// The nushell line: prepend `dir` to `PATH` (a string or a list at that point), without dupes.
/// A raw string, so nothing in the path is interpreted.
pub(crate) fn nu_line(dir: &Path) -> String {
    format!(
        "$env.PATH = ($env.PATH | split row (char esep) | prepend r#'{}'# | uniq)",
        dir.display()
    )
}

/// What install appends to a POSIX rc file.
pub(crate) fn block(dir: &Path) -> String {
    format!("\n{MARKER}\n{}\n", path_line(dir))
}

/// Contents install gives the fish `conf.d` file.
pub(crate) fn fish_contents(dir: &Path) -> String {
    format!("{MARKER}\n{}\n", fish_line(dir))
}

/// Contents install gives the nushell autoload file.
pub(crate) fn nu_contents(dir: &Path) -> String {
    format!("{MARKER}\n{}\n", nu_line(dir))
}

/// Whether `content` already puts `dir` on `PATH`: the exact line install writes (or the
/// unquoted spelling people type by hand). A longer path that merely starts with `dir`
/// (`.atlas/bin-old`) or a comment that mentions it does not count.
pub(crate) fn has_path_entry(content: &str, dir: &Path) -> bool {
    let quoted = path_line(dir);
    let bare = format!("export PATH={}:$PATH", dir.display());
    content.lines().any(|l| {
        let l = l.trim();
        l == quoted || l == bare
    })
}

/// `content` without the block install wrote for `dir`: the `# Atlas CLI` marker line and the
/// PATH line right after it, and the one blank line install put in front. `None` when there is no
/// such block. A line of the user's own, even one that mentions `.atlas`, stays.
pub(crate) fn strip_block(content: &str, dir: &Path) -> Option<String> {
    let line = path_line(dir);
    let lines: Vec<&str> = content.lines().collect();
    let at = lines
        .windows(2)
        .position(|w| w[0].trim() == MARKER && w[1].trim() == line)?;
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend(&lines[..at]);
    // The blank line install put before the marker goes with it.
    if out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.extend(&lines[at + 2..]);
    let mut s = out.join("\n");
    if content.ends_with('\n') && !s.is_empty() {
        s.push('\n');
    }
    // Several blocks (an installer run twice before the idempotency fix): strip them all.
    Some(strip_block(&s, dir).unwrap_or(s))
}

/// Whether `content` is a fish file install wrote, this version's or an older one (so deleting
/// the file removes only Atlas's own).
pub(crate) fn is_fish_file_ours(content: &str, dir: &Path) -> bool {
    let c = content.trim();
    c == fish_contents(dir).trim() || c == format!("{MARKER}\n{}", fish_line_legacy(dir))
}

/// Whether `content` is exactly the nushell file install writes.
pub(crate) fn is_nu_file_ours(content: &str, dir: &Path) -> bool {
    content.trim() == nu_contents(dir).trim()
}

/// The line that puts `dir` on `PATH` in the shell named `shell` (a basename: `fish`, `zsh`),
/// for the installer's "run now" hint. `None` for a shell whose syntax Atlas does not write.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn line_for_shell(shell: &str, dir: &Path) -> Option<String> {
    match shell {
        "fish" => Some(fish_line(dir)),
        "nu" => Some(nu_line(dir)),
        "bash" | "zsh" | "sh" | "dash" | "ksh" | "mksh" | "yash" => Some(path_line(dir)),
        _ => None,
    }
}

/// Run `script` in fish with `dir` as `$argv[1]`. A plain `fish -c`: `--no-config` would also skip
/// the universal variables this is about. `Ok(exit status success)`; `Err` when fish cannot run.
fn fish_run(env: &ShellEnv, script: &str, dir: &Path) -> anyhow::Result<bool> {
    let mut cmd = std::process::Command::new("fish");
    // The same HOME/XDG Atlas resolved every other path from, so fish changes the same files.
    cmd.env("HOME", &env.home);
    match &env.xdg_config {
        Some(x) => cmd.env("XDG_CONFIG_HOME", x),
        None => cmd.env_remove("XDG_CONFIG_HOME"),
    };
    let st = cmd
        .args(["-c", script])
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    Ok(st.success())
}

/// Whether fish's universal `fish_user_paths` holds `dir` (put there by the older fish line).
pub(crate) fn fish_universal_has(env: &ShellEnv, dir: &Path) -> bool {
    fish_run(env, "contains -- $argv[1] $fish_user_paths", dir).unwrap_or(false)
}

/// Take `dir` out of fish's universal `fish_user_paths`. `Ok(true)` when it was there.
pub(crate) fn fish_forget_universal(env: &ShellEnv, dir: &Path) -> anyhow::Result<bool> {
    fish_run(
        env,
        "contains -- $argv[1] $fish_user_paths; or exit 1; \
         set -U fish_user_paths (string match -v -- $argv[1] $fish_user_paths)",
        dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home with a space (or `$`, `"`) must still give fish one path argument.
    #[test]
    fn fish_line_quotes_the_path() {
        let d = PathBuf::from("/home/a b/$x\"/.atlas/bin");
        assert_eq!(
            fish_line(&d),
            "fish_add_path -gP \"/home/a b/\\$x\\\"/.atlas/bin\""
        );
        // The file an older install wrote is still recognized as ours, a user edit is not.
        let legacy = format!("{MARKER}\nfish_add_path {}\n", dir().display());
        assert!(is_fish_file_ours(&legacy, &dir()));
        assert!(!is_fish_file_ours(&format!("{legacy}set -x A 1\n"), &dir()));
    }

    #[test]
    fn line_for_shell_speaks_each_shells_syntax() {
        assert!(
            line_for_shell("fish", &dir())
                .unwrap()
                .starts_with("fish_add_path -gP")
        );
        assert!(
            line_for_shell("nu", &dir())
                .unwrap()
                .starts_with("$env.PATH")
        );
        assert!(
            line_for_shell("zsh", &dir())
                .unwrap()
                .starts_with("export PATH=")
        );
        assert_eq!(line_for_shell("tcsh", &dir()), None);
    }

    fn dir() -> PathBuf {
        PathBuf::from("/home/u/.atlas/bin")
    }

    /// What install appends and uninstall strips are the same thing: a round trip through the real
    /// `block` returns the user's file byte for byte.
    #[test]
    fn strip_undoes_exactly_what_install_appends() {
        for original in [
            "",
            "export A=1\n",
            "export A=1",
            "# my notes\nalias ll='ls -l'\n\nexport B=2\n",
        ] {
            let mut written = original.to_string();
            if !written.is_empty() && !written.ends_with('\n') {
                written.push('\n');
            }
            written.push_str(&block(&dir()));
            let back = strip_block(&written, &dir()).expect("block found");
            let want = if original.is_empty() || original.ends_with('\n') {
                original.to_string()
            } else {
                format!("{original}\n")
            };
            assert_eq!(back, want, "round trip of {original:?}");
        }
    }

    /// Incident class: the old uninstaller deleted every line containing `.atlas`, which also ate
    /// the user's own startup lines.
    #[test]
    fn strip_keeps_the_users_own_lines_that_mention_dot_atlas() {
        let mine = "source ~/.atlas/env.sh\nexport ATLAS_HOME=\"$HOME/.atlas-projects\"\n# ~/.atlas is where my notes live\nexport PATH=\"$HOME/.atlas/tools:$PATH\"\n";
        let file = format!("{mine}{}", block(&dir()));
        assert_eq!(strip_block(&file, &dir()).as_deref(), Some(mine));
    }

    #[test]
    fn strip_finds_nothing_in_a_file_without_the_block() {
        assert_eq!(strip_block("export A=1\n", &dir()), None);
        // The marker alone, or the line alone, is not a block install wrote.
        assert_eq!(
            strip_block(&format!("{MARKER}\nexport A=1\n"), &dir()),
            None
        );
        assert_eq!(
            strip_block(&format!("{}\n", path_line(&dir())), &dir()),
            None
        );
        // Another install dir's block is not ours.
        let other = PathBuf::from("/opt/x/.atlas/bin");
        assert_eq!(strip_block(&block(&other), &dir()), None);
    }

    #[test]
    fn strip_removes_every_copy_of_the_block() {
        let file = format!("a\n{}b\n{}", block(&dir()), block(&dir()));
        assert_eq!(strip_block(&file, &dir()).as_deref(), Some("a\nb\n"));
    }

    #[test]
    fn strip_handles_windows_line_endings_in_the_marker_lines() {
        let file = format!("a\r\n\r\n{MARKER}\r\n{}\r\nb\r\n", path_line(&dir()));
        let got = strip_block(&file, &dir()).unwrap();
        assert!(
            !got.contains("Atlas CLI") && !got.contains(".atlas/bin"),
            "{got:?}"
        );
        assert!(got.contains('a') && got.contains('b'), "{got:?}");
    }

    /// Install must not call a PATH "already set" because a longer path or a comment starts with it.
    #[test]
    fn has_path_entry_needs_the_exact_line() {
        assert!(has_path_entry(&block(&dir()), &dir()));
        assert!(has_path_entry(
            "export PATH=/home/u/.atlas/bin:$PATH\n",
            &dir()
        ));
        assert!(!has_path_entry(
            "export PATH=\"/home/u/.atlas/bin-old:$PATH\"\n",
            &dir()
        ));
        assert!(!has_path_entry(
            "# /home/u/.atlas/bin is on my PATH\n",
            &dir()
        ));
        assert!(!has_path_entry("", &dir()));
    }

    #[test]
    fn fish_file_is_ours_only_when_it_is_what_install_writes() {
        assert!(is_fish_file_ours(&fish_contents(&dir()), &dir()));
        assert!(!is_fish_file_ours(
            &format!("{}set -x FOO bar\n", fish_contents(&dir())),
            &dir()
        ));
        assert!(!is_fish_file_ours(
            "function atlas; echo mine; end\n",
            &dir()
        ));
    }

    #[test]
    fn locations_follow_zdotdir_and_xdg_config_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let plain = ShellEnv {
            home: home.clone(),
            zdotdir: None,
            xdg_config: None,
        };
        assert_eq!(plain.zsh_file(), home.join(".zshenv"));
        assert_eq!(
            plain.fish_file(),
            home.join(".config/fish/conf.d/atlas.fish")
        );

        let moved = ShellEnv {
            home: home.clone(),
            zdotdir: Some(home.join("zdot")),
            xdg_config: Some(home.join("xdg")),
        };
        assert_eq!(moved.zsh_file(), home.join("zdot/.zshenv"));
        assert_eq!(moved.fish_file(), home.join("xdg/fish/conf.d/atlas.fish"));
    }

    /// An rc file in `$ZDOTDIR` is one uninstall must look at (the old code only read `$HOME`).
    #[test]
    fn candidates_include_the_zdotdir_file_and_only_existing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        std::fs::create_dir(home.join("zdot")).unwrap();
        std::fs::write(home.join("zdot/.zshenv"), "x").unwrap();
        std::fs::write(home.join(".bashrc"), "x").unwrap();
        let env = ShellEnv {
            home: home.clone(),
            zdotdir: Some(home.join("zdot")),
            xdg_config: None,
        };
        assert_eq!(
            env.candidates(),
            vec![home.join(".bashrc"), home.join("zdot/.zshenv")]
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn bash_file_is_the_first_existing_of_the_login_files() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let env = ShellEnv {
            home: home.clone(),
            zdotdir: None,
            xdg_config: None,
        };
        assert_eq!(env.bash_file(), None);
        std::fs::write(home.join(".bashrc"), "").unwrap();
        assert_eq!(env.bash_file(), Some(home.join(".bashrc")));
        std::fs::write(home.join(".bash_profile"), "").unwrap();
        assert_eq!(env.bash_file(), Some(home.join(".bash_profile")));
    }

    /// Incident (Omarchy, 1.5.0-alpha.25): `~/.bash_profile` existed, so install wrote the PATH
    /// there; a Linux terminal starts a non-login bash that reads only `~/.bashrc`, and every new
    /// shell said `atlas: command not found`. On Linux the line goes to `~/.bashrc`, created when
    /// missing, whatever login files exist.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn bash_file_on_linux_is_bashrc_even_with_a_login_file() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let env = ShellEnv {
            home: home.clone(),
            zdotdir: None,
            xdg_config: None,
        };
        assert_eq!(env.bash_file(), Some(home.join(".bashrc")));
        std::fs::write(
            home.join(".bash_profile"),
            "[[ -f ~/.bashrc ]] && . ~/.bashrc\n",
        )
        .unwrap();
        assert_eq!(env.bash_file(), Some(home.join(".bashrc")));
    }
}
