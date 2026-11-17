//! `atlas update` against the real binary in a sandbox: a temp HOME, and stub `curl`, package
//! managers (`pacman`, `dpkg`, `rpm`) and privilege helpers (`sudo`, `pkexec`) first on `PATH`.
//! Nothing here touches the network, a real package manager or a real `sudo`.
//!
//! The binary under test is a debug build, which `atlas update` refuses to replace (it is a dev
//! binary). `ATLAS_UPDATE_PROMPT=force` lifts that half of the rule, and `ATLAS_UPDATE_EXE`
//! (debug builds only) stands a stub "installed" binary at `<sandbox>/usr/bin/atlas`, which is
//! what the install method is judged on and what the post-install refresh runs: it logs its
//! argv, so "refresh ran with the installed binary" is observable.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::{Digest, Sha256};

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
const LATEST: &str = "99.0.0";

fn script(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    /// `owner` = which package database claims `/usr/bin/atlas`: `pacman`, `dpkg`, `rpm`, or
    /// `none` (a hand-copied binary). `tty_less` is the default: `Command` has no tty.
    fn new(owner: &str) -> Self {
        let sb = Self {
            root: tempfile::tempdir().unwrap(),
        };
        let bin = sb.bin();
        let exe = sb.exe();
        let log = sb.log();

        // The "installed" CLI: records every invocation (`$0 args`), fails `desktop --sync`
        // when asked to.
        script(
            &exe,
            &format!(
                "echo \"atlas $*\" >> '{log}'\n\
                 if [ \"$1 $2\" = 'desktop --sync' ] && [ -n \"$FAIL_SYNC\" ]; then exit 1; fi\n\
                 exit 0",
                log = log.display()
            ),
        );

        let owned = exe.display().to_string();
        script(
            &bin.join("pacman"),
            &if owner == "pacman" {
                format!(
                    "[ \"$1\" = -Qo ] && echo \"{owned} is owned by codeatlas-atlas-bin 1.5.0alpha.13-2\" && exit 0\nexit 1"
                )
            } else {
                "exit 1".into()
            },
        );
        script(
            &bin.join("dpkg"),
            &if owner == "dpkg" {
                format!("[ \"$1\" = -S ] && echo \"atlas: {owned}\" && exit 0\nexit 1")
            } else {
                "exit 1".into()
            },
        );
        script(
            &bin.join("rpm"),
            &if owner == "rpm" {
                "[ \"$1\" = -qf ] && echo 'atlas-1.5.0-1.x86_64' && exit 0\nexit 1".to_string()
            } else {
                "exit 1".into()
            },
        );
        // Privilege helpers record what they were asked to run and do not run it.
        for helper in ["sudo", "pkexec"] {
            script(
                &bin.join(helper),
                &format!("echo \"{helper} $*\" >> '{}'\nexit 0", log.display()),
            );
        }
        script(&bin.join("curl"), &sb.curl_stub());
        sb
    }

    fn bin(&self) -> PathBuf {
        self.root.path().join("stubs")
    }
    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }
    fn exe(&self) -> PathBuf {
        self.root.path().join("usr/bin/atlas")
    }
    fn log(&self) -> PathBuf {
        self.root.path().join("calls.log")
    }
    fn assets(&self) -> PathBuf {
        self.root.path().join("assets")
    }

    /// Serves the three pages the update check reads and any file in `assets/` by basename.
    fn curl_stub(&self) -> String {
        let assets = self.assets();
        format!(
            r#"
for a in "$@"; do url="$a"; done
prev=""
out=""
for a in "$@"; do
  if [ "$prev" = "-o" ]; then out="$a"; fi
  prev="$a"
done
case "$url" in
  */releases/latest)
    if [ -f '{root}/latest' ]; then cat '{root}/latest'; exit 0; fi
    printf '%s' 'https://github.com/codeatlasdev/atlas/releases/tag/v{LATEST}'; exit 0;;
  */releases.atom*)
    if [ -f '{root}/feed.xml' ]; then cat '{root}/feed.xml'; exit 0; fi
    cat <<'EOF'
