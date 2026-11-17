//! `atlas crew install` / `uninstall` end to end: the real `atlas` binary, a fake `ssh` on PATH that runs the
//! script it is given on a simulated server (a temp root with fake `sudo`, `systemctl`, `id`, ... and the server's
//! absolute paths rewritten into the root). Nothing of atlas is mocked: the scripts, the exit-code mapping and the
//! messages are the ones a person gets. Unix only (the fake ssh is a shell script).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct World {
    tmp: tempfile::TempDir,
}

/// A server's `curl` for the vendor installers of the engines (`curl -fsSL <url> | sh`, issue #200): prints an
/// installer that behaves like the vendor's real one (read from claude.ai/install.sh, opencode.ai/install and
/// omp.sh/install on 2026-11-15), so a green test means the install works against what the vendors ship:
/// - claude installs to `~/.local/bin` and refuses to run under sudo (`SUDO_USER` set to a non-root user)
///   unless `CLAUDE_INSTALL_ALLOW_SUDO` is set;
/// - opencode installs to `~/.opencode/bin`;
/// - omp installs to `${PI_INSTALL_DIR:-~/.local/bin}`.
///
/// Any other URL succeeds silently.
const VENDOR_INSTALLER_CURL: &str = concat!(
    "for a; do url=$a; done\n",
    "case \"$url\" in\n",
    "  *claude.ai*) b=claude; d='$HOME/.local/bin'\n",
    "    echo 'if [ \"$(id -u)\" -eq 0 ] && [ -n \"${SUDO_USER:-}\" ] && [ \"$SUDO_USER\" != root ] && [ -z \"${CLAUDE_INSTALL_ALLOW_SUDO:-}\" ]; then echo \"Error: do not run this installer with sudo.\" >&2; exit 1; fi';;\n",
    "  *opencode.ai*) b=opencode; d='$HOME/.opencode/bin';;\n",
    "  *omp.sh*) b=omp; d='${PI_INSTALL_DIR:-$HOME/.local/bin}';;\n",
    "  *) exit 0;;\n",
    "esac\n",
    "echo \"mkdir -p \\\"$d\\\"; printf '#!/bin/sh\\\\n' > \\\"$d/$b\\\"; chmod +x \\\"$d/$b\\\"\"\n",
);

const FAKE_SUDO: &str = r#"
while [ $# -gt 0 ]; do
  case "$1" in
    -n) nopw=1; shift;; -S) stdin_pw=1; shift;; -A) askpass=1; shift;;
    -p) shift 2;; -v) validate=1; shift;; -u) shift 2;; --) shift; break;; -*) shift;; *) break;;
  esac
done
if [ -f "$ROOT/sudo-password" ]; then
  if [ -n "$nopw" ]; then echo 'sudo: a password is required' >&2; exit 1; fi
  if [ -n "$askpass" ] && [ -n "$SUDO_ASKPASS" ]; then given=$("$SUDO_ASKPASS")
  elif [ "$stdin_pw" = 1 ]; then IFS= read -r given
  else echo 'sudo: a terminal is required to read the password' >&2; exit 1; fi
  if [ "$given" != "$(cat "$ROOT/sudo-password")" ]; then echo 'sudo: incorrect password attempt' >&2; exit 1; fi
  echo "authenticated" >> "$ROOT/log/auth"
fi
echo "sudo $*" >> "$ROOT/log/sudo"
[ -n "$validate" ] && exit 0
# Like the real sudo: the command runs as root and sees who invoked it (the admin).
export SUDO_USER=admin SIM_UID=0
exec "$@"
"#;

