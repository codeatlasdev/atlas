//! Updating Atlas from a phone, end to end: a real hostd, a real paired phone over iroh, and a fake
//! `atlas` binary standing in for the CLI (`ServeOpts::atlas_exe`). The fake answers `update --check
//! --json` from a file and, for `update --yes`, prints the `@@atlas` lines the real CLI prints and
//! exits with the code it was given, so what is under test is hostd's side: scopes, one at a time,
//! the audit line, the order of the events, and what the phone is told on failure.
//!
//! The fixtures under `apps/mobile/test/fixtures/` (`info_update.json`, `atlas_update_check.json`,
//! `atlas_update_events.json`) are written from these real payloads:
//! `UPDATE_FIXTURES=1 cargo test -p atlas-host update_tests`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::tests::{Phone, pair_start};
use crate::HostClient;

/// What a real `atlas update --yes` prints on its way to a handoff (`update_core::phase`, `host_event`).
const PHASES: &str = "◆ Downloading v1.6.0...\n@@atlas phase download\n@@atlas bytes 10 100\n@@atlas phase verify\n\
@@atlas phase install-cli\n@@atlas phase update-app\n@@atlas phase terminal-host\n@@atlas host upgraded 2\n";

/// A stand-in `atlas`: `dir/atlas` plus the files that steer it.
struct Fake {
    dir: PathBuf,
}

impl Fake {
    fn new(root: &Path) -> Self {
        let dir = root.join("fake");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("atlas");
        std::fs::write(
            &exe,
            r#"#!/bin/sh
d="$(dirname "$0")"
case "$*" in
  "update --check --json")
    echo "check" >> "$d/calls"
    if [ -f "$d/check_error" ]; then cat "$d/check_error" >&2; exit 1; fi
    cat "$d/check.json"; exit 0;;
  "update --yes")
    echo "update --yes progress=$ATLAS_UPDATE_PROGRESS dir=$ATLAS_HOSTD_DIR" >> "$d/calls"
    while [ -f "$d/gate" ]; do sleep 0.05; done
    cat "$d/stdout"
    [ -f "$d/stderr" ] && cat "$d/stderr" >&2
    exit "$(cat "$d/exit")";;
esac
echo "unexpected: $*" >> "$d/calls"
exit 64
"#,
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let fake = Self { dir };
        fake.report("update");
        fake.run(PHASES, 0);
        fake
    }

    fn exe(&self) -> PathBuf {
        self.dir.join("atlas")
    }

    /// What `atlas update --check --json` prints, in the real CLI's shape: an `update` offer of 1.6.0
    /// over the 1.5.0 that runs; `managed` and `none` are what it says for the other outcomes.
    fn report(&self, action: &str) {
        let report = match action {
            "update" => {
                json!({"installed": "1.5.0", "channel": "alpha", "offered": "1.6.0", "tag": "v1.6.0", "action": "update"})
            }
            "managed" => {
                json!({"installed": "1.5.0", "channel": "alpha", "offered": "1.6.0", "tag": "v1.6.0", "action": "managed", "managed": true, "manager": null, "command": "brew upgrade atlas"})
            }
            _ => {
                json!({"installed": "1.5.0", "channel": "alpha", "offered": null, "tag": "v1.5.0", "action": "none"})
            }
        };
        std::fs::write(self.dir.join("check.json"), report.to_string()).unwrap();
    }

    /// What `atlas update --yes` prints to stdout, and its exit code.
    fn run(&self, stdout: &str, exit: i32) {
        std::fs::write(self.dir.join("stdout"), stdout).unwrap();
        std::fs::write(self.dir.join("exit"), exit.to_string()).unwrap();
    }

    fn stderr(&self, text: &str) {
        std::fs::write(self.dir.join("stderr"), text).unwrap();
    }

    /// `update --yes` waits until [`release`](Self::release).
    fn hold(&self) {
        std::fs::write(self.dir.join("gate"), "").unwrap();
    }

    fn release(&self) {
        std::fs::remove_file(self.dir.join("gate")).unwrap();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn installs(&self) -> usize {
        self.calls()
            .iter()
            .filter(|c| c.starts_with("update --yes"))
            .count()
    }
}

