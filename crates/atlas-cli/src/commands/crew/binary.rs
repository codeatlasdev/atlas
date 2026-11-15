//! The `atlas` binary on a crew server: which release asset fits the host, the shell that installs it,
//! the update (backup, restart, wait for hostd, roll back) and the version comparison `status` shows.
//! Pure functions: what the host said goes in, shell text and decisions come out, so every case is
//! tested without ssh.

use anyhow::{Result, bail};

use super::layout::{INSTALL_DIR, SERVICE, WHICH_USER};
use crate::commands::deploy::remote::shq;

const X86_64: &str = "x86_64-unknown-linux-gnu";
const AARCH64: &str = "aarch64-unknown-linux-gnu";

/// How long the update waits for hostd to answer after the restart, before it rolls back.
pub const HEALTH_TIMEOUT_S: u32 = 60;

/// The release target triple for a host's `uname -sm` (`Linux x86_64`, `Linux aarch64`).
pub fn target_for_uname(uname: &str) -> Result<&'static str> {
    match uname.trim() {
        "Linux x86_64" => Ok(X86_64),
        // Some distros' arm builds report `arm64` for the same CPU.
        "Linux aarch64" | "Linux arm64" => Ok(AARCH64),
        other => bail!(
            "atlas crew needs a Linux x86_64 or aarch64 host with systemd and sudo (got: {other})"
        ),
    }
}

/// `atlas-<version>-<target>.tar.gz`, the CLI asset of a release.
pub fn asset_name(version: &str, target: &str) -> String {
    format!("atlas-{version}-{target}.tar.gz")
}

/// Where the release asset of `version` for `target` is published.
pub fn asset_url(version: &str, target: &str) -> String {
    format!(
        "https://github.com/codeatlasdev/atlas/releases/download/v{version}/{}",
        asset_name(version, target)
    )
}

/// The locally cached Linux binary for `target` (offline fallback), relative to the home dir.
pub fn offline_cache_path(target: &str) -> &'static str {
    if target == AARCH64 {
        ".atlas/remote/atlas-aarch64-linux"
    } else {
        ".atlas/remote/atlas-x86_64-linux"
    }
}

/// Shell (run on the host) that downloads `version` for `target`, checks its `.sha256` and leaves the
/// verified binary at `<INSTALL_DIR>/atlas.new`. Nothing replaces `atlas` here. The temp file template ends in
/// the X's (BSD and busybox `mktemp` refuse a suffix after them, and macOS leaves a literal file behind that
/// makes the next call fail) and is removed however the script ends.
pub fn download_script(version: &str, target: &str) -> String {
    let url = asset_url(version, target);
    format!(
        r#"sudo mkdir -p {d} &&
tmpf=$(mktemp /tmp/atlas-crew-XXXXXX) &&
trap 'rm -f "$tmpf"' EXIT &&
curl -fsSL {url_q} -o "$tmpf" &&
WANT=$(curl -fsSL {sha_q} | awk '{{print $1}}') &&
GOT=$(sha256sum "$tmpf" | awk '{{print $1}}') &&
{{ [ "$WANT" = "$GOT" ] || {{ echo 'checksum mismatch' >&2; exit 1; }}; }} &&
sudo tar -xzf "$tmpf" -O atlas | sudo tee {d}/atlas.new > /dev/null &&
sudo chmod +x {d}/atlas.new"#,
        d = INSTALL_DIR,
        url_q = shq(&url),
        sha_q = shq(&format!("{url}.sha256")),
    )
}

/// Shell reading a binary from stdin into `<INSTALL_DIR>/atlas.new` (the offline / `--binary` path).
pub fn stage_upload_script() -> String {
    format!(
        "sudo mkdir -p {d} && sudo tee {d}/atlas.new > /dev/null && sudo chmod +x {d}/atlas.new",
        d = INSTALL_DIR
    )
}

/// Shell for an install or a reinstall: the same swap as an update ([`swap_script`]), so the binary that is
/// there keeps as `atlas.prev`, the unit is restarted, hostd must answer within `timeout_s`, and a binary
/// that does not bring it up is rolled back. A fresh host has no `atlas` yet, so no `.prev` and, if the new
/// binary is dead, exit 4 (nothing to fall back to). The unit file must already exist.
pub fn install_script(timeout_s: u32) -> String {
    swap_script(timeout_s)
}