impl World {
    fn new() -> Self {
        let w = Self {
            tmp: tempfile::tempdir().unwrap(),
        };
        for d in [
            "bin",
            "lib/atlas-crew",
            "etc/systemd/system",
            "var/lib",
            "log",
            "run/systemd/system",
            // systemd makes it when the helper's socket unit starts (the fake `systemctl` starts nothing).
            "run/atlas-crew",
            "home/.ssh",
        ] {
            std::fs::create_dir_all(w.root().join(d)).unwrap();
        }
        std::fs::write(
            w.root().join("home/.ssh/id_ed25519.pub"),
            "ssh-ed25519 AAAA test@test\n",
        )
        .unwrap();
        for t in [
            "bash",
            "sh",
            "cat",
            "cp",
            "mv",
            "rm",
            "mkdir",
            "chmod",
            "cmp",
            "sed",
            "head",
            "tr",
            "sleep",
            "printf",
            "test",
            "env",
            "tee",
            "mktemp",
            "tar",
            "sha256sum",
            "shasum",
            "awk",
            "dirname",
            "ls",
            "grep",
            "true",
            "false",
            "printenv",
            "ln",
            "uname",
            "basename",
            "cut",
            "dirname",
            "readlink",
        ] {
            w.link_real(t);
        }
        w.fake("uname", "echo 'Linux x86_64'");
        w.fake("sudo", FAKE_SUDO);
        // The unit "runs" the installed binary: restart succeeds, hostd answers when the binary says so.
        // A unit whose file is a symlink to /dev/null is masked, as in systemd: `is-enabled` says so, `unmask` removes
        // the link (unless the test sets $ROOT/unmask-fails).
        w.fake(
            "systemctl",
            r#"echo "systemctl $*" >> "$ROOT/log/systemctl"
f="$ROOT/etc/systemd/system/$2"
masked() { [ -L "$f" ] && [ "$(readlink "$f")" = /dev/null ]; }
case "$1" in
  is-enabled) if masked; then echo masked; exit 1; fi;;
  unmask) if [ -e "$ROOT/unmask-fails" ]; then echo 'Access denied' >&2; exit 1; fi; masked && rm -f "$f";;
  enable) if masked; then echo "Failed to enable unit: Unit $f is masked" >&2; exit 1; fi;;
esac
exit 0"#,
        );
        w.fake(
            "id",
            "[ \"$1\" = -u ] && { echo \"${SIM_UID:-1000}\"; exit 0; }\n[ -d \"$ROOT/var/lib/$1\" ] || [ -f \"$ROOT/users/$1\" ]",
        );
        w.fake(
            "useradd",
            "for a; do u=$a; done; mkdir -p \"$ROOT/var/lib/$u\"; echo \"useradd $*\" >> \"$ROOT/log/useradd\"",
        );
        // The simulated server's users exist only under $ROOT, so the real chown would refuse them (it did on
        // Linux CI; on macOS `chown` is in /usr/sbin and used to fall back to a no-op fake). Like the real one,
        // it refuses an owner that was never created, so a chown before useradd still fails.
        w.fake(
            "chown",
            "[ \"$1\" = -R ] && shift; u=${1%%:*}; shift; [ -d \"$ROOT/var/lib/$u\" ] || [ -f \"$ROOT/users/$u\" ] || { echo \"chown: invalid user: '$u'\" >&2; exit 1; }; echo \"chown $u $*\" >> \"$ROOT/log/chown\"",
        );
        // The isolation steps (#200) run through the same simulated server. Deterministic like chown and
        // useradd: each refuses what the simulation did not create, and records what it was asked.
        // `install -d -m MODE -o OWNER -g GROUP PATH...` / `install -m MODE -o O -g G SRC DST`: the real one
        // would need root to chown, and refuses an owner that does not exist.
        w.fake(
            "install",
            r#"dir=; files=
while [ $# -gt 0 ]; do
  case "$1" in
    -d) dir=1; shift;; -m|-g) shift 2;;
    -o) u=$2; shift 2
        [ -d "$ROOT/var/lib/$u" ] || [ -f "$ROOT/users/$u" ] || [ "$u" = root ] || { echo "install: invalid user '$u'" >&2; exit 1; };;
    *) files="$files $1"; shift;;
  esac