async fn daemon(dir: &Path, fake: &Fake) -> HostClient {
    let d = dir.to_path_buf();
    let opts = crate::server::ServeOpts {
        no_mobile_relay: true,
        mobile_loopback: true,
        atlas_exe: Some(fake.exe()),
        desktop: Some(atlas_core::desktop_ctl::Endpoint {
            token: dir.join("no-desktop.token"),
            socket: dir.join("no-desktop.sock"),
        }),
        ..Default::default()
    };
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    connect(dir).await
}

async fn connect(dir: &Path) -> HostClient {
    for _ in 0..100 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start");
}

/// A paired phone and its node id.
async fn pair(c: &HostClient) -> (Phone, String) {
    let info = pair_start(c).await;
    let mut phone = Phone::dial(&info).await;
    let r = phone
        .call("pair", json!({"code": info["code"], "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let node = c.call("mobile_devices", json!({})).await.unwrap()["devices"][0]["node_id"]
        .as_str()
        .unwrap()
        .to_string();
    (phone, node)
}

fn stages(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .filter(|e| e["ev"] == "atlas_update")
        .map(|e| e["stage"].as_str().unwrap())
        .collect()
}

fn ended(e: &Value) -> bool {
    e["ev"] == "atlas_update" && matches!(e["stage"].as_str(), Some("done" | "failed"))
}

/// The contract with the phone: its tests read these files, written from the real payloads.
fn fixture(name: &str, live: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/mobile/test/fixtures")
        .join(name);
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{name} is missing: UPDATE_FIXTURES=1 cargo test -p atlas-host update_tests")
        }))
        .unwrap();
    assert_eq!(
        &stored, live,
        "{name} changed: rerun with UPDATE_FIXTURES=1 and update the phone"
    );
}

/// `info.update` is the CLI's cache, read without the network: absent with no check on record, absent
/// when the user switched channel since (that offer is another channel's), the offer otherwise.
#[tokio::test]
async fn info_carries_the_update_the_cli_last_found_and_no_more() {
    // The user's own channel override would change what "followed" means here.
    if std::env::var_os(atlas_core::channel::ENV).is_some() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let fake = Fake::new(tmp.path());
    let c = daemon(tmp.path(), &fake).await;
    let info = c.call("info", json!({})).await.unwrap();
    assert!(info.get("update").is_none(), "no check yet: {info}");
    assert!(info["caps"].as_array().unwrap().contains(&json!("update")));

    let cache = tmp.path().join(".update_check");
    std::fs::write(
        &cache,
        r#"{"last_checked":1700000000,"available_version":"99.0.0","channel":"alpha"}"#,
    )
    .unwrap();
    let info = c.call("info", json!({})).await.unwrap();
    let mut update = info["update"].clone();
    assert_eq!(
        update,
        json!({
            "installed": env!("CARGO_PKG_VERSION"),
            "offered": "99.0.0",
            "channel": "alpha",
            "action": "update",
            "checked_at": 1700000000,
        }),
        "{info}"
    );
    // The running version moves with every release: the fixture pins the shape, not that.
    update["installed"] = json!("1.5.0");
    fixture("info_update.json", &update);

    // A check that found nothing newer is still a check: current, not absent.
    std::fs::write(
        &cache,
        r#"{"last_checked":1700000100,"available_version":null,"channel":"alpha"}"#,
    )
    .unwrap();
    let info = c.call("info", json!({})).await.unwrap();
    assert_eq!(info["update"]["action"], "none", "{info}");
    assert_eq!(info["update"]["offered"], Value::Null, "{info}");

    // The channel was switched since: the offer on record is another channel's.
    std::fs::write(tmp.path().join("update-channel"), "stable\n").unwrap();
    let info = c.call("info", json!({})).await.unwrap();
    assert!(info.get("update").is_none(), "{info}");
    c.call("shutdown", json!({})).await.ok();
}

