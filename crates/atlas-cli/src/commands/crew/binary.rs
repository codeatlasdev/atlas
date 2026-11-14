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
/// verified binary at `<INSTALL_DIR>/atlas.new`. Nothing replaces `atlas` here.
pub fn download_script(version: &str, target: &str) -> String {
    let url = asset_url(version, target);
    format!(
        r#"sudo mkdir -p {d} &&
tmpf=$(mktemp /tmp/atlas-crew-XXXXXX.tar.gz) &&
curl -fsSL {url_q} -o "$tmpf" &&
WANT=$(curl -fsSL {sha_q} | awk '{{print $1}}') &&
GOT=$(sha256sum "$tmpf" | awk '{{print $1}}') &&
{{ [ "$WANT" = "$GOT" ] || {{ echo 'checksum mismatch' >&2; rm -f "$tmpf"; exit 1; }}; }} &&
sudo tar -xzf "$tmpf" -O atlas | sudo tee {d}/atlas.new > /dev/null &&
sudo chmod +x {d}/atlas.new &&
rm -f "$tmpf""#,
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

/// Shell for a fresh install: move the staged `atlas.new` over `atlas` (the unit is written and started after).
pub fn install_staged_script() -> String {
    format!(
        "test -x {d}/atlas.new && sudo mv -f {d}/atlas.new {d}/atlas",
        d = INSTALL_DIR
    )
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
/// wait up to `timeout_s` for hostd and, if it does not answer, restore `atlas.prev` and restart.
/// Prints `update: ok <version>` or `update: rolled-back` and exits 0 / 3 (rolled back) / 4 (rollback failed).
/// Only the binary dir is touched: the crew home (data, secrets, `crew.json` with the paused flag) never is,
/// so a paused crew stays paused.
pub fn swap_script(timeout_s: u32) -> String {
    let d = INSTALL_DIR;
    format!(
        r#"u=$({WHICH_USER}); {health}
test -x {d}/atlas.new || {{ echo 'no staged binary at {d}/atlas.new' >&2; exit 2; }}
if [ -x {d}/atlas ]; then sudo cp -p {d}/atlas {d}/atlas.prev || exit 2; fi
sudo mv -f {d}/atlas.new {d}/atlas || exit 2
if {restart}; then echo "update: ok $v"; exit 0; fi
echo 'hostd did not come back: restoring the previous binary' >&2
if [ -x {d}/atlas.prev ]; then
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

    // Install stages like update does, then moves the verified binary into place: one download path for both.
    #[test]
    fn a_fresh_install_moves_the_staged_binary_into_place() {
        let sh = install_staged_script();
        assert!(
            sh.contains("test -x /usr/local/lib/atlas-crew/atlas.new"),
            "{sh}"
        );
        assert!(
            sh.contains(
                "mv -f /usr/local/lib/atlas-crew/atlas.new /usr/local/lib/atlas-crew/atlas"
            ),
            "{sh}"
        );
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