/// What the host's one preflight probe saw.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Preflight {
    /// `uname -sm`.
    pub uname: String,
    /// `/run/systemd/system` exists: systemd is PID 1 (a container with `systemctl` installed has none).
    pub systemd: bool,
    /// Tools the install or the runtime needs that the host lacks.
    pub tools: Vec<String>,
}

impl Preflight {
    pub fn is_ready(&self) -> bool {
        self.systemd && self.tools.is_empty()
    }
}

/// Every tool the scripts and the crew runtime need on the host, with the Debian/Ubuntu package that
/// brings it (`coreutils` ones share a package). Checked in one go so the person sees the whole list.
const NEEDED: &[(&str, &str)] = &[
    ("sudo", "sudo"),
    ("systemctl", "systemd"),
    ("bash", "bash"),
    ("curl", "curl"),
    ("tar", "tar"),
    ("sha256sum", "coreutils"),
    ("mktemp", "coreutils"),
    ("tee", "coreutils"),
    ("awk", "gawk"),
    ("useradd", "passwd"),
    ("git", "git"),
];

/// Shell (run on the host) that changes nothing: prints `uname -sm`, `systemd: yes|no` and one
/// `missing: <tool>` line per missing tool, and exits 1 when anything is missing.
pub fn preflight_script() -> String {
    let tools = NEEDED.iter().map(|(t, _)| *t).collect::<Vec<_>>().join(" ");
    format!(
        r#"uname -sm; bad=0
if [ -d /run/systemd/system ]; then echo 'systemd: yes'; else echo 'systemd: no'; bad=1; fi
for t in {tools}; do command -v "$t" >/dev/null 2>&1 || {{ echo "missing: $t"; bad=1; }}; done
exit $bad"#
    )
}

/// Read [`preflight_script`]'s output.
pub fn parse_preflight(stdout: &str) -> Preflight {
    let mut p = Preflight::default();
    for (i, l) in stdout.lines().enumerate() {
        if let Some(t) = l.strip_prefix("missing: ") {
            p.tools.push(t.trim().to_owned());
        } else if let Some(s) = l.strip_prefix("systemd: ") {
            p.systemd = s.trim() == "yes";
        } else if i == 0 {
            p.uname = l.trim().to_owned();
        }
    }
    p
}

/// The one error for everything the host lacks: each missing tool, the package that has it, one install line.
pub fn preflight_error(target: &str, p: &Preflight) -> anyhow::Error {
    let mut lines = Vec::new();
    if !p.systemd {
        lines.push(
            "systemd is not running as PID 1 (no /run/systemd/system): a crew server needs a systemd host, not a container or an OpenRC/runit system"
                .to_owned(),
        );
    }
    let pkgs: Vec<&str> = p
        .tools
        .iter()
        .filter_map(|t| NEEDED.iter().find(|(n, _)| n == t).map(|(_, pkg)| *pkg))
        .collect();
    if !p.tools.is_empty() {
        let mut uniq = pkgs.clone();
        uniq.sort_unstable();
        uniq.dedup();
        lines.push(format!(
            "missing on the host: {}\n  Debian/Ubuntu: sudo apt-get install -y {}",
            p.tools.join(", "),
            uniq.join(" ")
        ));
    }
    anyhow::anyhow!(
        "{target} is not ready for a crew server; nothing was changed:\n  {}",
        lines.join("\n  ")
    )
}

/// `script` for a host whose sudo asks for a password. With `tty_tickets` (the Debian and Ubuntu default) the
/// credential `sudo -v` caches belongs to one tty, and every ssh exec has its own, so a password given once
/// is lost by the next call. Each call instead reads the password from the FIRST line of its stdin (never
/// argv, never in the script text) and answers every `sudo` in it with `sudo -A`: a temp dir at the front of
/// `PATH` holds an askpass helper (it prints the password from this shell's environment) and a `sudo` shim that
/// adds `-A`. The password never goes through the stdin of `sudo`, so the rest of the call's stdin stays
/// untouched for the command that reads it (`sudo tee file` of an upload), whichever `sudo` comes first. The
/// temp dir is removed on exit.
///
/// The returned text never contains a `"`, whatever `script` holds: `script` itself is written to a file in
/// that temp dir from octal escapes ([`octal`]) and run from there, with the caller's stdin. A Windows client
/// sends a script that has a `"` as `printf … | bash` (`atlas_core::ssh::windows_safe`), which would make bash
/// read its script from the very stdin that carries the password and the upload.
pub fn with_sudo_password(script: &str) -> String {
    format!(
        r"IFS= read -r ATLAS_SUDO_PW; export ATLAS_SUDO_PW; \
ATLAS_SHIM=$(mktemp -d) || exit 1; trap 'rm -rf '$ATLAS_SHIM EXIT; \
real=$(command -v sudo) || exit 1; \
printf '%b\n' '#!/bin/sh' 'exec printenv ATLAS_SUDO_PW' > $ATLAS_SHIM/askpass; \
printf '%b\n' '#!/bin/sh' 'exec '$real' -A \042$@\042' > $ATLAS_SHIM/sudo; \
printf %b '{body}' > $ATLAS_SHIM/run; \
chmod 700 $ATLAS_SHIM/askpass $ATLAS_SHIM/sudo; \
export SUDO_ASKPASS=$ATLAS_SHIM/askpass PATH=$ATLAS_SHIM:$PATH; \
bash $ATLAS_SHIM/run; exit $?",
        body = octal(script)
    )
}