/// `atlas_update_check` is the CLI's own check (read-level), answered as the CLI printed it; a check
/// that fails says why in the CLI's words.
#[tokio::test]
async fn a_read_phone_checks_for_updates_through_the_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = Fake::new(tmp.path());
    let c = daemon(tmp.path(), &fake).await;
    let (mut phone, node) = pair(&c).await;
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read"]}),
    )
    .await
    .unwrap();

    let r = phone.call("atlas_update_check", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["action"], "update");
    assert_eq!(r["data"]["offered"], "1.6.0");
    fixture("atlas_update_check.json", &r["data"]);
    assert_eq!(fake.calls(), ["check"]);

    std::fs::write(
        fake.dir.join("check_error"),
        "\x1b[1;31merror:\x1b[0m Could not check for updates. Check your network connection (needs curl).\n",
    )
    .unwrap();
    let r = phone.call("atlas_update_check", json!({})).await.unwrap();
    assert_eq!(
        r["error"], "Could not check for updates. Check your network connection (needs curl).",
        "{r}"
    );
    c.call("shutdown", json!({})).await.ok();
}

/// Installing replaces the binary on the computer: `act`, not `read`. A phone without it starts nothing.
#[tokio::test]
async fn a_phone_without_act_cannot_start_an_update() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = Fake::new(tmp.path());
    let c = daemon(tmp.path(), &fake).await;
    let (mut phone, node) = pair(&c).await;
    c.call(
        "mobile_set_scopes",
        json!({"node_id": node, "scopes": ["read", "term_read"]}),
    )
    .await
    .unwrap();
    let r = phone.call("atlas_update", json!({})).await.unwrap();
    assert_eq!(r["error"], "forbidden: act", "{r}");
    assert!(fake.calls().is_empty(), "nothing ran: {:?}", fake.calls());
    // The audit has no `atlas_update_started`: the refusal is before anything starts.
    let audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap_or_default();
    assert!(!audit.contains("atlas_update_started"), "{audit}");
    c.call("shutdown", json!({})).await.ok();
}