done
echo "install${dir:+ -d}$files" >> "$ROOT/log/install"
if [ -n "$dir" ]; then for d in $files; do mkdir -p "$d"; done
else set -- $files; cp "$1" "$2"; fi"#,
        );
        // `usermod -aG GROUP USER` for the crew user: both must exist (the group is the crewmate user's).
        w.fake(
            "usermod",
            r#"u=; for a; do u=$a; done; [ -d "$ROOT/var/lib/$u" ] || [ -f "$ROOT/users/$u" ] || { echo "usermod: user '$u' does not exist" >&2; exit 6; }; echo "usermod $*" >> "$ROOT/log/usermod""#,
        );
        for t in ["git", "pgrep"] {
            w.fake(t, "exit 0");
        }
        // The vendor installers (`curl -fsSL <url> | sh`) are an installer script on stdout that leaves the
        // agent's binary in `~/.local/bin`, named after the URL's vendor. Release downloads (`releases/download`)
        // are what the install tests of the binary use: those keep failing the way they did, here as a no-op.
        w.fake("curl", VENDOR_INSTALLER_CURL);
        // The engine installers: the vendor line runs under the simulated engine home and leaves the binary
        // where that vendor puts it (`~/.npm-global/bin` for npm), which is what the step then looks for.
        w.fake(
            "npm",
            r#"mkdir -p "$npm_config_prefix/bin"; printf '#!/bin/sh\n' > "$npm_config_prefix/bin/codex"; chmod +x "$npm_config_prefix/bin/codex""#,
        );
        // The fake ssh: run the last argument (`bash -c '<script>'`) on the simulated server.
        w.write_exec(
            "bin-ssh/ssh",
                "#!/bin/sh\n[ \"$1\" = -G ] && { if [ -f \"$ROOT/identity\" ]; then echo \"identityfile $(cat \"$ROOT/identity\")\"; else echo 'identityfile ~/.ssh/id_ed25519'; fi; exit 0; }\nfor a; do last=$a; done\n[ -f \"$ROOT/ssh-down\" ] && { echo 'ssh: connect refused' >&2; exit 255; }\nR=\"$ROOT\"\nexport PATH=\"$R/bin\"\nscript=$(printf '%s' \"$last\" | sed -e \"s#/usr/local/lib/atlas-crew#$R/lib/atlas-crew#g\" -e \"s#/usr/local/lib/atlas-bot#$R/lib/atlas-bot#g\" -e \"s#/etc/systemd/system#$R/etc/systemd/system#g\" -e \"s#/etc/atlas-crew#$R/etc/atlas-crew#g\" -e \"s#/run/atlas-crew#$R/run/atlas-crew#g\" -e \"s#/run/systemd/system#$R/run/systemd/system#g\" -e \"s#/var/lib#$R/var/lib#g\")\nif [ -f \"$R/ssh-drop-after\" ] && printf '%s' \"$script\" | grep -q \"$(cat \"$R/ssh-drop-after\")\"; then\n  /bin/sh -c \"$script\"; echo run >> \"$R/log/dropped-runs\"; echo 'ssh: connection reset' >&2; exit 255\nfi\nexec /bin/sh -c \"$script\"\n"
        );
        w
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn link_real(&self, name: &str) {
        // sbin too: macOS keeps some of these there (chown), Linux in /usr/bin. A tool found on one OS and
        // faked on the other made these tests pass on macOS by accident and fail on Linux.
        for dir in [
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
            "/usr/local/bin",
            "/opt/homebrew/bin",
        ] {
            let src = Path::new(dir).join(name);
            if src.exists() {
                let _ = std::os::unix::fs::symlink(&src, self.root().join("bin").join(name));
                return;
            }
        }
        self.fake(name, "exit 0");
    }

    fn write_exec(&self, rel: &str, body: &str) {
        let p = self.root().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let _ = std::fs::remove_file(&p);
        std::fs::write(&p, body).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn fake(&self, name: &str, body: &str) {
        self.write_exec(&format!("bin/{name}"), &format!("#!/bin/sh\n{body}\n"));
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root().join(rel)).unwrap_or_default()
    }

    fn exists(&self, rel: &str) -> bool {
        self.root().join(rel).exists()
    }

    /// A local `atlas` for `--binary`: answers `hostd --status` like a healthy hostd, or never.
    fn atlas_file(&self, version: &str, healthy: bool) -> PathBuf {
        let status = if healthy {
            format!("echo '{{\"version\": \"{version}\"}}'")
        } else {
            "exit 1".to_owned()
        };
        let rel = format!("local-atlas-{version}-{healthy}");
        self.write_exec(
            &rel,
            &format!("#!/bin/sh\n# {version}\ncase \"$*\" in *--status*) {status};; esac\n"),
        );
        self.root().join(rel)
    }

    fn atlas(&self, args: &[&str], stdin: &str) -> Output {
        self.atlas_in("en", args, stdin)
    }

    /// [`World::atlas`] with `lang` as the display language (`ATLAS_LANG`).
    fn atlas_in(&self, lang: &str, args: &[&str], stdin: &str) -> Output {
        let path = format!(
            "{}:{}",
            self.root().join("bin-ssh").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
            .arg("crew")
            .args(args)
            .env("PATH", path)
            .env("ATLAS_LANG", lang)
            .env("HOME", self.root().join("home"))
            .env("ROOT", self.root())
            .env_remove("ATLAS_CREW_BINARY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
        }
        child.wait_with_output().unwrap()
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// The events on stdout of `--progress json`: every line must parse, in the documented shape.
fn events(o: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {l}"))
        })
        .collect()
}