<feed><entry><link rel="alternate" href="https://github.com/codeatlasdev/atlas/releases/tag/v{LATEST}"/></entry></feed>
EOF
    exit 0;;
  */manifest.json) printf '200'; exit 0;;
  */data/yanked.json)
    if [ -f '{root}/yanked.json' ]; then cat '{root}/yanked.json'; exit 0; fi
    echo 'curl: (22) The requested URL returned error: 404' >&2; exit 22;;
esac
name="${{url##*/}}"
if [ -f '{assets}'/"$name" ] && [ -n "$out" ] && [ "$out" != /dev/null ]; then
  cp '{assets}'/"$name" "$out"; exit 0
fi
case " $* " in *" -I "*) printf '1024'; exit 0;; esac
echo "curl stub: no fixture for $url" >&2
exit 22
"#,
            assets = assets.display(),
            root = self.root.path().display()
        )
    }

    /// The release feed the stub serves: these tags, newest first (default: only `LATEST`); the
    /// `releases/latest` redirect points at the first.
    fn feed(&self, tags: &[&str]) {
        std::fs::write(
            self.root.path().join("latest"),
            format!(
                "https://github.com/codeatlasdev/atlas/releases/tag/{}",
                tags[0]
            ),
        )
        .unwrap();
        let entries: String = tags
            .iter()
            .map(|t| {
                format!(
                    r#"<entry><link rel="alternate" href="https://github.com/codeatlasdev/atlas/releases/tag/{t}"/></entry>"#
                )
            })
            .collect();
        std::fs::write(
            self.root.path().join("feed.xml"),
            format!("<feed>{entries}</feed>"),
        )
        .unwrap();
    }

    /// `data/yanked.json` as main serves it, written in the file's own schema.
    fn yank(&self, entries: &[(&str, &str, Option<&str>)]) {
        let list: Vec<serde_json::Value> = entries
            .iter()
            .map(|(v, why, rep)| {
                serde_json::json!({"version": v, "reason": why, "replacement": rep})
            })
            .collect();
        std::fs::write(
            self.root.path().join("yanked.json"),
            serde_json::json!({ "yanked": list }).to_string(),
        )
        .unwrap();
    }

    /// Put a release asset and its `.sha256` where the curl stub serves them. `sum` overrides
    /// the digest (to stage a corrupted download).
    fn asset(&self, name: &str, bytes: &[u8], sum: Option<&str>) {
        let dir = self.assets();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
        let digest = sum
            .map(str::to_string)
            .unwrap_or_else(|| hex::encode(Sha256::digest(bytes)));
        std::fs::write(
            dir.join(format!("{name}.sha256")),
            format!("{digest}  {name}\n"),
        )
        .unwrap();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.log())
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Take `name` off the whole `PATH`, the stub and the host's own: `run` then uses a copy of
    /// `/usr/bin` and `/bin` (symlinks) without it. A Linux desktop has a real `/usr/bin/pkexec`
    /// (polkit), which a removed stub alone does not hide.
    fn hide(&self, name: &str) {
        let _ = std::fs::remove_file(self.bin().join(name));
        let sys = self.root.path().join("sys");
        std::fs::create_dir_all(&sys).unwrap();
        for dir in ["/usr/bin", "/bin"] {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let to = sys.join(e.file_name());
                if e.file_name() != name && !to.exists() {
                    let _ = std::os::unix::fs::symlink(e.path(), to);
                }
            }
        }
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let sys = self.root.path().join("sys");
        // Stubs first, then only the base system tools (minus anything `hide` took away).
        let path = if sys.is_dir() {
            format!("{}:{}", self.bin().display(), sys.display())
        } else {
            format!("{}:/usr/bin:/bin", self.bin().display())
        };
        let mut cmd = Command::new(BIN);
        cmd.args(args)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", path)
            .env("ATLAS_UPDATE_EXE", self.exe())
            .env("ATLAS_UPDATE_PROMPT", "force")
            .env("ATLAS_UPDATE_CHANNEL", "alpha");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().unwrap()
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