/// The whole run, as the phone sees it: one at a time (a second tap is refused while the first
/// runs), an audit line naming who started it, the stages in order ending in `done`, the update
/// started detached with the CLI's progress protocol on, and a failure after it reported with the
/// CLI's own message. Nothing stays locked after either ending.
#[tokio::test]
async fn an_update_runs_once_streams_its_stages_in_order_and_reports_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = Fake::new(tmp.path());
    let c = daemon(tmp.path(), &fake).await;
    let (mut phone, node) = pair(&c).await;

    // ── a run that succeeds ──
    fake.hold();
    let mut events = Vec::new();
    let (r, pushed) = phone
        .call_with_pushes("atlas_update", json!({}))
        .await
        .unwrap();
    events.extend(pushed);
    assert_eq!(
        r["data"],
        json!({"started": true, "from": "1.5.0", "to": "1.6.0"}),
        "{r}"
    );
    // Held at the gate: the daemon says an update runs and starts no second one.
    let (second, pushed) = phone
        .call_with_pushes("atlas_update", json!({}))
        .await
        .unwrap();
    events.extend(pushed);
    assert_eq!(second["error"], "an update is already running", "{second}");
    fake.release();
    events.extend(phone.pushes_until(10_000, ended).await);
    assert_eq!(
        stages(&events),
        ["downloading", "installing", "handing_off", "done"],
        "{events:?}"
    );
    assert!(events.iter().all(|e| e["version"] == "1.6.0"), "{events:?}");
    assert_eq!(
        events.last().unwrap()["message"],
        "Atlas v1.6.0 is installed"
    );
    assert_eq!(fake.installs(), 1, "the refused tap started nothing");
    // The progress lines on, and hostd's own directory named: the handoff at the end of a real
    // update has to find this daemon.
    let call = fake
        .calls()
        .into_iter()
        .find(|c| c.starts_with("update --yes"))
        .unwrap();
    assert_eq!(
        call,
        format!("update --yes progress=1 dir={}", tmp.path().display())
    );
    // Who started it is on the record, with what it went from and to.
    let audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap();
    let line: Value = audit
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|l| l["cmd"] == "atlas_update_started")
        .unwrap_or_else(|| panic!("no audit line: {audit}"));
    assert_eq!(line["node_id"], node);
    assert_eq!(
        line["args"],
        json!({"by": format!("phone:{node}"), "from": "1.5.0", "to": "1.6.0"})
    );

    // ── the same phone again: a run that fails ──
    fake.run("@@atlas phase download\n", 1);
    fake.stderr("\x1b[1;31merror:\x1b[0m Download failed. Releases: https://github.com/codeatlasdev/atlas/releases\n  caused by: curl exited with 22\n");
    let (r, mut failed) = phone
        .call_with_pushes("atlas_update", json!({}))
        .await
        .unwrap();
    assert_eq!(
        r["ok"], true,
        "nothing stayed locked after the first run: {r}"
    );
    failed.extend(phone.pushes_until(10_000, ended).await);
    assert_eq!(stages(&failed), ["downloading", "failed"], "{failed:?}");
    assert_eq!(
        failed.last().unwrap()["message"],
        "Download failed. Releases: https://github.com/codeatlasdev/atlas/releases"
    );
    fixture(
        "atlas_update_events.json",
        &json!({"success": events, "failed": failed}),
    );

    // ── a run another tool owns ends `failed` even though the CLI exits 0 ──
    fake.run("@@atlas managed homebrew\n", 0);
    fake.stderr("");
    let (r, mut managed) = phone
        .call_with_pushes("atlas_update", json!({}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    managed.extend(phone.pushes_until(10_000, ended).await);
    assert_eq!(stages(&managed), ["downloading", "failed"], "{managed:?}");
    c.call("shutdown", json!({})).await.ok();
}

/// A check that says there is nothing to install here (Homebrew or a package manager owns it, it is
/// current) is an error with the CLI's reason, before anything starts, and leaves nothing locked.
#[tokio::test]
async fn an_update_that_cannot_start_here_says_why_and_starts_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = Fake::new(tmp.path());
    let c = daemon(tmp.path(), &fake).await;
    let (mut phone, _node) = pair(&c).await;

    fake.report("managed");
    let r = phone.call("atlas_update", json!({})).await.unwrap();
    let why = r["error"].as_str().unwrap_or_default();
    assert!(
        why.contains("Homebrew") && why.contains("`brew upgrade atlas`"),
        "{r}"
    );

    fake.report("none");
    let r = phone.call("atlas_update", json!({})).await.unwrap();
    assert_eq!(r["error"], "Atlas v1.5.0 is up to date", "{r}");
    assert_eq!(fake.installs(), 0);
    let audit = std::fs::read_to_string(tmp.path().join("mobile/audit.log")).unwrap_or_default();
    assert!(!audit.contains("atlas_update_started"), "{audit}");

    // Neither refusal left the lock taken: an update that can start does.
    fake.report("update");
    let r = phone.call("atlas_update", json!({})).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    c.call("shutdown", json!({})).await.ok();
}

// ── idle exit ──────────────────────────────────────────────────────────────────