// The machine channel (the human lines are for people and get translated; a program reads this).
// Desktop reads `kind`/`code` only, so a client never matches on text and a translation can never break it.
#[test]
fn progress_json_prints_only_events_on_stdout_and_ends_with_done() {
    let w = World::new();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &[
            "install",
            "me@srv",
            "--binary",
            bin.to_str().unwrap(),
            "--progress",
            "json",
        ],
        "",
    );
    assert!(o.status.success(), "{}", text(&o));
    let ev = events(&o);
    let codes: Vec<&str> = ev.iter().map(|e| e["code"].as_str().unwrap()).collect();
    assert_eq!(
        codes,
        [
            "check",
            "install",
            "user",
            "crew.user",
            "service",
            "ssh",
            "done"
        ]
    );
    assert!(ev.iter().all(|e| e["v"] == 1 && e["args"].is_object()));
    let user = ev.iter().find(|e| e["code"] == "crew.user").unwrap();
    assert_eq!(
        (user["kind"].as_str(), user["args"]["user"].as_str()),
        (Some("info"), Some("atlas-crew"))
    );
    // The human lines moved to stderr, they did not vanish.
    assert!(String::from_utf8_lossy(&o.stderr).contains("Crew server ready"));
    // The default is unchanged for old clients: `step:` markers.
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("step: check"),
        "{}",
        text(&o)
    );
}

#[test]
fn progress_json_failures_carry_a_stable_code_the_client_branches_on() {
    let bin_of = |w: &World| w.atlas_file("9.9.9", true);
    let last = |o: &Output| -> serde_json::Value { events(o).pop().expect("an error event") };

    // A wrong sudo password: the client asks for it again.
    let w = World::new();
    std::fs::write(w.root().join("sudo-password"), "hunter2\n").unwrap();
    let o = w.atlas(
        &[
            "install",
            "me@srv",
            "--sudo-password-stdin",
            "--binary",
            bin_of(&w).to_str().unwrap(),
            "--progress",
            "json",
        ],
        "nope\n",
    );
    let e = last(&o);
    assert_eq!(
        (e["kind"].as_str(), e["code"].as_str()),
        (Some("error"), Some("sudo.password_rejected"))
    );
    assert_eq!(e["args"]["target"], "me@srv");

    // A server that wants a password and none was given: the client shows the password field.
    let w = World::new();
    std::fs::write(w.root().join("sudo-password"), "hunter2\n").unwrap();
    let o = w.atlas(
        &[
            "install",
            "me@srv",
            "--binary",
            bin_of(&w).to_str().unwrap(),
            "--progress",
            "json",
        ],
        "",
    );
    assert_eq!(last(&o)["code"], "sudo.password_required");

    // ssh itself down: not "the host lacks systemd".
    let w = World::new();
    std::fs::write(w.root().join("ssh-down"), "").unwrap();
    let o = w.atlas(
        &[
            "install",
            "me@srv",
            "--binary",
            bin_of(&w).to_str().unwrap(),
            "--progress",
            "json",
        ],
        "",
    );
    assert_eq!(last(&o)["code"], "ssh.unreachable");
}

// The point of the channel: the same bytes in every language, whatever the human lines say.
#[test]
fn progress_json_is_byte_identical_under_en_and_pt_br() {
    for (name, setup) in [("success", false), ("sudo rejected", true)] {
        let run = |lang: &str| {
            let w = World::new();
            if setup {
                std::fs::write(w.root().join("sudo-password"), "hunter2\n").unwrap();
            }
            let bin = w.atlas_file("9.9.9", true);
            let mut args = vec![
                "install",
                "me@srv",
                "--binary",
                bin.to_str().unwrap(),
                "--progress",
                "json",
            ];
            if setup {
                args.insert(2, "--sudo-password-stdin");
            }
            let o = w.atlas_in(lang, &args, if setup { "nope\n" } else { "" });
            String::from_utf8_lossy(&o.stdout).replace(w.root().to_str().unwrap(), "<root>")
        };
        let (en, pt) = (run("en"), run("pt-BR"));
        assert!(!en.is_empty(), "{name}: no events");
        assert_eq!(en, pt, "{name}: events differ between en and pt-BR");
    }
}