// ── #144: detection and reporting ─────────────────────────────────────────────

/// Path rule + package database: each manager's database claims the binary and picks the asset.
#[test]
fn check_reports_current_and_latest_for_each_package_manager() {
    for (owner, asset_hint) in [
        ("pacman", "pacman -U"),
        ("dpkg", "apt install"),
        ("rpm", "install ./atlas-"),
    ] {
        let sb = Sandbox::new(owner);
        let o = sb.run(&["update", "--check"], &[]);
        let out = text(&o);
        assert!(o.status.success(), "{owner}: {out}");
        assert!(out.contains("Latest:  v99.0.0"), "{owner}: {out}");
        assert!(
            out.contains(&format!("Current: v{}", env!("CARGO_PKG_VERSION"))),
            "{out}"
        );
        assert!(
            sb.calls().is_empty(),
            "--check never escalates or installs: {:?}",
            sb.calls()
        );
        let j = sb.run(&["update", "--check", "--json"], &[]);
        let v: serde_json::Value = serde_json::from_slice(&j.stdout)
            .unwrap_or_else(|e| panic!("{owner}: {e}: {}", text(&j)));
        assert_eq!(v["action"], "package", "{owner}: {v}");
        assert_eq!(v["managed"], true);
        assert_eq!(v["offered"], LATEST);
        assert!(
            v["command"]
                .as_str()
                .unwrap()
                .contains(asset_hint.split(' ').next().unwrap()),
            "{owner}: {v}"
        );
    }
}

// ── #144: install ─────────────────────────────────────────────────────────────

fn stage_deb(sb: &Sandbox) -> String {
    let name = format!("atlas_{LATEST}_amd64.deb");
    sb.asset(&name, b"deb-bytes", None);
    name
}

#[test]
fn package_update_downloads_verifies_then_installs_through_pkexec_and_refreshes() {
    let sb = Sandbox::new("dpkg");
    let name = stage_deb(&sb);
    let o = sb.run(&["update", "--yes"], &[]);
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    let calls = sb.calls();

    let sudo = calls
        .iter()
        .position(|c| c.starts_with("pkexec apt-get install"))
        .unwrap_or_else(|| panic!("no pkexec apt-get install in {calls:?}\n{out}"));
    assert!(calls[sudo].contains(&name), "{}", calls[sudo]);
    assert!(!calls.iter().any(|c| c.starts_with("sudo ")), "{calls:?}");
    // The refresh runs with the *installed* binary, after the package manager returned.
    let refresh = calls
        .iter()
        .position(|c| c == "atlas skills install")
        .unwrap_or_else(|| panic!("no refresh in {calls:?}\n{out}"));
    assert!(refresh > sudo, "refresh must follow the install: {calls:?}");
    for agent in ["omp", "claude-code", "codex"] {
        assert!(
            calls
                .iter()
                .any(|c| c == &format!("atlas setup {agent} --refresh")),
            "{agent}: {calls:?}"
        );
    }
}

#[test]
fn a_checksum_mismatch_aborts_before_any_escalation() {
    let sb = Sandbox::new("dpkg");
    let name = format!("atlas_{LATEST}_amd64.deb");
    sb.asset(&name, b"deb-bytes", Some(&"0".repeat(64)));
    let o = sb.run(&["update", "--yes"], &[]);
    let out = text(&o);
    assert!(!o.status.success(), "{out}");
    assert!(out.contains("checksum mismatch"), "{out}");
    assert!(
        sb.calls().is_empty(),
        "sudo/pkexec/package manager must never run on an unverified file: {:?}",
        sb.calls()
    );
}

