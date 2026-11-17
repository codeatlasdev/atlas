//! The servers the user already has in `~/.ssh/config`, for "Add a crew server" (#502).
//!
//! Two steps, each doing what only it can do right:
//! 1. [`host_aliases`] reads the file the way ssh does for the one question "which names can I type": `Host`
//!    lines, `Include` (globs, relative to `~/.ssh`), quotes and comments; wildcard and negated patterns and
//!    `Match` blocks never name a server.
//! 2. [`resolve`] asks `ssh -G <alias>` what each name means (user, host name, port, key). `Match` blocks, `Host *`
//!    defaults and first-value-wins precedence are ssh's to apply, not ours: a second parser would drift from the
//!    one that connects. `-G` only prints the effective configuration, it never opens a connection.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;

/// OpenSSH stops following `Include` at this depth, which also ends a file that includes itself.
const MAX_INCLUDE_DEPTH: usize = 16;
/// `ssh -G` calls running at once: a config with a hundred hosts is a hundred processes.
const WORKERS: usize = 8;
/// One `ssh -G` that does not answer (a `Match exec` that hangs) costs one host, not the picker.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// One of the user's ssh servers, as the picker shows it and as `ssh <alias>` would reach it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SshHost {
    /// The name in `~/.ssh/config`.
    pub alias: String,
    pub user: String,
    pub hostname: String,
    pub port: u16,
    /// The first key file of `ssh -G` that exists on this Mac (`~/.ssh/id_work`), for display.
    pub identity_file: Option<String>,
    /// What goes in the SSH address field: `user@alias`. The alias, not `hostname`, so ssh keeps applying this
    /// entry's key, port and `ProxyJump` to every call Atlas makes to it.
    pub address: String,
}

/// Every name the user can type to reach a server: the plain patterns of the `Host` lines of `~/.ssh/config` and of
/// what it includes, in file order, once each.
pub fn host_aliases(home: &Path) -> Vec<String> {
    let ssh_dir = home.join(".ssh");
    let mut names = Vec::new();
    collect(&ssh_dir.join("config"), home, &ssh_dir, 0, &mut names);
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

/// The names of `file`'s `Host` lines, then of every file its `Include` lines reach, at the line they stand on.
/// An unreadable file or a missing include is skipped, as ssh skips a glob with no match.
fn collect(file: &Path, home: &Path, ssh_dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_INCLUDE_DEPTH {
        return;
    }
    let Ok(bytes) = std::fs::read(file) else {
        return;
    };
    for line in String::from_utf8_lossy(&bytes).lines() {
        let Some((key, args)) = split_line(line) else {
            continue;
        };
        match key.as_str() {
            "host" => out.extend(plain_names(&args)),
            "include" => {
                for pattern in &args {
                    for f in expand_include(pattern, home, ssh_dir) {
                        collect(&f, home, ssh_dir, depth + 1, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The patterns of one `Host` line that name a single server: no `*` or `?`, not negated, and not negated by
/// another pattern of the same line. `Match` lines are never read as names.
fn plain_names(patterns: &[String]) -> impl Iterator<Item = String> + '_ {
    patterns.iter().filter_map(move |p| {
        let negated = |n: &str| patterns.iter().any(|x| x.strip_prefix('!') == Some(n));
        let plain = !p.starts_with('!') && !p.contains(['*', '?']) && !negated(p);
        plain.then(|| p.clone())
    })
}

/// One config line as ssh reads it: the keyword (lowercase), then its arguments. The keyword ends at a blank or an
/// `=`, which may have blanks around it (`Host=a`, `Host = a`). `None` for an empty line or a comment.
fn split_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let (key, rest) = line.split_at(end);
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest);
    Some((key.to_ascii_lowercase(), split_args(rest)))
}

/// Arguments split on blanks, with `"…"` and `'…'` grouping, a backslash escaping a quote, a backslash or (outside
/// quotes) a blank, and a `#` at the start of a word ending the line (`d#e` is one word, `a #b` is a comment).
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = s.trim_start().chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), '\\') if matches!(chars.peek(), Some('"' | '\'' | '\\')) => {
                cur.extend(chars.next());
            }
            (None, '\\') if matches!(chars.peek(), Some('"' | '\'' | '\\' | ' ')) => {
                cur.extend(chars.next());
                in_word = true;
            }
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, '#') if !in_word => break,
            (None, c) if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            (_, c) => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

/// The files an `Include` argument names: `~/` is the home, a relative path is relative to `~/.ssh`, and `*`, `?`
/// and `[…]` in any path segment match as in glob(3) (sorted, no dotfiles unless the segment starts with a dot).
fn expand_include(pattern: &str, home: &Path, ssh_dir: &Path) -> Vec<PathBuf> {
    let full = match pattern.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => ssh_dir.join(pattern),
    };
    let mut paths = vec![PathBuf::new()];
    for part in full.components() {
        match part {
            std::path::Component::Normal(seg)
                if seg.to_string_lossy().contains(['*', '?', '[']) =>
            {
                paths = paths.iter().flat_map(|dir| matching(dir, seg)).collect();
            }
            other => paths.iter_mut().for_each(|p| p.push(other.as_os_str())),
        }
    }
    paths.retain(|p| p.is_file());
    paths
}

/// The entries of `dir` whose name matches the glob segment `seg`, sorted.
fn matching(dir: &Path, seg: &std::ffi::OsStr) -> Vec<PathBuf> {
    let seg = seg.to_string_lossy();
    let Ok(glob) = globset::GlobBuilder::new(&seg)
        .literal_separator(true)
        .build()
    else {
        return Vec::new();
    };
    let glob = glob.compile_matcher();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut hits: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (seg.starts_with('.') || !name.starts_with('.')) && glob.is_match(&name)
        })
        .map(|e| e.path())
        .collect();
    hits.sort();
    hits
}