// Gap 2: install reported "ready" without checking hostd. A binary that never answers must fail, point at
// `atlas crew logs`, and never print the ready line.
#[test]
fn install_of_a_binary_that_never_answers_fails_and_points_at_the_logs() {
    let w = World::new();
    let bin = w.atlas_file("9.9.9", false);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("atlas crew logs me@srv"), "{t}");
    assert!(!t.contains("Crew server ready"), "{t}");
}

#[test]
fn install_of_a_healthy_binary_reports_ready_and_leaves_it_in_place() {
    let w = World::new();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("Crew server ready"), "{t}");
    assert!(w.read("lib/atlas-crew/atlas").contains("9.9.9"));
    assert!(
        w.read("var/lib/atlas-crew/.ssh/authorized_keys")
            .contains("ssh-ed25519 AAAA")
    );
}

// #223 review: the engine installers are the vendors' real behaviour (claude refuses sudo unless told it is
// fine, opencode writes to ~/.opencode/bin), so every engine must end up linked for the crewmate users,
// pointing into the vendor's own directory under the shared engine home.
#[test]
fn install_links_every_engine_from_where_its_vendor_put_it() {
    let w = World::new();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    let home = w.root().join("lib/atlas-crew/engine-home");
    for (engine, dir) in [
        ("claude", ".local/bin"),
        ("omp", ".local/bin"),
        ("opencode", ".opencode/bin"),
        ("codex", ".npm-global/bin"),
    ] {
        let link = w.root().join("lib/atlas-crew/engines").join(engine);
        let target =
            std::fs::read_link(&link).unwrap_or_else(|e| panic!("{engine} not linked: {e}\n{t}"));
        assert_eq!(target, home.join(dir).join(engine), "{engine}");
    }
}

// Incident (found by the real-systemd end-to-end test, scripts/crew-e2e.sh, on a plain Ubuntu 24.04): the codex
// engine is installed with npm, which a fresh VPS does not have. `atlas crew install` failed there with
// `npm: not found`, after it had written the unit and the isolation helpers and before it started hostd:
// a half-installed server, over an engine nobody asked for. An engine that cannot be installed is a warning
// that says which one and how to install it; the server and the other engines are installed.
#[test]
fn install_on_a_server_without_npm_installs_the_server_and_every_other_engine() {
    let w = World::new();
    std::fs::remove_file(w.root().join("bin/npm")).unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("Crew server ready"), "{t}");
    assert!(
        t.contains("codex") && t.contains("npm install -g @openai/codex"),
        "the warning names the engine and how to install it: {t}"
    );
    assert!(
        w.read("lib/atlas-crew/atlas").contains("9.9.9"),
        "hostd was swapped in"
    );
    assert!(
        w.exists("var/lib/atlas-crew/.ssh/authorized_keys"),
        "the install ran to the end"
    );
    let engines = w.root().join("lib/atlas-crew/engines");
    for engine in ["claude", "omp", "opencode"] {
        assert!(engines.join(engine).is_symlink(), "{engine} is linked: {t}");
    }
    assert!(
        !engines.join("codex").exists(),
        "no link to an engine that was not installed"
    );
}

// Same class: a vendor installer that leaves no binary (its site is down, a proxy eats the script) must not
// fail the install of the server either, and a later `atlas crew install` retries it.
#[test]
fn install_when_one_vendor_installer_leaves_nothing_still_installs_the_server() {
    let w = World::new();
    w.fake(
        "curl",
        &format!("for a; do url=$a; done\ncase \"$url\" in *claude.ai*) exit 22;; esac\n{VENDOR_INSTALLER_CURL}"),
    );
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("Crew server ready"), "{t}");
    assert!(t.contains("claude"), "the warning names the engine: {t}");
    let engines = w.root().join("lib/atlas-crew/engines");
    assert!(!engines.join("claude").exists(), "{t}");
    for engine in ["omp", "opencode", "codex"] {
        assert!(engines.join(engine).is_symlink(), "{engine} is linked: {t}");
    }
}