/// `s` as text for `printf %b` that has no quote, no `$` and no raw backslash: every byte outside a small safe
/// set becomes `\0NNN`, so the remote `printf` gives back exactly `s`. The leading `0` matters: `printf %b` reads
/// `\0` followed by up to three octal digits, so a literal digit after the escape (`$0`) is never swallowed.
fn octal(s: &str) -> String {
    use std::fmt::Write as _;
    s.bytes()
        .fold(String::with_capacity(s.len() * 2), |mut o, b| {
            if b.is_ascii_alphanumeric() || b" ./_:=-".contains(&b) {
                o.push(b as char);
            } else {
                let _ = write!(o, "\\0{b:03o}");
            }
            o
        })
}

/// What the hostd health wait saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// `atlas hostd --status` answered with this version.
    Up(String),
    /// The wait ran out without an answer.
    Down,
}

/// What to do after the restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The new binary is running: keep it.
    Keep,
    /// hostd answered but runs another version than the one installed.
    KeepWithWarning(String),
    /// hostd did not come back: restore `atlas.prev` and restart.
    RollBack,
}

/// Decide from the health wait. `expected` is the version just installed (`None` for an uploaded
/// binary of unknown version). The shell in [`swap_script`] applies the same rule for the rollback
/// itself (it must not depend on the ssh link surviving); this is what the CLI reports.
pub fn verdict(health: &Health, expected: Option<&str>) -> Verdict {
    match (health, expected) {
        (Health::Down, _) => Verdict::RollBack,
        (Health::Up(got), Some(want)) if got != want => Verdict::KeepWithWarning(format!(
            "hostd answers as {got}, not {want}: a process from before the update may still hold the socket"
        )),
        (Health::Up(_), _) => Verdict::Keep,
    }
}

/// Read the script's `hostd: <version>` lines back (the last one is the hostd that is running now).
pub fn parse_health(stdout: &str) -> Health {
    stdout
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("hostd: "))
        .map_or(Health::Down, |v| Health::Up(v.trim().to_owned()))
}

/// A warning to show after a run that ended well, from what the script printed; `None` when all is as expected.
pub fn report(stdout: &str, expected: Option<&str>) -> Option<String> {
    match verdict(&parse_health(stdout), expected) {
        Verdict::KeepWithWarning(w) => Some(w),
        Verdict::Keep | Verdict::RollBack => None,
    }
}

