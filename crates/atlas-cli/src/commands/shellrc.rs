//! The shell startup lines `atlas install` writes so `atlas` is on `PATH`, and the exact inverse
//! `atlas uninstall` applies. One module owns both halves so they cannot drift: what install
//! appends is what uninstall removes, and nothing else (a user's own line that merely mentions
//! `.atlas` is never touched).
//!
//! Every function takes the directories explicitly ([`ShellEnv`]) instead of reading `$HOME`, so
//! tests run against a temp dir without mutating the process environment.

use std::path::{Path, PathBuf};

/// First line of the block Atlas appends to an rc file (and of the fish `conf.d` file).
pub(crate) const MARKER: &str = "# Atlas CLI";

/// Where shells look for their startup files.
#[derive(Debug, Clone)]
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

    /// `$ZDOTDIR/.zshenv`, else `~/.zshenv`: where install puts the zsh line.
    pub(crate) fn zsh_file(&self) -> PathBuf {
        self.zdotdir
            .clone()
            .unwrap_or_else(|| self.home.clone())
            .join(".zshenv")
    }

    /// The first bash startup file that exists (bash reads only the first of these for a login
    /// shell), if any.
    pub(crate) fn bash_file(&self) -> Option<PathBuf> {
        [".bash_profile", ".bash_login", ".bashrc"]
            .iter()
            .map(|f| self.home.join(f))
            .find(|p| p.is_file())
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

/// The fish line.
pub(crate) fn fish_line(dir: &Path) -> String {
    format!("fish_add_path {}", dir.display())
}

/// What install appends to a POSIX rc file.
pub(crate) fn block(dir: &Path) -> String {
    format!("\n{MARKER}\n{}\n", path_line(dir))
}

/// Contents install gives the fish `conf.d` file.
pub(crate) fn fish_contents(dir: &Path) -> String {
    format!("{MARKER}\n{}\n", fish_line(dir))
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

/// Whether `content` is exactly what install writes for fish (so deleting the file removes only
/// Atlas's own).
pub(crate) fn is_fish_file_ours(content: &str, dir: &Path) -> bool {
    content.trim() == fish_contents(dir).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