// Gap 3: a reinstall over a working server with a dead binary restores the old one.
#[test]
fn reinstall_with_a_dead_binary_restores_the_working_one() {
    let w = World::new();
    let good = w.atlas_file("1.0.0", true);
    assert!(
        w.atlas(
            &["install", "me@srv", "--binary", good.to_str().unwrap()],
            ""
        )
        .status
        .success()
    );
    let dead = w.atlas_file("2.0.0", false);
    let o = w.atlas(
        &["install", "me@srv", "--binary", dead.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("previous binary was restored"), "{t}");
    assert!(
        w.read("lib/atlas-crew/atlas").contains("1.0.0"),
        "the old binary is back"
    );
}

// Gap 7: every missing tool at once, with a package hint, and nothing was changed.
#[test]
fn install_on_a_host_missing_tools_lists_them_all_and_changes_nothing() {
    let w = World::new();
    for t in ["curl", "git", "tar"] {
        let _ = std::fs::remove_file(w.root().join("bin").join(t));
    }
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    for want in ["curl", "git", "tar", "apt-get install"] {
        assert!(t.contains(want), "{want}: {t}");
    }
    assert!(t.contains("nothing was changed"), "{t}");
    assert!(!w.exists("lib/atlas-crew/atlas.new") && !w.exists("lib/atlas-crew/atlas"));
    assert!(!w.exists("log/sudo"), "no privileged step ran");
}

#[test]
fn install_on_a_host_without_running_systemd_is_refused_before_any_change() {
    let w = World::new();
    std::fs::remove_dir_all(w.root().join("run/systemd/system")).unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("systemd is not running as PID 1"), "{t}");
    assert!(!w.exists("log/sudo"));
}

// An unreachable ssh is not "this host lacks systemd".
#[test]
fn install_over_a_dead_ssh_says_so_not_that_the_host_lacks_tools() {
    let w = World::new();
    std::fs::write(w.root().join("ssh-down"), "").unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success());
    assert!(t.contains("could not connect to me@srv over SSH"), "{t}");
}

// Gap 9: password sudo (Atlas Desktop sends it on stdin). Every privileged call must authenticate itself.
#[test]
fn install_with_a_sudo_password_authenticates_every_privileged_call() {
    let w = World::new();
    std::fs::write(w.root().join("sudo-password"), "hunter2\n").unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &[
            "install",
            "me@srv",
            "--sudo-password-stdin",
            "--binary",
            bin.to_str().unwrap(),
        ],
        "hunter2\n",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("Crew server ready"), "{t}");
    // Every sudo that ran was authenticated by itself (the fake refuses a cached credential).
    let sudo_calls = w.read("log/sudo").lines().count();
    let auths = w.read("log/auth").lines().count();
    assert!(sudo_calls >= 8, "{sudo_calls} privileged calls");
    assert_eq!(sudo_calls, auths, "each call authenticated");
    // The binary the upload carried is intact (the password line did not eat or shift it).
    assert!(w.read("lib/atlas-crew/atlas").contains("9.9.9"));
}

#[test]
fn a_wrong_sudo_password_stops_the_install_before_any_change() {
    let w = World::new();
    std::fs::write(w.root().join("sudo-password"), "hunter2\n").unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &[
            "install",
            "me@srv",
            "--sudo-password-stdin",
            "--binary",
            bin.to_str().unwrap(),
        ],
        "nope\n",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("sudo password rejected"), "{t}");
    assert!(!w.exists("lib/atlas-crew/atlas"));
}

// Gap 13: uninstall removes nothing while the crew user is busy.
#[test]
fn uninstall_with_a_busy_crew_user_removes_nothing_and_says_who() {
    let w = World::new();
    let bin = w.atlas_file("9.9.9", true);
    assert!(
        w.atlas(
            &["install", "me@srv", "--binary", bin.to_str().unwrap()],
            ""
        )
        .status
        .success()
    );
    // Someone is logged in as the crew user: a process that is not the unit's.
    w.fake("pgrep", "echo 4242");
    let o = w.atlas(&["uninstall", "me@srv", "--yes"], "");
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("atlas-crew is still in use"), "{t}");
    assert!(w.exists("lib/atlas-crew/atlas"), "binary removed");
    assert!(
        w.exists("etc/systemd/system/atlas-crew.service"),
        "unit removed"
    );
}

