//! ssh to a crew server, in ONE place: the target (`user@host` plus a port), the options every
//! caller passes, the `PATH` the agents' tools need, and the key lookup. The CLI
//! (`atlas crew …`), Atlas Desktop (check, install, sign-in, the hostd tunnel) and the hostd relay
//! all build their `ssh` argv here, so a port or a `PATH` fix lands on every path at once.

use std::path::PathBuf;

/// An ssh target: a `~/.ssh/config` alias or `user@host`. Only `[A-Za-z0-9._-]` on each side of
/// at most one `@`, and neither side may start with `-`, so it can never be read as an ssh option.
pub fn valid_target(a: &str) -> Result<(), String> {
    let part = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    let ok = match a.split_once('@') {
        Some((user, host)) => part(user) && part(host),
        None => part(a),
    };
    if ok {
        Ok(())
    } else {
        Err(format!("invalid ssh host alias {a:?}"))
    }
}

/// Where the agents' CLIs live on a crew server. A non-interactive ssh command gets a bare `PATH`
/// (no `~/.profile`), so `claude` in `~/.local/bin` or `codex` in `~/.npm-global/bin` was "not
/// found" there while a login shell finds it. Every remote script starts with [`with_path`].
pub const PATH_EXPORT: &str = r#"export PATH="$HOME/.atlas/bin:$HOME/.local/bin:$HOME/.bun/bin:$HOME/.opencode/bin:$HOME/.npm-global/bin:$PATH""#;

/// `script` with the agents' `PATH` exported first.
pub fn with_path(script: &str) -> String {
    format!("{PATH_EXPORT}; {script}")
}

/// Single-quote for POSIX shells: `a'b` → `'a'\''b'`.
pub fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A crew server's ssh address: `user@host` (or an alias) and the port when it is not 22.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub target: String,
    /// `None` is the default port; 22 is stored as `None`, so one address has one spelling.
    pub port: Option<u16>,
    /// A private key the user picked for this server (`-i`), instead of the ones ssh finds by itself. `None` leaves
    /// the choice to ssh (`~/.ssh/config`, the default key names, the agent).
    pub identity: Option<PathBuf>,
}

/// What an ssh call is for: it decides the options (a one-shot command, a tunnel, a terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// A command whose output is read; never asks anything (`BatchMode`).
    Exec,
    /// `ssh -N -L …`: the connection itself is the product.
    Tunnel,
    /// `ssh -t`: a person types in it.
    Terminal,
}

impl SshTarget {
    pub fn new(target: impl Into<String>, port: Option<u16>) -> Self {
        Self {
            target: target.into(),
            port: port.filter(|&p| p != 22),
            identity: None,
        }
    }

    /// The same address reached with `identity` (a private key file) when there is one.
    pub fn with_identity(mut self, identity: Option<impl Into<PathBuf>>) -> Self {
        self.identity = identity.map(Into::into);
        self
    }

    /// `-i <key> -o IdentitiesOnly=yes` when a key was picked: ssh then offers that key and no other, so the
    /// server is not shown every key of the agent first (and refuses after `MaxAuthTries`).
    fn identity_args(&self) -> Vec<String> {
        match &self.identity {
            Some(k) => vec![
                "-i".to_string(),
                k.display().to_string(),
                "-o".to_string(),
                "IdentitiesOnly=yes".to_string(),
            ],
            None => Vec::new(),
        }
    }

    pub fn valid(&self) -> Result<(), String> {
        valid_target(&self.target)
    }