/// The effective settings `ssh -G` printed for `alias`, as far as a server entry needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub user: String,
    pub hostname: String,
    pub port: u16,
    /// Every `identityfile` line, as printed (`~` not expanded).
    pub identity_files: Vec<String>,
}

/// Read `ssh -G` output (lowercase `key value` lines). `None` when it has no user, host name or valid port.
pub fn parse_resolved(g: &str) -> Option<Resolved> {
    let (mut user, mut hostname, mut port) = (None, None, None);
    let mut identity_files = Vec::new();
    for line in g.lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        let value = value.trim();
        match key {
            "user" => user = user.or(Some(value)),
            "hostname" => hostname = hostname.or(Some(value)),
            "port" => port = port.or(Some(value)),
            "identityfile" => identity_files.push(value.to_string()),
            _ => {}
        }
    }
    Some(Resolved {
        user: user.filter(|u| !u.is_empty())?.to_string(),
        hostname: hostname.filter(|h| !h.is_empty())?.to_string(),
        port: port?.parse().ok().filter(|&p| p != 0)?,
        identity_files,
    })
}

/// The server entry for `alias` from its `ssh -G` output; `None` when it cannot be used as an SSH address (every
/// address goes through [`crate::ssh::valid_target`], so one it would refuse is not offered).
pub fn host_from_g(alias: &str, g: &str, home: &Path) -> Option<SshHost> {
    let r = parse_resolved(g)?;
    let address = format!("{}@{alias}", r.user);
    crate::ssh::valid_target(&address).ok()?;
    let identity_file = r.identity_files.iter().find(|f| {
        let path = match f.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => PathBuf::from(f),
        };
        path.is_file()
    });
    Some(SshHost {
        alias: alias.to_string(),
        user: r.user,
        hostname: r.hostname,
        port: r.port,
        identity_file: identity_file.cloned(),
        address,
    })
}

/// Resolve every alias of `~/.ssh/config` with `ssh -G`, at most [`WORKERS`] at a time, keeping file order. An alias
/// ssh cannot resolve (or that cannot be an address) is left out. No file, no hosts: never an error.
pub fn list_hosts(home: &Path) -> Vec<SshHost> {
    let config = home.join(".ssh/config");
    let aliases = host_aliases(home);
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<SshHost>>> = aliases.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..WORKERS.min(aliases.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(alias) = aliases.get(i) else { break };
                    *slots[i].lock() = resolve(&config, alias, home);
                }
            });
        }
    });
    slots.into_iter().filter_map(Mutex::into_inner).collect()
}