impl World {
    /// The server cannot reach GitHub (offline / proxied): its curl always fails. This machine can: a `curl` on the
    /// client's PATH serves `atlas-<ver>-x86_64-unknown-linux-gnu.tar.gz` (a real tarball with a real `atlas`
    /// inside) and its `.sha256` for the release of this CLI, and logs every request.
    fn offline_server_online_client(&self, hostd_version: &str, corrupt_sum: bool) {
        self.fake(
            "curl",
            // Only the release download is unreachable (GitHub, behind a proxy the server lacks); the vendor
            // installers of the engines (#200) are other hosts, and are not what this test is about.
            &format!(
                "case \"$*\" in *releases/download*) echo \"server curl $*\" >> \"$ROOT/log/server-curl\"; echo 'curl: (6) Could not resolve host' >&2; exit 6;; esac\n{VENDOR_INSTALLER_CURL}"
            ),
        );
        let ver = env!("CARGO_PKG_VERSION");
        let asset = format!("atlas-{ver}-x86_64-unknown-linux-gnu.tar.gz");
        let stage = self.root().join("release");
        std::fs::create_dir_all(&stage).unwrap();
        let bin = self.atlas_file(hostd_version, true);
        std::fs::copy(&bin, stage.join("atlas")).unwrap();
        let st = Command::new("tar")
            .args(["-czf"])
            .arg(stage.join(&asset))
            .arg("-C")
            .arg(&stage)
            .arg("atlas")
            .status()
            .unwrap();
        assert!(st.success());
        let sum = if corrupt_sum {
            "0".repeat(64)
        } else {
            let out = Command::new("shasum")
                .args(["-a", "256"])
                .arg(stage.join(&asset))
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .next()
                .unwrap()
                .to_owned()
        };
        std::fs::write(
            stage.join(format!("{asset}.sha256")),
            format!("{sum}  {asset}\n"),
        )
        .unwrap();
        self.write_exec(
            "bin-ssh/curl",
            "#!/bin/sh\n\
             echo \"client curl $*\" >> \"$ROOT/log/client-curl\"\n\
             for a; do last=$a; done\n\
             case \"$*\" in *' -I '*) exit 0;; esac\n\
             while [ $# -gt 0 ]; do case \"$1\" in -o) out=$2; shift 2;; *) shift;; esac; done\n\
             f=\"$ROOT/release/${last##*/}\"\n\
             [ -f \"$f\" ] || { echo \"curl: (22) 404 $last\" >&2; exit 22; }\n\
             cp \"$f\" \"$out\"\n",
        );
    }
}

// Gap 8: the server cannot download (offline, or a proxy only this machine has settings for). The CLI downloads the
// release itself, verifies the checksum and streams the binary over ssh: no `--binary`, no ~/.atlas/remote cache.
#[test]
fn install_on_a_server_that_cannot_download_streams_the_release_from_this_machine() {
    let w = World::new();
    w.offline_server_online_client("7.7.7", false);
    let o = w.atlas(&["install", "me@srv"], "");
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(
        t.contains("downloading it here and sending it over ssh"),
        "{t}"
    );
    assert!(t.contains("Crew server ready"), "{t}");
    // The server tried first (and failed), then this machine fetched the asset and its checksum.
    assert!(
        w.read("log/server-curl").contains("releases/download"),
        "server curl log: {:?}\nsudo log:\n{}\n{t}",
        w.read("log/server-curl"),
        w.read("log/sudo")
    );
    let client = w.read("log/client-curl");
    assert!(client.contains(".tar.gz.sha256"), "{client}");
    // The binary on the server is the one inside the tarball.
    assert!(w.read("lib/atlas-crew/atlas").contains("7.7.7"));
}

// A release whose checksum does not match is never installed, and the message says both sides failed.
#[test]
fn a_download_here_with_a_bad_checksum_installs_nothing() {
    let w = World::new();
    w.offline_server_online_client("7.7.7", true);
    let o = w.atlas(&["install", "me@srv"], "");
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("on this machine:"), "{t}");
    assert!(t.contains("checksum mismatch"), "{t}");
    assert!(
        !w.exists("lib/atlas-crew/atlas"),
        "an unverified binary was installed"
    );
}

