//! Test-only simulated crew server: the real generated shell scripts run in a temp root against fake
//! `sudo`, `systemctl`, `id`, `userdel`, ... on `PATH`. Absolute server paths (`/usr/local/lib/atlas-crew`,
//! `/var/lib`, `/run/systemd/system`, `/etc/systemd/system`) are rewritten into the root, nothing else of
//! the script is touched. No mock of the other side: the scripts are exactly what `atlas crew` sends over ssh.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Sim {
    pub dir: tempfile::TempDir,
}

pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Sim {
    pub fn new() -> Self {
        let s = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        for d in [
            "bin",
            "lib/atlas-crew",
            "etc/systemd/system",
            "var/lib",
            "log",
        ] {
            std::fs::create_dir_all(s.root().join(d)).unwrap();
        }
        // Real tools the scripts run (linked from this machine into the closed PATH); tests remove one by
        // name to simulate a host without it.
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
            // GNU tar runs `gzip` for `z` (bsdtar on macOS has it built in): every Linux server has it.
            "gzip",
            "sha256sum",
            "shasum",
            "awk",
            "dirname",
            "ls",
            "grep",
            "true",
            "false",
            "printenv",
            "readlink",
        ] {
            s.link_real(t);
        }
        // Fakes that make a healthy Linux host. Tests overwrite or remove what they want broken.
        s.fake("uname", "echo 'Linux x86_64'");
        s.fake("sudo", FAKE_SUDO);
        s.fake("systemctl", FAKE_SYSTEMCTL);
        s.fake(
            "id",
            "[ -d \"$ROOT/var/lib/$1\" ] || [ -f \"$ROOT/users/$1\" ]",
        );
        for t in ["curl", "useradd", "git", "npm"] {
            s.fake(t, "exit 0");
        }
        s
    }

    /// Link the real `name` of this machine into the closed PATH (no-op when this machine has none).
    fn link_real(&self, name: &str) {
        for dir in ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            let src = Path::new(dir).join(name);
            if src.exists() {
                let _ = std::os::unix::fs::symlink(&src, self.path(&format!("bin/{name}")));
                return;
            }
        }
        // This machine lacks it (macOS has no `sha256sum`): a do-nothing fake keeps the simulated host complete
        // unless a test removes it.
        self.fake(name, "exit 0");
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root().join(rel)
    }

    pub fn write(&self, rel: &str, body: &[u8]) {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        // A fake over a linked real tool must replace the link, never write through it into the system.
        let _ = std::fs::remove_file(&p);
        std::fs::write(&p, body).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap_or_default()
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    pub fn fake(&self, name: &str, body: &str) {
        self.write(
            &format!("bin/{name}"),
            format!("#!/bin/sh\n{body}\n").as_bytes(),
        );
    }

    pub fn remove_fake(&self, name: &str) {
        let _ = std::fs::remove_file(self.path(&format!("bin/{name}")));
    }

    /// An `atlas` binary of the simulated server: answers `hostd --status` like the real one when
    /// `healthy`, prints nothing otherwise. `version` is what it reports.
    pub fn atlas_binary(&self, version: &str, healthy: bool) -> Vec<u8> {
        let status = if healthy {
            format!("echo '{{\"version\": \"{version}\"}}'")
        } else {
            "exit 1".to_string()
        };
        format!("#!/bin/sh\n# {version}\ncase \"$*\" in *--status*) {status};; esac\n").into_bytes()
    }

    pub fn install_atlas(&self, rel: &str, version: &str, healthy: bool) {
        self.write(rel, &self.atlas_binary(version, healthy));
    }

    /// Rewrite the server's absolute paths into this root.
    pub fn rooted(&self, script: &str) -> String {
        let r = self.root().to_string_lossy();
        script
            .replace("/usr/local/lib/atlas-crew", &format!("{r}/lib/atlas-crew"))
            .replace("/usr/local/lib/atlas-bot", &format!("{r}/lib/atlas-bot"))
            .replace("/etc/systemd/system", &format!("{r}/etc/systemd/system"))
            .replace("/run/systemd/system", &format!("{r}/run/systemd/system"))
            .replace("/var/lib", &format!("{r}/var/lib"))
    }

    /// Run `script` (already rooted by the caller or not: it is rooted here) as the remote `bash -c` does.
    pub fn run(&self, script: &str, stdin: &[u8]) -> Run {
        let script = self.rooted(script);
        let mut child = Command::new("/bin/bash")
            .arg("-c")
            .arg(&script)
            .env_clear()
            .env("ROOT", self.root())
            // Closed PATH: only the fakes and the coreutils linked into bin/, so a tool the test removed is
            // really missing (the Mac running the tests has curl and git in /usr/bin).
            .env("PATH", self.path("bin"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            let mut si = child.stdin.take().unwrap();
            let _ = si.write_all(stdin);
        }
        let o = child.wait_with_output().unwrap();
        Run {
            code: o.status.code(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }
}

/// `sudo [-n|-S|-A|-p x|-u user] cmd...`. With `$ROOT/sudo-password` present it enforces per-call auth like
/// `Defaults tty_tickets` without a tty: every call must bring the password itself, by `-S` (one line of stdin,
/// the rest of stdin goes to the command) or `-A` (the output of `$SUDO_ASKPASS`, stdin untouched); `-n` fails.
/// `-u` runs the command as that user (the fake does not switch users).
const FAKE_SUDO: &str = r#"
stdin_pw=0
while [ $# -gt 0 ]; do
  case "$1" in
    -n) nopw=1; shift;;
    -S) stdin_pw=1; shift;;
    -A) askpass=1; shift;;
    -p) shift 2;;
    -v) validate=1; shift;;
    -u) shift 2;;
    --) shift; break;;
    -*) shift;;
    *) break;;
  esac
done
if [ -f "$ROOT/sudo-password" ]; then
  if [ -n "$nopw" ]; then echo 'sudo: a password is required' >&2; exit 1; fi
  if [ -n "$askpass" ] && [ -n "$SUDO_ASKPASS" ]; then given=$("$SUDO_ASKPASS")
  elif [ "$stdin_pw" = 1 ]; then IFS= read -r given
  else echo 'sudo: a terminal is required to read the password' >&2; exit 1; fi
  if [ "$given" != "$(cat "$ROOT/sudo-password")" ]; then echo 'sudo: incorrect password attempt' >&2; exit 1; fi
fi
echo "sudo $*" >> "$ROOT/log/sudo"
[ -n "$validate" ] && exit 0
exec "$@"
"#;

/// The log records every call. Masking is modelled like systemd does it: a unit whose file is a symlink to
/// `/dev/null` is `masked` (`is-enabled` says so and exits 1), and `unmask` removes that link.
const FAKE_SYSTEMCTL: &str = r#"
echo "systemctl $*" >> "$ROOT/log/systemctl"
f="$ROOT/etc/systemd/system/$2"
masked() { [ -L "$f" ] && [ "$(readlink "$f")" = /dev/null ]; }
case "$1" in
  is-enabled) if masked; then echo masked; exit 1; fi;;
  unmask) if masked; then rm -f "$f"; fi;;
esac
exit 0
"#;