/// A daemon that leaves after `limit` of nothing, and the task that runs it (finished = it exited).
async fn daemon_that_idles(
    dir: &Path,
    limit: Duration,
) -> (HostClient, tokio::task::JoinHandle<anyhow::Result<()>>) {
    let d = dir.to_path_buf();
    let opts = crate::server::ServeOpts {
        no_mobile_relay: true,
        mobile_loopback: true,
        idle: Some(limit),
        desktop: Some(atlas_core::desktop_ctl::Endpoint {
            token: dir.join("no-desktop.token"),
            socket: dir.join("no-desktop.sock"),
        }),
        ..Default::default()
    };
    let task = tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    (connect(dir).await, task)
}

/// The phone goes away (the app is killed) and hostd has seen it: only `c`'s own connection is left.
/// Dropping the phone right after its hang-up would close its endpoint before the close frame left,
/// and hostd would keep the connection until its idle timeout.
async fn leave(c: &HostClient, phone: Phone) {
    phone.hang_up();
    for _ in 0..100 {
        if c.call("info", json!({})).await.unwrap()["clients"] == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd never noticed the phone leave");
}

/// Incident: the desktop closed, hostd left 10 minutes later, and the paired phone showed "Can't reach
/// Mac" until someone opened Atlas Desktop. Nothing runs, nobody is connected: only the pairing
/// keeps it. Control: the same daemon with the phone revoked does leave, so the wait below is real.
#[tokio::test]
async fn a_daemon_with_a_paired_phone_does_not_idle_out_and_one_without_does() {
    let limit = Duration::from_secs(1);

    let kept = tempfile::tempdir().unwrap();
    let (c, kept_task) = daemon_that_idles(kept.path(), limit).await;
    let (phone, _node) = pair(&c).await;
    leave(&c, phone).await;
    drop(c);

    let revoked = tempfile::tempdir().unwrap();
    let (c, revoked_task) = daemon_that_idles(revoked.path(), limit).await;
    let (phone, node) = pair(&c).await;
    c.call("mobile_revoke", json!({"node_id": node}))
        .await
        .unwrap();
    leave(&c, phone).await;
    drop(c);

    // The revoked one goes (a few ticks of half the limit); the paired one is still here as long after.
    let gone = tokio::time::timeout(Duration::from_secs(10), revoked_task).await;
    assert!(gone.is_ok(), "a daemon nobody can reach never exited");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !kept_task.is_finished(),
        "the daemon left with a phone paired and nothing else running"
    );
    let c = connect(kept.path()).await;
    assert_eq!(
        c.call("info", json!({})).await.unwrap()["proto"],
        json!(crate::protocol::PROTO)
    );
    c.call("shutdown", json!({})).await.ok();
}

// ── scopes and routes (no daemon needed) ──────────────────────────────────────

/// Reading what is on offer and its progress is `read`; replacing the binary is `act`; and an update is
/// never started through a Mac's relay (it would be announced and audited as the Mac), nor on the lab.
#[test]
fn update_scopes_and_where_each_command_is_refused() {
    use super::{Scope, push_scope, required_scope};
    use crate::protocol::HostCmd;

    assert_eq!(
        required_scope(&HostCmd::AtlasUpdateCheck {}),
        Ok(Scope::Read)
    );
    assert_eq!(required_scope(&HostCmd::AtlasUpdate {}), Ok(Scope::Act));
    assert_eq!(push_scope("atlas_update"), Some(Scope::Read));

    let relayed = |cmd: &str| HostCmd::CrewCall {
        server: "box".into(),
        cmd: cmd.into(),
        args: json!({}),
    };
    assert_eq!(
        required_scope(&relayed("atlas_update")),
        Err("not available on mobile")
    );
    assert_eq!(
        required_scope(&relayed("atlas_update_check")),
        Ok(Scope::Read),
        "looking is harmless through the relay"
    );

    use crate::lab::{Route, route};
    assert!(matches!(route(&HostCmd::AtlasUpdate {}), Route::Refuse(_)));
    assert!(matches!(route(&HostCmd::AtlasUpdateCheck {}), Route::Local));
}