// Gap 4: ssh drops (255) AFTER the swap ran on the server, so the client retries it. The retry must not copy the new
// binary over atlas.prev: the rollback point stays the binary that was there before the update.
#[test]
fn an_update_retried_after_a_dropped_link_keeps_the_real_previous_binary() {
    let w = World::new();
    let old = w.atlas_file("1.0.0", true);
    assert!(
        w.atlas(
            &["install", "me@srv", "--binary", old.to_str().unwrap()],
            ""
        )
        .status
        .success()
    );
    let new = w.atlas_file("2.0.0", true);
    // The link drops after every run of the swap script, on all three attempts.
    std::fs::write(w.root().join("ssh-drop-after"), "update: ok").unwrap();
    let o = w.atlas(&["update", "me@srv", "--binary", new.to_str().unwrap()], "");
    let t = text(&o);
    // The staged binary is uploaded again on each attempt? No: only the swap is retried, atlas.new is gone, so the
    // retry fails with "no staged binary" and must not touch atlas.prev.
    let _ = t;
    assert!(
        w.read("lib/atlas-crew/atlas.prev").contains("1.0.0"),
        "atlas.prev was overwritten by a retry: {}",
        w.read("lib/atlas-crew/atlas.prev")
    );
    assert!(w.read("lib/atlas-crew/atlas").contains("2.0.0"));
}

// The key is the one `ssh -G` says ssh would offer, whatever its name (not only id_ed25519 / id_rsa), and a person
// with no key is told before anything on the server changes.
#[test]
fn install_authorizes_the_key_ssh_would_use_even_with_a_custom_name() {
    let w = World::new();
    std::fs::write(
        w.root().join("home/.ssh/work_key.pub"),
        "ssh-ed25519 WORKKEY me@work\n",
    )
    .unwrap();
    std::fs::remove_file(w.root().join("home/.ssh/id_ed25519.pub")).unwrap();
    std::fs::write(w.root().join("identity"), "~/.ssh/work_key").unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    let keys = w.read("var/lib/atlas-crew/.ssh/authorized_keys");
    assert!(keys.contains("ssh-ed25519 WORKKEY me@work"), "{keys}");
}

#[test]
fn install_without_any_key_stops_before_touching_the_server() {
    let w = World::new();
    std::fs::remove_file(w.root().join("home/.ssh/id_ed25519.pub")).unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    assert!(t.contains("no SSH public key found"), "{t}");
    assert!(!w.exists("log/sudo"), "the server was touched: {t}");
}

// Issue #518: a masked unit (a symlink to /dev/null) made `sudo tee` write the unit into /dev/null and
// `systemctl enable` refuse it ("Failed to enable unit: ... is masked"). The install unmasks the units it owns,
// says so, and ends with real files, whichever of its units were masked.
#[test]
fn install_unmasks_every_unit_it_owns_and_writes_real_files() {
    let w = World::new();
    let units = [
        "atlas-crew.service",
        "atlas-crew-spawn.socket",
        "atlas-crew-spawn.service",
        "atlas-crew-provision.socket",
        "atlas-crew-provision.service",
        "atlas-crew-egress.service",
    ];
    let dir = w.root().join("etc/systemd/system");
    std::fs::create_dir_all(&dir).unwrap();
    for u in units {
        std::os::unix::fs::symlink("/dev/null", dir.join(u)).unwrap();
    }
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("Crew server ready"), "{t}");
    for u in units {
        let meta = std::fs::symlink_metadata(dir.join(u)).unwrap();
        assert!(!meta.file_type().is_symlink(), "{u} is still a link: {t}");
        assert!(
            std::fs::read_to_string(dir.join(u))
                .unwrap()
                .contains("[Unit]"),
            "{u} has no unit text: {t}"
        );
        assert!(
            t.contains(&format!("{u} was masked")),
            "{u} not announced: {t}"
        );
    }
}

// Unmask impossible: the install stops at that unit with the command to run, as one `hint:` line (what Atlas
// Desktop shows) and a one-line error, and nothing is left half-written.
#[test]
fn install_that_cannot_unmask_says_the_command_to_run() {
    let w = World::new();
    let dir = w.root().join("etc/systemd/system");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink("/dev/null", dir.join("atlas-crew.service")).unwrap();
    std::fs::write(w.root().join("unmask-fails"), "").unwrap();
    let bin = w.atlas_file("9.9.9", true);
    let o = w.atlas(
        &["install", "me@srv", "--binary", bin.to_str().unwrap()],
        "",
    );
    let t = text(&o);
    assert!(!o.status.success(), "{t}");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.lines().any(|l| l
            == "hint: run `sudo systemctl unmask atlas-crew.service` on the server, then retry"),
        "{t}"
    );
    assert!(t.contains("is masked on the server"), "{t}");
    assert!(
        std::fs::symlink_metadata(dir.join("atlas-crew.service"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "{t}"
    );
    assert!(!t.contains("Crew server ready"), "{t}");
}