#[test]
fn a_missing_checksum_file_also_aborts_before_any_escalation() {
    let sb = Sandbox::new("pacman");
    let name = format!("codeatlas-atlas-bin-{LATEST}-1-x86_64.pkg.tar.zst");
    sb.asset(&name, b"pkg", None);
    std::fs::remove_file(sb.assets().join(format!("{name}.sha256"))).unwrap();
    let o = sb.run(&["update", "--yes"], &[]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
}

#[test]
fn declining_the_confirmation_installs_nothing() {
    let sb = Sandbox::new("dpkg");
    stage_deb(&sb);
    // No `--yes` and no terminal: there is nobody to ask, which is a refusal, not a guess.
    let o = sb.run(&["update"], &[]);
    let out = text(&o);
    assert!(!o.status.success(), "{out}");
    assert!(out.contains("--yes"), "tells how to proceed: {out}");
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
}

// #278: the in-app Update has no terminal and no `--yes` until the user confirms in the app.
// It must end in the `managed package` state with the exact command, not an error, and
// without having downloaded or escalated anything.
#[test]
fn the_app_without_confirmation_gets_the_managed_event_with_the_command() {
    for (owner, manager, command) in [
        ("dpkg", "apt", "sudo apt install ./atlas_99.0.0_amd64.deb"),
        (
            "pacman",
            "pacman",
            "sudo pacman -U codeatlas-atlas-bin-99.0.0-1-x86_64.pkg.tar.zst",
        ),
    ] {
        let sb = Sandbox::new(owner);
        // No asset is staged: the curl stub fails any download, so a download would be an error.
        let o = sb.run(&["update"], &[("ATLAS_UPDATE_PROGRESS", "1")]);
        let out = text(&o);
        assert!(o.status.success(), "{owner}: {out}");
        assert!(
            out.contains(&format!(
                "@@atlas managed package {manager} install {command}"
            )),
            "{owner}: {out}"
        );
        assert!(out.contains(command), "the command is printed too: {out}");
        assert!(sb.calls().is_empty(), "{owner}: {:?}", sb.calls());
    }
    let sb = Sandbox::new("rpm");
    let o = sb.run(&["update"], &[("ATLAS_UPDATE_PROGRESS", "1")]);
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(
        out.contains("install sudo ") && out.contains("install ./atlas-99.0.0-1.x86_64.rpm"),
        "{out}"
    );
}

#[test]
fn without_pkexec_or_a_terminal_the_command_is_shown_and_sudo_is_never_run() {
    let sb = Sandbox::new("dpkg");
    sb.hide("pkexec");
    stage_deb(&sb);
    // Confirmed in the app, but nothing can ask for the password: manual, not installable.
    let o = sb.run(&["update", "--yes"], &[("ATLAS_UPDATE_PROGRESS", "1")]);
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(
        out.contains(
            "@@atlas managed package apt manual sudo apt install ./atlas_99.0.0_amd64.deb"
        ),
        "{out}"
    );
    assert!(
        sb.calls().is_empty(),
        "no sudo without a tty: {:?}",
        sb.calls()
    );
    // Outside the app the same situation is a failure that still names the command.
    let o = sb.run(&["update", "--yes"], &[]);
    let out = text(&o);
    assert!(!o.status.success(), "{out}");
    assert!(
        out.contains("sudo apt install ./atlas_99.0.0_amd64.deb"),
        "{out}"
    );
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
}

#[test]
fn the_app_confirmation_installs_through_pkexec() {
    let sb = Sandbox::new("dpkg");
    let name = stage_deb(&sb);
    let o = sb.run(&["update", "--yes"], &[("ATLAS_UPDATE_PROGRESS", "1")]);
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(!out.contains("@@atlas managed"), "{out}");
    assert!(
        sb.calls()
            .iter()
            .any(|c| c.starts_with("pkexec apt-get install") && c.contains(&name)),
        "{:?}",
        sb.calls()
    );
}

#[test]
fn without_a_terminal_pkexec_is_the_escalation() {
    let sb = Sandbox::new("rpm");
    sb.asset(&format!("atlas-{LATEST}-1.x86_64.rpm"), b"rpm", None);
    let o = sb.run(&["update", "--yes"], &[]);
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    let calls = sb.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("pkexec ")),
        "no tty → pkexec: {calls:?}"
    );
    assert!(!calls.iter().any(|c| c.starts_with("sudo ")), "{calls:?}");
}

