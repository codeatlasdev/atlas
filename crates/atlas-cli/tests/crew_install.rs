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
            "chown",
            "ln",
            "uname",
        ] {
            w.link_real(t);
        }
        w.fake("uname", "echo 'Linux x86_64'");
        w.fake("sudo", FAKE_SUDO);
        // The unit "runs" the installed binary: restart succeeds, hostd answers when the binary says so.
        w.fake(
            "systemctl",
            "echo \"systemctl $*\" >> \"$ROOT/log/systemctl\"; exit 0",
        );
        w.fake(
            "id",
            "[ -d \"$ROOT/var/lib/$1\" ] || [ -f \"$ROOT/users/$1\" ]",
        );
        w.fake(
            "useradd",
            "for a; do u=$a; done; mkdir -p \"$ROOT/var/lib/$u\"; echo \"useradd $*\" >> \"$ROOT/log/useradd\"",
        );
        for t in ["curl", "git", "npm", "pgrep"] {
            w.fake(t, "exit 0");
        }
        // The fake ssh: run the last argument (`bash -c '<script>'`) on the simulated server.
        w.write_exec(
            "bin-ssh/ssh",
            &format!(
                "#!/bin/sh\nfor a; do last=$a; done\n[ -f \"$ROOT/ssh-down\" ] && {{ echo 'ssh: connect refused' >&2; exit 255; }}\nR=\"$ROOT\"\nexport PATH=\"$R/bin\"\nscript=$(printf '%s' \"$last\" | sed -e \"s#/usr/local/lib/atlas-crew#$R/lib/atlas-crew#g\" -e \"s#/usr/local/lib/atlas-bot#$R/lib/atlas-bot#g\" -e \"s#/etc/systemd/system#$R/etc/systemd/system#g\" -e \"s#/run/systemd/system#$R/run/systemd/system#g\" -e \"s#/var/lib#$R/var/lib#g\")\nif [ -f \"$R/ssh-drop-after\" ] && printf '%s' \"$script\" | grep -q \"$(cat \"$R/ssh-drop-after\")\"; then\n  /bin/sh -c \"$script\"; echo run >> \"$R/log/dropped-runs\"; echo 'ssh: connection reset' >&2; exit 255\nfi\nexec /bin/sh -c \"$script\"\n"
            ),
        );
        w
    }

    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn link_real(&self, name: &str) {
        for dir in ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
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
        let path = format!(
            "{}:{}",
            self.root().join("bin-ssh").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_atlas"))
            .arg("crew")
            .args(args)
            .env("PATH", path)
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
            "echo \"server curl $*\" >> \"$ROOT/log/server-curl\"; echo 'curl: (6) Could not resolve host' >&2; exit 6",
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