/// `ssh -F <config> -G <alias>`, bounded by [`RESOLVE_TIMEOUT`]. The same file the aliases were read from, so the
/// two steps see one configuration. The alias is checked first: one that starts with `-` would be read as an option.
fn resolve(config: &Path, alias: &str, home: &Path) -> Option<SshHost> {
    crate::ssh::valid_target(alias).ok()?;
    let config = config.to_string_lossy();
    let g = crate::probe::run_bounded("ssh", &["-F", &config, "-G", alias], RESOLVE_TIMEOUT)?;
    host_from_g(alias, &g, home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const G_ALIAS: &str = include_str!("../data/ssh-g-alias.txt");
    const G_DEFAULTS: &str = include_str!("../data/ssh-g-defaults.txt");

    /// A fake home with `~/.ssh/<name>` files.
    fn home(files: &[(&str, &str)]) -> tempfile::TempDir {
        let h = tempfile::tempdir().unwrap();
        for (name, body) in files {
            let p = h.path().join(".ssh").join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        h
    }

    // A realistic file: defaults first, an Include with a glob, a relative Include, an absolute and a `~` one, one
    // line with several names, a negated pattern, quotes, a trailing comment, `=` syntax and Match blocks.
    #[test]
    fn aliases_of_a_realistic_config_with_includes() {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("extra"), "Host abs-box\n  User x\n").unwrap();
        let main = format!(
            r#"# my servers
Host *
  ServerAliveInterval 30
  IdentityFile ~/.ssh/id_ed25519

Include config.d/*.conf
Include work/client ~/.ssh/tilde-hosts
Include {abs}

Host prod staging   # the two EU boxes
  HostName eu.example.com

HOST web-* !web-old db1 db?
  User deploy

host="quoted-one" 'single' "has space"
Host=eq-alias
Host   =   spaced-alias

Match host *.internal
  User bob
Match all
  ForwardAgent no
Host after-match
"#,
            abs = outside.path().join("extra").display()
        );
        let h = home(&[
            ("config", &main),
            ("config.d/10-a.conf", "Host from-a\nHost from-a-second\n"),
            ("config.d/20-b.conf", "Host from-b\n"),
            ("config.d/notes.txt", "Host not-included\n"),
            ("config.d/.hidden.conf", "Host dotfile\n"),
            ("work/client", "Host client-vm\n  HostName 10.1.1.1\n"),
            ("tilde-hosts", "Host tilde-box\n"),
        ]);
        assert_eq!(
            host_aliases(h.path()),
            [
                "from-a",
                "from-a-second",
                "from-b",
                "client-vm",
                "tilde-box",
                "abs-box",
                "prod",
                "staging",
                "db1",
                "quoted-one",
                "single",
                "has space",
                "eq-alias",
                "spaced-alias",
                "after-match",
            ]
        );
    }

    // Wildcards, negations and Match patterns never name a server, and a name negated on its own line is not one.
    #[test]
    fn wildcard_negated_and_match_patterns_are_not_servers() {
        let h = home(&[(
            "config",
            "Host *\nHost *.example.com\nHost web? !web1\nHost a b !a\nMatch host c d\nMatch originalhost e\nHost f\n",
        )]);
        assert_eq!(host_aliases(h.path()), ["b", "f"]);
    }

    // `#` starts a comment only at the start of a word, as ssh reads it.
    #[test]
    fn comments_end_the_line_only_at_a_word_boundary() {
        let h = home(&[("config", "Host a #b c\nHost d#e\n# Host f\n   # Host g\n")]);
        assert_eq!(host_aliases(h.path()), ["a", "d#e"]);
    }

    // No file, an unreadable one (a directory where the file should be) or one that includes itself: an answer,
    // never a panic or a loop.
    #[test]
    fn missing_unreadable_and_self_including_configs_give_a_list() {
        let none = tempfile::tempdir().unwrap();
        assert!(host_aliases(none.path()).is_empty());

        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".ssh/config")).unwrap();
        assert!(host_aliases(dir.path()).is_empty());

        let h = home(&[("config", "Include config\nHost loop\n")]);
        let got = host_aliases(h.path());
        assert_eq!(
            got,
            ["loop"],
            "one entry per name however deep the cycle went"
        );
    }

    // The real output of `ssh -G prod` (OpenSSH 10.5): Match, Host * and precedence are already applied.
    #[test]
    fn the_effective_settings_come_from_real_ssh_g_output() {
        let r = parse_resolved(G_ALIAS).unwrap();
        assert_eq!(r.user, "deploy");
        assert_eq!(r.hostname, "203.0.113.7");
        assert_eq!(r.port, 2222);
        assert_eq!(r.identity_files, ["~/.ssh/id_work"]);

        let d = parse_resolved(G_DEFAULTS).unwrap();
        assert_eq!((d.user.as_str(), d.port), ("alice", 22));
        assert_eq!(d.identity_files.first().unwrap(), "~/.ssh/id_rsa");
        assert!(
            d.identity_files.len() > 1,
            "ssh lists its default key names too"
        );
    }

    #[test]
    fn output_without_a_user_host_or_valid_port_is_not_a_server() {
        assert!(parse_resolved("").is_none());
        assert!(parse_resolved("user a\nhostname h\nport 99999\n").is_none());
        assert!(parse_resolved("user a\nport 22\n").is_none());
        assert!(parse_resolved("hostname h\nport 22\n").is_none());
    }

    #[test]
    fn the_address_is_user_at_alias_and_the_key_shown_is_one_that_exists() {
        let h = home(&[("id_ed25519", "k")]);
        let host = host_from_g("nohost", G_DEFAULTS, h.path()).unwrap();
        assert_eq!(host.address, "alice@nohost");
        assert_eq!(host.hostname, "nohost");
        assert_eq!(host.identity_file.as_deref(), Some("~/.ssh/id_ed25519"));

        let none = tempfile::tempdir().unwrap();
        assert_eq!(
            host_from_g("nohost", G_DEFAULTS, none.path())
                .unwrap()
                .identity_file,
            None
        );

        let prod = host_from_g("prod", G_ALIAS, none.path()).unwrap();
        assert_eq!((prod.address.as_str(), prod.port), ("deploy@prod", 2222));
    }

    // Everything the field accepts goes through `ssh::valid_target`; a name or user it would refuse is left out
    // instead of being offered and failing on "Install".
    #[test]
    fn names_that_cannot_be_an_address_are_left_out() {
        let h = tempfile::tempdir().unwrap();
        assert!(host_from_g("has space", G_ALIAS, h.path()).is_none());
        assert!(host_from_g("-oProxyCommand=x", G_ALIAS, h.path()).is_none());
        assert!(
            host_from_g(
                "prod",
                &G_ALIAS.replace("user deploy", "user a@b"),
                h.path()
            )
            .is_none()
        );
    }

    // What `ssh_hosts_list` hands to `ConnectStep`'s picker: written from the real serializer
    // (`UPDATE_FIXTURES=1 cargo test -p atlas-core ssh_config`), read by `SshHostPicker.test.ts`.
    #[test]
    fn ssh_hosts_fixture_is_the_real_serialization() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/ssh-hosts.json");
        let h = home(&[("id_work", "k")]);
        let hosts = vec![
            host_from_g("prod", G_ALIAS, h.path()).unwrap(),
            host_from_g("nohost", G_DEFAULTS, h.path()).unwrap(),
        ];
        let fresh = serde_json::to_value(&hosts).unwrap();
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&path).expect("fixture missing: run with UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    // The whole path against the real `ssh`: what a parser alone gets wrong. `plain` has no User or Port of its own
    // (the `Host *` defaults and a `Match host` block decide them, and the first value wins); `via-include` comes
    // from another file; a name ssh refuses to resolve is not listed. Unix only: Windows OpenSSH prints key paths
    // in its own form.
    #[cfg(unix)]
    #[test]
    fn list_hosts_resolves_each_alias_with_the_real_ssh() {
        // The Include is absolute: `ssh -F` resolves a relative one against the real `~/.ssh`, not this fake home.
        let more = tempfile::tempdir().unwrap();
        fs::write(
            more.path().join("more"),
            "Host via-include\n  HostName 10.0.0.9\n  User dave\n",
        )
        .unwrap();
        let config = format!(
            "Include {}\n\
             Match originalhost plain\n  Port 2200\n\
             Host alt\n  HostName alt.example.com\n  User carol\n  Port 2022\n  IdentityFile ~/.ssh/id_alt\n\
             Host plain\n  HostName plain.example.com\n\
             Host *\n  User fallback\n  Port 22\n",
            more.path().join("more").display()
        );
        let h = home(&[("config", &config), ("id_alt", "k")]);
        let got = list_hosts(h.path());
        let by = |a: &str| {
            got.iter()
                .find(|x| x.alias == a)
                .unwrap_or_else(|| panic!("{a} missing: {got:?}"))
        };
        assert_eq!(
            got.iter().map(|x| x.alias.as_str()).collect::<Vec<_>>(),
            ["via-include", "alt", "plain"],
            "file order, Include in place"
        );
        let plain = by("plain");
        assert_eq!(
            (plain.user.as_str(), plain.hostname.as_str(), plain.port),
            ("fallback", "plain.example.com", 2200)
        );
        let alt = by("alt");
        assert_eq!((alt.address.as_str(), alt.port), ("carol@alt", 2022));
        assert_eq!(alt.identity_file.as_deref(), Some("~/.ssh/id_alt"));
        assert_eq!(by("via-include").address, "dave@via-include");
    }

    // A config ssh itself rejects resolves nothing: the picker is empty, not an error.
    #[cfg(unix)]
    #[test]
    fn a_config_ssh_rejects_lists_no_hosts() {
        let h = home(&[("config", "Host a\n  NoSuchKeyword yes\n")]);
        assert!(list_hosts(h.path()).is_empty());
    }
}