/// Shell that waits up to `timeout_s` for hostd to answer as the crew user, in its data dir. On success it
/// prints `hostd: <version>` and leaves `$?` 0; otherwise it ends non-zero. `$u` is set by the caller.
fn health_wait(timeout_s: u32) -> String {
    format!(
        r#"healthy() {{ i=0; while [ "$i" -lt {timeout_s} ]; do \
v=$(sudo -u "$u" {INSTALL_DIR}/atlas hostd --status --dir "/var/lib/$u/.atlas" 2>/dev/null | sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' | head -1); \
if [ -n "$v" ]; then echo "hostd: $v"; return 0; fi; i=$((i+2)); sleep 2; done; return 1; }}"#
    )
}

/// Restart the unit and wait; `sudo systemctl restart` failing counts as unhealthy.
fn restart_and_wait() -> String {
    format!("sudo systemctl restart {SERVICE} && healthy")
}

/// Shell: put the staged `atlas.new` in place (keeping the current one as `atlas.prev`), restart the unit,
/// wait up to `timeout_s` for hostd and, if it does not answer, restore the binary that was there and restart.
/// Prints `update: ok <version>` or `update: rolled-back` and exits 0 / 3 (rolled back) / 4 (rollback failed).
/// Only the binary dir is touched: the crew home (data, secrets, `crew.json` with the paused flag) never is,
/// so a paused crew stays paused.
///
/// Safe to run twice (ssh can drop after the remote side ran and the client retries): the backup is made
/// only from a binary that differs from the staged one, so a re-run never overwrites `atlas.prev` with the
/// new binary, and a stale `atlas.prev` from an older run is never what a failed run rolls back to.
pub fn swap_script(timeout_s: u32) -> String {
    let d = INSTALL_DIR;
    format!(
        r#"u=$({WHICH_USER}); {health}
test -x {d}/atlas.new || {{ echo 'no staged binary at {d}/atlas.new' >&2; exit 2; }}
kept=
if [ -x {d}/atlas ] && ! cmp -s {d}/atlas {d}/atlas.new; then sudo cp -p {d}/atlas {d}/atlas.prev || exit 2; kept=1; fi
sudo mv -f {d}/atlas.new {d}/atlas || exit 2
if {restart}; then echo "update: ok $v"; exit 0; fi
echo 'hostd did not come back' >&2
if [ -n "$kept" ]; then
  echo 'restoring the previous binary' >&2
  sudo mv -f {d}/atlas.prev {d}/atlas
  if {restart}; then echo 'update: rolled-back'; exit 3; fi
fi
echo 'update: rollback-failed' >&2; exit 4"#,
        health = health_wait(timeout_s),
        restart = restart_and_wait(),
    )
}

/// Shell: restore `atlas.prev` (the binary before the last update) and restart, same health wait. The binary it
/// replaces becomes the new `atlas.prev`, so running it twice flips back.
pub fn rollback_script(timeout_s: u32) -> String {
    let d = INSTALL_DIR;
    format!(
        r#"u=$({WHICH_USER}); {health}
test -x {d}/atlas.prev || {{ echo 'no previous binary at {d}/atlas.prev: nothing to roll back to' >&2; exit 2; }}
sudo mv -f {d}/atlas {d}/atlas.swap || exit 2
sudo mv -f {d}/atlas.prev {d}/atlas || exit 2
sudo mv -f {d}/atlas.swap {d}/atlas.prev || exit 2
if {restart}; then echo "update: ok $v"; exit 0; fi
echo 'update: rollback-failed' >&2; exit 4"#,
        health = health_wait(timeout_s),
        restart = restart_and_wait(),
    )
}

/// Whether the server's atlas is behind, level with or ahead of this CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Behind,
    Ahead,
    /// The server's version could not be read or compared.
    Unknown,
}

/// Compare the server's version (`atlas hostd --status` JSON `"version"`) with the CLI's.
pub fn freshness(server: Option<&str>, cli: &str) -> Freshness {
    let (Some(Ok(s)), Ok(c)) = (
        server.map(semver::Version::parse),
        semver::Version::parse(cli),
    ) else {
        return Freshness::Unknown;
    };
    match s.cmp(&c) {
        std::cmp::Ordering::Less => Freshness::Behind,
        std::cmp::Ordering::Equal => Freshness::Current,
        std::cmp::Ordering::Greater => Freshness::Ahead,
    }
}

/// The `"version"` of `atlas hostd --status`'s JSON output (pretty or not).
pub fn version_from_status(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json.trim())
        .ok()?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// One line for `atlas crew status`.