#[test]
fn a_failing_package_manager_is_an_error_and_skips_the_refresh() {
    let sb = Sandbox::new("dpkg");
    stage_deb(&sb);
    script(
        &sb.bin().join("pkexec"),
        &format!("echo \"pkexec $*\" >> '{}'\nexit 5", sb.log().display()),
    );
    let o = sb.run(&["update", "--yes"], &[]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(
        !sb.calls().iter().any(|c| c == "atlas skills install"),
        "nothing was installed, nothing is refreshed: {:?}",
        sb.calls()
    );
}

// ── G11: a managed install reports `managed`, never a silent success ─────────

#[test]
fn homebrew_reports_a_managed_action_in_json() {
    let sb = Sandbox::new("none");
    let brew = sb.root.path().join("opt/homebrew/bin/atlas");
    script(&brew, "exit 0");
    let o = sb.run(
        &["update", "--check", "--json"],
        &[("ATLAS_UPDATE_EXE", brew.to_str().unwrap())],
    );
    let v: serde_json::Value =
        serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&o)));
    assert_eq!(v["action"], "managed", "{v}");
    assert_eq!(v["command"], "brew upgrade atlas");
}

#[test]
fn homebrew_update_emits_the_managed_event_instead_of_succeeding_silently() {
    let sb = Sandbox::new("none");
    let brew = sb.root.path().join("opt/homebrew/bin/atlas");
    script(&brew, "exit 0");
    let o = sb.run(
        &["update"],
        &[
            ("ATLAS_UPDATE_EXE", brew.to_str().unwrap()),
            ("ATLAS_UPDATE_PROGRESS", "1"),
        ],
    );
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("@@atlas managed homebrew"), "{out}");
    assert!(out.contains("brew upgrade atlas"), "{out}");
}

// ── G3: dev builds and linked binaries are never replaced ────────────────────

#[test]
fn a_debug_build_is_refused_unless_forced() {
    let sb = Sandbox::new("none");
    // No force: this debug-profile test binary is a dev build.
    let managed = sb.root.path().join("home/.atlas/bin/atlas");
    script(&managed, "exit 0");
    let o = sb.run(
        &["update"],
        &[
            ("ATLAS_UPDATE_PROMPT", ""),
            ("ATLAS_UPDATE_EXE", managed.to_str().unwrap()),
        ],
    );
    let out = text(&o);
    if cfg!(debug_assertions) {
        assert!(!o.status.success(), "{out}");
        assert!(out.contains("dev build"), "{out}");
        assert!(sb.calls().is_empty());
    }
}

#[test]
fn a_symlinked_binary_is_refused_even_when_forced() {
    let sb = Sandbox::new("none");
    let real = sb.root.path().join("checkout/target/debug/atlas");
    script(&real, "exit 0");
    let link = sb.home().join(".atlas/bin/atlas");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let o = sb.run(&["update"], &[("ATLAS_UPDATE_EXE", link.to_str().unwrap())]);
    let out = text(&o);
    assert!(!o.status.success(), "{out}");
    assert!(out.contains("dev build") || out.contains("linked"), "{out}");
}

// ── G7/G14: the cache survives a broken write and an offline start ───────────

#[test]
fn a_failed_check_is_stamped_so_offline_starts_do_not_retry() {
    let sb = Sandbox::new("none");
    // curl that always fails: offline.
    script(&sb.bin().join("curl"), "exit 7");
    let managed = sb.home().join(".atlas/bin/atlas");
    script(&managed, "exit 0");
    let env = [("ATLAS_UPDATE_EXE", managed.to_str().unwrap())];
    let o = sb.run(&["update", "--check"], &env);
    assert!(!o.status.success(), "{}", text(&o));
    let cache: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(sb.home().join(".atlas/.update_check")).unwrap(),
    )
    .unwrap();
    assert!(
        cache["failed_at"]
            .as_u64()
            .is_some_and(|t| t > 1_000_000_000),
        "failure stamped: {cache}"
    );
}