    /// Everything before the remote command: `-o` options for `use_`, `-p <port>` when not 22 (a
    /// port set in `~/.ssh/config` still applies when there is none here), then the target.
    pub fn argv(&self, use_: Use) -> Vec<String> {
        let mut a: Vec<String> = Vec::new();
        let mut o = |s: &str| a.extend(["-o".to_string(), s.to_string()]);
        match use_ {
            Use::Exec => {
                o("BatchMode=yes");
                o("ConnectTimeout=15");
                o("ServerAliveInterval=15");
                o("ServerAliveCountMax=4");
                // Windows OpenSSH has no usable connection sharing (no unix-socket mux): without
                // these every call is its own handshake, which is slow but works.
                if multiplexing() {
                    o("ControlMaster=auto");
                    o("ControlPath=~/.ssh/atlas-%C");
                    o("ControlPersist=120");
                }
            }
            Use::Tunnel => {
                o("BatchMode=yes");
                o("ConnectTimeout=10");
                o("ServerAliveInterval=15");
                o("ExitOnForwardFailure=yes");
                o("StreamLocalBindUnlink=yes");
            }
            Use::Terminal => {}
        }
        a.extend(self.identity_args());
        if let Some(p) = self.port {
            a.extend(["-p".to_string(), p.to_string()]);
        }
        if use_ == Use::Tunnel {
            a.push("-N".to_string());
        }
        if use_ == Use::Terminal {
            a.push("-t".to_string());
        }
        a.push(self.target.clone());
        a
    }

    /// The whole argv after `ssh` for running `script` (with the agents' `PATH`) on the server.
    pub fn exec_argv(&self, script: &str) -> Vec<String> {
        let mut a = self.argv(Use::Exec);
        a.push("--".to_string());
        a.push(remote_command(&with_path(script)));
        a
    }

    /// `ssh -N -L <forward> <target>` argv (after `ssh`): a tunnel to a socket or port on the server.
    pub fn tunnel_argv(&self, forward: &str) -> Vec<String> {
        let mut a = self.argv(Use::Tunnel);
        let target = a.pop().expect("the target is last");
        a.extend(["-L".to_string(), forward.to_string(), target]);
        a
    }

    /// argv (after `ssh`) for a read-only probe: the options of [`Use::Exec`] and `script` as given,
    /// for hosts that are not crew servers (no `PATH` export, no `bash -c`).
    pub fn raw_argv(&self, script: &str) -> Vec<String> {
        let mut a = self.argv(Use::Exec);
        a.push(script.to_string());
        a
    }

    /// argv (after `ssh`) for the first contact with a server: `true` through the options of [`Use::Exec`], and a
    /// host key ssh has not seen yet is recorded (`StrictHostKeyChecking=accept-new`; a key that CHANGED is still
    /// refused). Under `BatchMode` ssh cannot ask "are you sure you want to continue connecting?", so without
    /// this a server nobody ever connected to fails with "Host key verification failed" on the very first call.
    pub fn first_contact_argv(&self) -> Vec<String> {
        let mut a = self.argv(Use::Exec);
        let target = a.pop().expect("the target is last");
        a.extend([
            "-o".to_string(),
            "StrictHostKeyChecking=accept-new".to_string(),
            target,
            "true".to_string(),
        ]);
        a
    }

    /// `scp` argv (after `scp`) copying `src` to `<target>:<dest>`.
    pub fn scp_argv(&self, src: &std::path::Path, dest: &str) -> Vec<String> {
        let mut a = vec![
            "-q".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
        ];
        a.extend(self.identity_args());
        if let Some(p) = self.port {
            a.extend(["-P".to_string(), p.to_string()]);
        }
        a.push(src.display().to_string());
        a.push(format!("{}:{dest}", self.target));
        a
    }

    /// `ssh -t <target> <command>` for a person to type in (the engine sign-in), with the `PATH`.
    pub fn terminal_argv(&self, command: &str) -> Vec<String> {
        let mut a = vec!["ssh".to_string()];
        a.extend(self.argv(Use::Terminal));
        a.push("--".to_string());
        a.push(remote_command(&with_path(command)));
        a
    }
}

/// Connection sharing is used where `ssh` supports it.
const fn multiplexing() -> bool {
    cfg!(not(windows))
}

/// The one remote-command argument for `script`. ssh joins its command arguments with spaces and
/// the server's shell parses the result, so the script goes in single quotes inside `bash -c`.
/// Windows OpenSSH passes it through `CreateProcess`'s command line, where a double quote needs a
/// backslash the remote shell then sees: on a Windows client [`windows_safe`] leaves none in argv.
pub fn remote_command(script: &str) -> String {
    let script = if cfg!(windows) {
        windows_safe(script)
    } else {
        script.to_string()
    };
    format!("bash -c {}", shq(&script))
}