pub fn version_line(server: Option<&str>, cli: &str) -> String {
    match (freshness(server, cli), server) {
        (Freshness::Current, _) => format!("atlas {cli} (up to date)"),
        (Freshness::Behind, Some(s)) => {
            format!("atlas {s} (behind this CLI's {cli}: run atlas crew update)")
        }
        (Freshness::Ahead, Some(s)) => format!("atlas {s} (ahead of this CLI's {cli})"),
        _ => "atlas version unknown (is hostd running?)".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Issue #203: install only accepted x86_64; Graviton, Oracle A1 and Hetzner CAX were refused.
    #[test]
    fn the_release_target_follows_the_hosts_uname() {
        assert_eq!(
            target_for_uname("Linux x86_64").unwrap(),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            target_for_uname("Linux aarch64").unwrap(),
            "aarch64-unknown-linux-gnu"
        );
        // `uname -m` says arm64 on some distros' arm builds; same CPU, same asset.
        assert_eq!(
            target_for_uname("Linux arm64").unwrap(),
            "aarch64-unknown-linux-gnu"
        );
        // Trailing newline from the ssh output.
        assert_eq!(
            target_for_uname("Linux aarch64\n").unwrap(),
            "aarch64-unknown-linux-gnu"
        );
    }

    #[test]
    fn other_hosts_are_refused_with_what_they_are_and_what_works() {
        for (uname, got) in [
            ("Darwin arm64", "Darwin arm64"),
            ("Linux armv7l", "Linux armv7l"),
            ("Linux riscv64", "Linux riscv64"),
            ("Linux i686", "Linux i686"),
            ("FreeBSD amd64", "FreeBSD amd64"),
            ("", ""),
        ] {
            let e = target_for_uname(uname).unwrap_err().to_string();
            assert!(e.contains("x86_64") && e.contains("aarch64"), "{e}");
            assert!(e.contains(&format!("got: {got}")), "{e}");
        }
    }

    #[test]
    fn asset_names_and_urls_match_the_release_workflow() {
        assert_eq!(
            asset_name("1.6.0", "aarch64-unknown-linux-gnu"),
            "atlas-1.6.0-aarch64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            asset_url("1.6.0-alpha.2", "x86_64-unknown-linux-gnu"),
            "https://github.com/codeatlasdev/atlas/releases/download/v1.6.0-alpha.2/atlas-1.6.0-alpha.2-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    // The offline cache `atlas update` fills is per arch; x86_64 keeps its old name.
    #[test]
    fn the_offline_cache_is_per_arch() {
        assert_eq!(
            offline_cache_path("x86_64-unknown-linux-gnu"),
            ".atlas/remote/atlas-x86_64-linux"
        );
        assert_eq!(
            offline_cache_path("aarch64-unknown-linux-gnu"),
            ".atlas/remote/atlas-aarch64-linux"
        );
    }

    #[test]
    fn the_download_stages_a_checked_atlas_new_and_never_replaces_atlas() {
        let sh = download_script("1.6.0", "aarch64-unknown-linux-gnu");
        assert!(sh.contains(
            "https://github.com/codeatlasdev/atlas/releases/download/v1.6.0/atlas-1.6.0-aarch64-unknown-linux-gnu.tar.gz"
        ));
        assert!(sh.contains(".tar.gz.sha256"));
        assert!(sh.contains("checksum mismatch"));
        assert!(sh.contains("/usr/local/lib/atlas-crew/atlas.new"));
        // The running binary is only touched by the swap, after the download succeeded.
        assert!(!sh.contains("mv "), "{sh}");
        assert!(!sh.contains("/atlas-crew/atlas "), "{sh}");
    }

    #[test]
    fn a_checksum_mismatch_stops_the_download_script() {
        let sh = download_script("1.6.0", "x86_64-unknown-linux-gnu");
        let mismatch = sh.find("checksum mismatch").unwrap();
        let tar = sh.find("tar -x").unwrap();
        assert!(mismatch < tar, "extract only after the checksum passed");
        assert!(sh.contains("exit 1"));
    }

    #[test]
    fn an_upload_is_staged_as_atlas_new_and_made_executable() {
        let sh = stage_upload_script();
        assert!(sh.contains("tee /usr/local/lib/atlas-crew/atlas.new"));
        assert!(sh.contains("chmod +x /usr/local/lib/atlas-crew/atlas.new"));
        assert!(!sh.contains("mv "), "{sh}");
    }

    // The script prints what the health wait saw; the CLI turns that into `Health` and then `verdict`.
    #[test]
    fn the_health_line_of_the_script_is_read_back() {
        assert_eq!(
            parse_health("hostd: 1.6.0\nupdate: ok 1.6.0\n"),
            Health::Up("1.6.0".into())
        );
        // After a rollback the last answer is the old binary's: that is the one that counts.
        assert_eq!(
            parse_health("hostd: 1.5.0\nupdate: rolled-back\n"),
            Health::Up("1.5.0".into())
        );
        assert_eq!(parse_health(""), Health::Down);
        assert_eq!(parse_health("update: rollback-failed\n"), Health::Down);
    }

    // What the person is told after a clean run: nothing odd, or the warning of a hostd on another version.
    #[test]
    fn a_clean_run_reports_a_warning_only_for_a_version_mismatch() {
        assert_eq!(
            report("hostd: 1.6.0\nupdate: ok 1.6.0\n", Some("1.6.0")),
            None
        );
        assert_eq!(report("hostd: 9.9.9\nupdate: ok 9.9.9\n", None), None);
        let w = report("hostd: 1.5.0\nupdate: ok 1.5.0\n", Some("1.6.0")).unwrap();
        assert!(w.contains("1.5.0") && w.contains("1.6.0"), "{w}");
    }

    // The decision after the restart.
    #[test]
    fn a_hostd_that_answers_with_the_installed_version_is_kept() {
        assert_eq!(
            verdict(&Health::Up("1.6.0".into()), Some("1.6.0")),
            Verdict::Keep
        );
    }

    #[test]
    fn a_hostd_that_does_not_answer_is_rolled_back() {
        assert_eq!(verdict(&Health::Down, Some("1.6.0")), Verdict::RollBack);
        assert_eq!(verdict(&Health::Down, None), Verdict::RollBack);
    }

    // An uploaded binary has no known version: any answer is healthy.
    #[test]
    fn an_uploaded_binary_is_healthy_when_hostd_answers() {
        assert_eq!(verdict(&Health::Up("9.9.9".into()), None), Verdict::Keep);
    }

    // hostd up but on another version (an old process kept the socket): not a rollback (it answers), but say so.
    #[test]
    fn a_hostd_on_another_version_is_kept_with_a_warning() {
        match verdict(&Health::Up("1.5.0".into()), Some("1.6.0")) {
            Verdict::KeepWithWarning(w) => {
                assert!(w.contains("1.5.0") && w.contains("1.6.0"), "{w}")
            }
            other => panic!("{other:?}"),
        }
    }

    // The shell does the work in one session, so a dropped ssh link cannot leave a half update: backup,
    // swap, restart, wait, rollback on failure, in that order.
    #[test]
    fn the_swap_backs_up_restarts_waits_and_rolls_back_in_order() {
        let sh = swap_script(60);
        let at = |needle: &str| {
            sh.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}: {sh}"))
        };
        let backup =
            at("sudo cp -p /usr/local/lib/atlas-crew/atlas /usr/local/lib/atlas-crew/atlas.prev");
        let swap =
            at("sudo mv -f /usr/local/lib/atlas-crew/atlas.new /usr/local/lib/atlas-crew/atlas");
        let restart = at("if sudo systemctl restart atlas-crew && healthy");
        let restore =
            at("sudo mv -f /usr/local/lib/atlas-crew/atlas.prev /usr/local/lib/atlas-crew/atlas");
        let rollback = at("update: rolled-back");
        assert!(
            backup < swap && swap < restart && restart < restore && restore < rollback,
            "{sh}"
        );
        // The wait is a function defined before use, polling hostd --status.
        assert!(at("healthy() {") < backup && at("hostd --status") < backup);
        // The previous binary is copied before it is replaced, and restored on failure.
        assert!(sh.contains(
            "cp -p /usr/local/lib/atlas-crew/atlas /usr/local/lib/atlas-crew/atlas.prev"
        ));
        assert!(
            sh.contains(
                "mv -f /usr/local/lib/atlas-crew/atlas.prev /usr/local/lib/atlas-crew/atlas"
            ) || sh.contains(
                "cp -p /usr/local/lib/atlas-crew/atlas.prev /usr/local/lib/atlas-crew/atlas"
            )
        );
        // The health wait asks as the crew user, in its data dir, with the given timeout.
        assert!(sh.contains("sudo -u \"$u\""));
        assert!(sh.contains("--dir \"/var/lib/$u/.atlas\""));
        assert!(sh.contains("60"));
        // Data and the paused flag live in the crew home: the script never goes near it.
        for never in [
            "rm -rf",
            "userdel",
            "--crew-pause",
            "--crew-resume",
            "crew.json",
            "/var/lib/atlas",
        ] {
            assert!(!sh.contains(never), "{never} in {sh}");
        }
    }

    #[test]
    fn the_swap_reports_a_rollback_with_its_own_exit_code() {
        let sh = swap_script(30);
        assert!(sh.contains("exit 3"), "{sh}");
        // Rolled back but the old binary did not come up either: a distinct, louder code.
        assert!(sh.contains("exit 4"), "{sh}");
        assert!(sh.contains("update: ok"));
    }

    #[test]
    fn the_swap_is_one_failure_path_when_the_staged_binary_is_missing() {
        let sh = swap_script(30);
        assert!(
            sh.contains("test -x /usr/local/lib/atlas-crew/atlas.new"),
            "{sh}"
        );
        // Checked before the current binary is touched.
        assert!(sh.find("test -x").unwrap() < sh.find("cp -p").unwrap());
    }

    // `--rollback`: restore what the last update kept, wait for hostd, and fail loudly with no backup.
    #[test]
    fn rollback_restores_atlas_prev_and_waits_for_hostd() {
        let sh = rollback_script(45);
        assert!(
            sh.contains("test -x /usr/local/lib/atlas-crew/atlas.prev"),
            "{sh}"
        );
        assert!(sh.contains("no previous binary"), "{sh}");
        assert!(sh.contains("systemctl restart atlas-crew"));
        assert!(sh.contains("hostd --status"));
        assert!(sh.contains("45"));
        assert!(sh.find("atlas.prev").unwrap() < sh.find("restart").unwrap());
        for never in [
            "rm -rf",
            "userdel",
            "--crew-pause",
            "--crew-resume",
            "crew.json",
        ] {
            assert!(!sh.contains(never), "{never} in {sh}");
        }
    }

    // A rollback keeps the binary it replaced as atlas.prev, so `--rollback` twice flips back.
    #[test]
    fn rollback_keeps_the_replaced_binary_as_the_new_prev() {
        let sh = rollback_script(45);
        assert!(sh.contains("atlas.swap"), "{sh}");
    }

    // `status` shows whether the server is behind the CLI.
    #[test]
    fn freshness_compares_the_servers_version_with_the_clis() {
        assert_eq!(freshness(Some("1.5.0"), "1.6.0"), Freshness::Behind);
        assert_eq!(freshness(Some("1.6.0"), "1.6.0"), Freshness::Current);
        assert_eq!(freshness(Some("1.7.0"), "1.6.0"), Freshness::Ahead);
        // Semver precedence, not string order: 1.10 > 1.9, a prerelease is below its release.
        assert_eq!(freshness(Some("1.9.0"), "1.10.0"), Freshness::Behind);
        assert_eq!(freshness(Some("1.6.0-alpha.3"), "1.6.0"), Freshness::Behind);
        assert_eq!(
            freshness(Some("1.6.0-alpha.3"), "1.6.0-alpha.10"),
            Freshness::Behind
        );
    }

    #[test]
    fn an_unreadable_server_version_is_unknown_not_current() {
        assert_eq!(freshness(None, "1.6.0"), Freshness::Unknown);
        assert_eq!(freshness(Some("nonsense"), "1.6.0"), Freshness::Unknown);
        assert_eq!(freshness(Some(""), "1.6.0"), Freshness::Unknown);
    }

    #[test]
    fn the_version_is_read_from_the_hostd_status_json() {
        let pretty = "{\n  \"version\": \"1.6.0-alpha.2\",\n  \"proto\": 4,\n  \"pid\": 12\n}";
        assert_eq!(
            version_from_status(pretty).as_deref(),
            Some("1.6.0-alpha.2")
        );
        assert_eq!(
            version_from_status("{\"version\":\"1.0.0\"}").as_deref(),
            Some("1.0.0")
        );
        // `status` appends `2>&1 || true`: an error text or nothing is no version.
        assert_eq!(version_from_status("atlas hostd is not running (/x)"), None);
        assert_eq!(version_from_status(""), None);
        assert_eq!(version_from_status("{\"proto\":4}"), None);
    }

    #[test]
    fn the_status_line_says_whether_to_update() {
        assert_eq!(
            version_line(Some("1.6.0"), "1.6.0"),
            "atlas 1.6.0 (up to date)"
        );
        let behind = version_line(Some("1.5.0"), "1.6.0");
        assert!(
            behind.contains("1.5.0") && behind.contains("behind") && behind.contains("1.6.0"),
            "{behind}"
        );
        assert!(behind.contains("atlas crew update"), "{behind}");
        let ahead = version_line(Some("1.7.0"), "1.6.0");
        assert!(
            ahead.contains("1.7.0") && ahead.contains("ahead"),
            "{ahead}"
        );
        let unknown = version_line(None, "1.6.0");
        assert!(unknown.contains("unknown"), "{unknown}");
    }
}