/// A phone's "check now" is `atlas update --check --json` run by hostd, and hostd later reads
/// the cache for the version it shows (`info.update`). A check that leaves the cache as it was
/// would tell the phone the old answer for up to six hours.
#[test]
fn a_check_records_what_it_found_in_the_cache_hostd_reads() {
    for args in [
        &["update", "--check", "--json"][..],
        &["update", "--check"][..],
    ] {
        let sb = Sandbox::new("none");
        let o = sb.run(args, &[]);
        assert!(o.status.success(), "{args:?}: {}", text(&o));
        let cache: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(sb.home().join(".atlas/.update_check"))
                .unwrap_or_else(|e| panic!("{args:?}: no cache written: {e}")),
        )
        .unwrap();
        assert_eq!(cache["available_version"], LATEST, "{args:?}: {cache}");
        assert_eq!(cache["channel"], "alpha", "{args:?}: {cache}");
        assert!(
            cache["last_checked"]
                .as_u64()
                .is_some_and(|t| t > 1_000_000_000),
            "{args:?}: {cache}"
        );
    }
}

// ── #313: the yank list ───────────────────────────────────────────────────────

fn check_json(sb: &Sandbox) -> serde_json::Value {
    let o = sb.run(&["update", "--check", "--json"], &[]);
    assert!(o.status.success(), "{}", text(&o));
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&o)))
}

/// A yanked newest release is never what `atlas update` offers: the previous published one is.
#[test]
fn a_yanked_release_is_not_offered_by_the_update_check() {
    let sb = Sandbox::new("none");
    sb.feed(&["v99.1.0", "v99.0.0"]);
    sb.yank(&[("99.1.0", "corrupts the workspace", Some("99.2.0"))]);
    let j = check_json(&sb);
    assert_eq!(j["offered"], "99.0.0", "{j}");
    assert_eq!(
        j["yanked"],
        serde_json::Value::Null,
        "this install is fine: {j}"
    );
}

/// A list that cannot be read changes nothing: the newest release is still offered.
#[test]
fn an_unreadable_yank_list_still_offers_the_newest_release() {
    let sb = Sandbox::new("none");
    sb.feed(&["v99.1.0", "v99.0.0"]);
    std::fs::write(sb.root.path().join("yanked.json"), "<html>502</html>").unwrap();
    assert_eq!(check_json(&sb)["offered"], "99.1.0");
}

/// The running version is the yanked one: the report says why and what replaces it.
#[test]
fn a_yanked_running_version_is_told_why_in_the_report_and_in_the_text() {
    let sb = Sandbox::new("none");
    let me = env!("CARGO_PKG_VERSION");
    sb.yank(&[(me, "corrupts the workspace", Some("99.0.0"))]);
    let j = check_json(&sb);
    assert_eq!(j["offered"], "99.0.0", "{j}");
    assert_eq!(
        j["yanked"],
        serde_json::json!({"version": me, "reason": "corrupts the workspace", "replacement": "99.0.0"})
    );
    let out = text(&sb.run(&["update", "--check"], &[]));
    assert!(out.contains("withdrawn"), "{out}");
    assert!(out.contains("corrupts the workspace"), "{out}");
    assert!(out.contains("99.0.0"), "{out}");
}

/// Yanked, and the channel's newest safe release is *older* than what runs: that is a way out, not a
/// "you are ahead, nothing to do". An ordinary ahead-of-channel build stays untouched.
#[test]
fn a_yanked_running_version_is_offered_the_older_safe_release_without_the_downgrade_flag() {
    let sb = Sandbox::new("none");
    sb.feed(&["v1.0.0"]);
    let ahead = text(&sb.run(&["update", "--check"], &[]));
    assert!(ahead.contains("Nothing is installed"), "{ahead}");
    assert!(!ahead.contains("withdrawn"), "{ahead}");

    sb.yank(&[(env!("CARGO_PKG_VERSION"), "corrupts the workspace", None)]);
    let out = text(&sb.run(&["update", "--check"], &[]));
    assert!(out.contains("withdrawn"), "{out}");
    assert!(out.contains("1.0.0"), "{out}");
    assert!(!out.contains("Nothing is installed"), "{out}");
}