/// `script` with no `"` in it: the script travels with every `"` and `\` in octal and the remote
/// bash gets the original text back from `printf %b` (the same bytes, run by a nested bash).
/// Used on a Windows client only (see [`remote_command`]).
pub fn windows_safe(script: &str) -> String {
    if !script.contains('"') {
        return script.to_string();
    }
    let escaped = script.replace('\\', r"\134").replace('"', r"\042");
    format!("printf %b {} | bash", shq(&escaped))
}

/// The key this computer signs in to a crew server with: the private key file `ssh` offers, and its `.pub` when
/// that file exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalKey {
    /// The private key file (what `-i` takes, and what the connect form shows).
    pub identity: PathBuf,
    /// `<identity>.pub` when it exists; without it [`LocalKey::public_text`] derives the public key.
    pub public: Option<PathBuf>,
}

impl LocalKey {
    /// The one-line public key to put in the server's `authorized_keys`: the `.pub` file, else derived from the
    /// private key with `ssh-keygen -y` (a key downloaded without its `.pub`). A passphrase-protected key without
    /// a `.pub` cannot be derived without asking, so that is an error saying so.
    pub fn public_text(&self) -> Result<String, String> {
        use crate::sys::CommandExt as _;
        if let Some(p) = &self.public {
            let text =
                std::fs::read_to_string(p).map_err(|e| format!("reading {}: {e}", p.display()))?;
            return Ok(text.trim().to_owned());
        }
        let mut cmd = std::process::Command::new("ssh-keygen");
        cmd.no_window()
            // `-P ""`: never ask for the passphrase (a prompt would hang with no terminal).
            .args(["-y", "-P", "", "-f"])
            .arg(&self.identity)
            .stdin(std::process::Stdio::null());
        let out = cmd.output().map_err(|e| format!("ssh-keygen: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "{} has no .pub next to it and its public key could not be read from it ({}); \
if it has a passphrase, create the .pub with: ssh-keygen -y -f <key> > <key>.pub",
                self.identity.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

/// The key `ssh` would use to reach `target`: the `IdentityFile` given with it, else the first of the ones `ssh -G`
/// lists (`~/.ssh/config` and the default names: id_ed25519, id_ecdsa, id_rsa, …, so a key with another name is
/// found too) that exists. `None` when there is none.
pub fn find_key(target: &SshTarget) -> Option<LocalKey> {
    use crate::sys::CommandExt as _;
    let home = dirs::home_dir()?;
    if let Some(identity) = &target.identity {
        return key_at(identity.clone());
    }
    let mut cmd = std::process::Command::new("ssh");
    cmd.no_window().arg("-G");
    if let Some(p) = target.port {
        cmd.args(["-p", &p.to_string()]);
    }
    cmd.arg(&target.target);
    let listed = cmd
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| key_from_g(&String::from_utf8_lossy(&o.stdout), &home));
    listed.or_else(|| default_key_in(&home))
}

/// The key a person (or Atlas) made with the default names under `<home>/.ssh`. `ssh -G` does not list it when a
/// `~/.ssh/config` names an `IdentityFile` of its own (OpenSSH adds its default names only when none is given), so a
/// key Atlas created for the password sign-in is still found; the caller passes it as `-i`.
pub fn default_key_in(home: &std::path::Path) -> Option<LocalKey> {
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .into_iter()
        .find_map(|n| key_at(home.join(".ssh").join(n)))
}

/// [`find_key`] before there is a server to ask about (onboarding's "do you have a key?"): the keys ssh offers
/// to any host, plus a `Host *` block of `~/.ssh/config`.
pub fn find_default_key() -> Option<LocalKey> {
    find_key(&SshTarget::new("atlas-key-probe", None))
}

/// The key at `identity`, when the private file or its `.pub` is there.
fn key_at(identity: PathBuf) -> Option<LocalKey> {
    let mut p = identity.clone().into_os_string();
    p.push(".pub");
    let public = Some(PathBuf::from(p)).filter(|p| p.is_file());
    (public.is_some() || identity.is_file()).then_some(LocalKey { identity, public })
}

/// The first existing key among the `identityfile` lines of `ssh -G` output (`~/` is `home`).
pub fn key_from_g(g: &str, home: &std::path::Path) -> Option<LocalKey> {
    g.lines()
        .filter_map(|l| l.strip_prefix("identityfile "))
        .map(|p| {
            let p = p.trim();
            match p.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(p),
            }
        })
        .find_map(key_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Crew servers are added as `atlas-crew@host`; the old check rejected every `@`, so
    // `crew_servers_add` could never connect one.
    #[test]
    fn user_at_host_and_plain_aliases_are_accepted() {
        for a in [
            "atlas-crew@myserver.example.com",
            "atlas-crew@10.0.0.5",
            "devbox",
            "my_host-1",
        ] {
            assert!(valid_target(a).is_ok(), "{a}");
        }
    }

    // Anything ssh could parse as an option, or a shell could expand, stays rejected.
    #[test]
    fn option_like_or_malformed_targets_are_rejected() {
        for a in [
            "",
            "-oProxyCommand=x",
            "u@-oProxyCommand=x",
            "-u@host",
            "@host",
            "user@",
            "a@b@c",
            "u@h;rm",
            "u@h x",
            "u@$(id)",
        ] {
            assert!(valid_target(a).is_err(), "{a}");
        }
    }

    // Issue #355: one address has one spelling, so the registry, the argv and the relay agree.
    #[test]
    fn port_22_is_no_port() {
        assert_eq!(SshTarget::new("u@h", Some(22)).port, None);
        assert_eq!(SshTarget::new("u@h", Some(2222)).port, Some(2222));
        assert_eq!(SshTarget::new("u@h", None).port, None);
    }

    fn has_pair(a: &[String], k: &str, v: &str) -> bool {
        a.windows(2).any(|w| w[0] == k && w[1] == v)
    }

    // Issue #355: `-p` is on every kind of call, before the target (after it ssh reads it as part
    // of the remote command), and absent for the default port.
    #[test]
    fn the_port_precedes_the_target_on_every_kind_of_call() {
        for u in [Use::Exec, Use::Tunnel, Use::Terminal] {
            let a = SshTarget::new("atlas-crew@h", Some(2222)).argv(u);
            let p = a.iter().position(|x| x == "-p").unwrap_or(usize::MAX);
            let t = a.iter().position(|x| x == "atlas-crew@h").unwrap();
            assert!(p < t, "{u:?}: {a:?}");
            assert_eq!(a[p + 1], "2222", "{u:?}");
            assert_eq!(a.last().unwrap(), "atlas-crew@h", "{u:?}");

            let none = SshTarget::new("atlas-crew@h", None).argv(u);
            assert!(!none.contains(&"-p".to_string()), "{u:?}: {none:?}");
        }
    }

    #[test]
    fn each_use_gets_its_own_options() {
        let t = SshTarget::new("u@h", None);
        let exec = t.argv(Use::Exec);
        assert!(has_pair(&exec, "-o", "BatchMode=yes"));
        let tun = t.argv(Use::Tunnel);
        assert!(tun.contains(&"-N".to_string()));
        assert!(has_pair(&tun, "-o", "ExitOnForwardFailure=yes"));
        let term = t.argv(Use::Terminal);
        assert!(term.contains(&"-t".to_string()));
        assert!(
            !term.iter().any(|x| x.contains("BatchMode")),
            "a person must be able to answer a prompt: {term:?}"
        );
    }

    // Issue #355 / audit #10: connection sharing only where ssh supports it.
    #[test]
    fn connection_sharing_is_off_on_windows() {
        let a = SshTarget::new("u@h", None).argv(Use::Exec);
        let shares = a.iter().any(|x| x.starts_with("ControlMaster"));
        assert_eq!(shares, cfg!(not(windows)), "{a:?}");
        assert_eq!(a.iter().any(|x| x.starts_with("ControlPath")), shares);
        assert_eq!(a.iter().any(|x| x.starts_with("ControlPersist")), shares);
    }

    // Audit #5: the agents' dirs are first, in the order the desktop's check used.
    #[test]
    fn every_remote_script_exports_the_agents_path_first() {
        let a = SshTarget::new("u@h", Some(2222)).exec_argv("claude auth status");
        let cmd = a.last().unwrap();
        for d in [
            "$HOME/.local/bin",
            "$HOME/.bun/bin",
            "$HOME/.npm-global/bin",
        ] {
            assert!(cmd.contains(d), "{d} missing: {cmd}");
        }
        assert!(cmd.find("export PATH").unwrap() < cmd.find("claude auth").unwrap());
        assert!(cmd.contains("$PATH"), "the old PATH stays at the end");
        assert_eq!(a[a.len() - 2], "--");
    }

    #[test]
    fn a_terminal_runs_the_command_with_the_path_too() {
        let a = SshTarget::new("atlas-crew@h", Some(2222)).terminal_argv("claude");
        assert_eq!(a[0], "ssh");
        assert!(has_pair(&a, "-p", "2222"));
        assert!(a.contains(&"-t".to_string()));
        assert!(a.last().unwrap().contains(".local/bin"));
        assert!(a.last().unwrap().contains("claude"));
    }

    // The remote command is one argv element that a POSIX shell parses back to the script.
    #[cfg(unix)]
    #[test]
    fn the_remote_command_round_trips_quotes_through_bash() {
        let script = r#"printf '%s|%s' "it's" 'a "b" $HOME'"#;
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(remote_command(script))
            .env("HOME", "/h")
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "it's|a \"b\" $HOME");
    }

    // The argv per OS (runs on every host, Windows CI included): a Windows client never puts a `"` in the remote
    // command or the options of connection sharing; the others send the script as written. The agents' PATH line
    // has double quotes, so this also pins that the Windows form carries it without one.
    #[test]
    fn the_remote_argv_is_quote_free_on_windows_and_plain_elsewhere() {
        let t = SshTarget::new("atlas-crew@srv", Some(2222));
        let a = t.exec_argv(r#"printf '%s' "$HOME""#);
        let cmd = a.last().unwrap();
        if cfg!(windows) {
            assert!(!cmd.contains('"'), "{cmd}");
            assert!(cmd.contains("| bash"), "{cmd}");
        } else {
            assert!(cmd.contains(r#"printf '\''%s'\'' "$HOME""#), "{cmd}");
            assert!(cmd.contains(r#"export PATH=""#), "{cmd}");
        }
        // Same either way: the port and the target are separate argv elements, never inside the script.
        assert!(has_pair(&a, "-p", "2222") && a.iter().any(|x| x == "atlas-crew@srv"));
        assert_eq!(a.iter().filter(|x| x.as_str() == "--").count(), 1);
    }

    // Audit #10: a double quote in argv is mangled by Windows OpenSSH's command line; the script a
    // Windows client sends has none, and bash gives the same output.
    #[cfg(unix)]
    #[test]
    fn windows_safe_scripts_carry_no_double_quote_and_mean_the_same() {
        let script = r#"u=$(echo "a b"); printf '%s\n' "$u" "x\y""#;
        let safe = windows_safe(script);
        assert!(!safe.contains('"'), "{safe}");
        let run = |s: &str| {
            let o = std::process::Command::new("bash")
                .arg("-c")
                .arg(s)
                .output()
                .unwrap();
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        assert_eq!(run(&safe), run(script));
        assert_eq!(run(&safe), "a b\nx\\y\n");
        assert_eq!(windows_safe("echo hi"), "echo hi");
    }

    // Audit #10: a key outside id_ed25519/id_rsa is found through the `IdentityFile`s ssh lists.
    #[test]
    fn the_key_ssh_would_use_is_found_by_its_identityfile() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/id_work.pub"), "ssh-ed25519 AAAA w").unwrap();
        std::fs::write(home.join(".ssh/id_rsa.pub"), "ssh-rsa AAAA r").unwrap();
        let g = format!(
            "host h\nidentityfile ~/.ssh/id_missing\nidentityfile {}/.ssh/id_work\nidentityfile ~/.ssh/id_rsa\n",
            home.display()
        );
        let k = key_from_g(&g, home).unwrap();
        assert_eq!(
            k.identity,
            home.join(".ssh/id_work"),
            "the first listed key that exists wins"
        );
        assert_eq!(k.public, Some(home.join(".ssh/id_work.pub")));
        assert_eq!(key_from_g("host h\n", home), None);
    }

    // #517: a key someone downloaded has no `.pub`. The form still shows it (the private file is what ssh uses),
    // and the public key is derived from it instead of "no SSH public key found".
    #[test]
    fn a_private_key_without_its_pub_is_found_and_its_public_key_is_derived() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let key = home.join("id_dl");
        let st = std::process::Command::new("ssh-keygen")
            .args(["-t", "ed25519", "-N", "", "-q", "-C", "dl", "-f"])
            .arg(&key)
            .status()
            .expect("ssh-keygen");
        assert!(st.success());
        let want = std::fs::read_to_string(home.join("id_dl.pub")).unwrap();
        std::fs::remove_file(home.join("id_dl.pub")).unwrap();

        let g = format!("identityfile {}\n", key.display());
        let k = key_from_g(&g, home).unwrap();
        assert_eq!(
            (k.identity.as_path(), k.public.as_ref()),
            (key.as_path(), None)
        );
        let derived = k.public_text().unwrap();
        // The comment is not stored in the private key: compare the algorithm and the key blob.
        let blob = |s: &str| s.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
        assert_eq!(blob(&derived), blob(&want));
    }

    // #517: a config with its own `IdentityFile` hides the default names from `ssh -G`; the key Atlas created at
    // ~/.ssh/id_ed25519 for the password sign-in must still be found (it then travels as `-i`).
    #[test]
    fn a_default_named_key_is_found_even_when_ssh_does_not_list_it() {
        let dir = tempfile::tempdir().unwrap();
        let ssh = dir.path().join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        assert_eq!(default_key_in(dir.path()), None);
        std::fs::write(ssh.join("id_rsa"), "x").unwrap();
        std::fs::write(ssh.join("id_ed25519"), "x").unwrap();
        std::fs::write(ssh.join("id_ed25519.pub"), "ssh-ed25519 AAAA a").unwrap();
        let k = default_key_in(dir.path()).unwrap();
        assert_eq!(k.identity, ssh.join("id_ed25519"), "ed25519 before rsa");
        assert_eq!(k.public, Some(ssh.join("id_ed25519.pub")));
    }

    // #517: a key the user picked is used as given. A chosen key never falls back to another one: it either exists
    // or there is no key.
    #[test]
    fn a_picked_key_is_the_key_and_a_missing_one_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("picked");
        std::fs::write(&key, "private").unwrap();
        std::fs::write(dir.path().join("picked.pub"), "ssh-ed25519 AAAA p\n").unwrap();
        let t = SshTarget::new("u@h", None).with_identity(Some(key.clone()));
        let k = find_key(&t).unwrap();
        assert_eq!(k.identity, key);
        assert_eq!(k.public_text().unwrap(), "ssh-ed25519 AAAA p");
        let gone = SshTarget::new("u@h", None).with_identity(Some(dir.path().join("nope")));
        assert_eq!(find_key(&gone), None);
    }

    // #517: a picked key reaches EVERY ssh call of the target (exec, tunnel, terminal, scp), before the target,
    // with `IdentitiesOnly` so no other key is offered first; no key picked adds nothing.
    #[test]
    fn a_picked_key_is_on_every_kind_of_call_before_the_target() {
        let t = SshTarget::new("atlas-crew@h", Some(2222)).with_identity(Some("/k/id work"));
        for u in [Use::Exec, Use::Tunnel, Use::Terminal] {
            let a = t.argv(u);
            let i = a.iter().position(|x| x == "-i").unwrap_or(usize::MAX);
            assert_eq!(a[i + 1], "/k/id work", "{u:?}: {a:?}");
            assert!(has_pair(&a, "-o", "IdentitiesOnly=yes"), "{u:?}: {a:?}");
            assert!(
                i < a.iter().position(|x| x == "atlas-crew@h").unwrap(),
                "{u:?}"
            );
            assert_eq!(a.last().unwrap(), "atlas-crew@h");
            assert!(
                !SshTarget::new("atlas-crew@h", None)
                    .argv(u)
                    .contains(&"-i".to_string())
            );
        }
        assert!(t.tunnel_argv("/a:/b").contains(&"-i".to_string()));
        assert!(t.exec_argv("true").contains(&"-i".to_string()));
        assert!(t.raw_argv("true").contains(&"-i".to_string()));
        let scp = t.scp_argv(std::path::Path::new("/a"), "d");
        assert!(has_pair(&scp, "-i", "/k/id work"), "{scp:?}");
        let plain = SshTarget::new("h", None).scp_argv(std::path::Path::new("/a"), "d");
        assert!(!plain.contains(&"-i".to_string()));
    }

    // The forward belongs before the target (after it ssh reads it as the remote command), and the port is
    // there too; scp spells the port `-P`, not `-p` (`-p` keeps file times), a classic way to drop it.
    #[test]
    fn tunnels_and_copies_carry_the_port_in_their_own_spelling() {
        let t = SshTarget::new("atlas-crew@h", Some(2222));
        let tun = t.tunnel_argv("/tmp/s.sock:/home/u/.atlas/hostd.sock");
        let l = tun.iter().position(|x| x == "-L").unwrap();
        assert_eq!(tun[l + 1], "/tmp/s.sock:/home/u/.atlas/hostd.sock");
        assert_eq!(tun.last().unwrap(), "atlas-crew@h");
        assert!(l < tun.len() - 1 && has_pair(&tun, "-p", "2222"));
        assert!(tun.contains(&"-N".to_string()));

        let scp = t.scp_argv(std::path::Path::new("/a/atlas"), ".atlas/bin/atlas");
        assert!(has_pair(&scp, "-P", "2222"), "{scp:?}");
        assert!(!scp.contains(&"-p".to_string()), "{scp:?}");
        assert_eq!(scp.last().unwrap(), "atlas-crew@h:.atlas/bin/atlas");
        let plain = SshTarget::new("h", None).scp_argv(std::path::Path::new("/a"), "d");
        assert!(!plain.contains(&"-P".to_string()));
    }

    // A host that is not a crew server keeps its script untouched: no PATH export, no `bash -c`.
    #[test]
    fn a_raw_probe_is_the_script_as_given() {
        let a = SshTarget::new("devbox", None).raw_argv("uname -m");
        assert_eq!(a.last().unwrap(), "uname -m");
        assert!(!a.iter().any(|x| x.contains("PATH")));
    }

    // #517: a server nobody connected to has no host key in known_hosts, and `BatchMode` cannot ask about it, so
    // the first call of a first-time user failed with "Host key verification failed". The first contact records
    // the key (accept-new), keeps every option of an exec call (port, key) and runs `true`.
    #[test]
    fn the_first_contact_records_an_unknown_host_key_and_keeps_the_exec_options() {
        let a = SshTarget::new("u@h", Some(2222))
            .with_identity(Some("/k/id"))
            .first_contact_argv();
        assert!(
            has_pair(&a, "-o", "StrictHostKeyChecking=accept-new"),
            "{a:?}"
        );
        assert!(
            has_pair(&a, "-o", "BatchMode=yes")
                && has_pair(&a, "-p", "2222")
                && has_pair(&a, "-i", "/k/id")
        );
        assert_eq!(&a[a.len() - 2..], ["u@h", "true"]);
        let plain = SshTarget::new("u@h", None).raw_argv("true");
        assert!(
            !plain.iter().any(|x| x.contains("StrictHostKeyChecking")),
            "only the first contact trusts a new key"
        );
    }
}
