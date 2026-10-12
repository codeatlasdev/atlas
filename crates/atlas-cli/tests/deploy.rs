//! End-to-end `atlas deploy` against `host: local` (needs GNU coreutils +
//! util-linux flock, so Linux only). macOS: run in docker, see docs/deploy.md.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Fx {
    _tmp: tempfile::TempDir,
    repo: PathBuf,
    srv: PathBuf,
}

fn sh(dir: &Path, cmd: &str) {
    let st = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(st.success(), "{cmd}");
}

impl Fx {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let srv = tmp.path().join("srv");
        fs::create_dir_all(&repo).unwrap();
        let fx = Self {
            _tmp: tmp,
            repo,
            srv,
        };
        sh(
            &fx.repo,
            "git init -q && git config user.email t@t && git config user.name t",
        );
        fx.commit("date >> restarts");
        fx
    }

    fn commit(&self, body: &str) {
        fs::write(self.repo.join("start.sh"), body).unwrap();
        let restart = "bash start.sh";
        let yaml = format!(
            "name: demo\nservices:\n  app:\n    command: 'true'\ndeploy:\n  targets:\n    t:\n      host: local\n      dir: {}\n      soak: 1\n      health_passes: 1\n      health_timeout: 5\n      min_free_mb: 1\n      services:\n        - {{ name: app, kind: command, restart: {} }}\n",
            self.srv.display(),
            serde_json::to_string(&restart).unwrap()
        );
        fs::write(self.repo.join("atlas.yaml"), yaml).unwrap();
        sh(&self.repo, "git add -A && git commit -qm c --allow-empty");
    }

    fn atlas(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_atlas"))
            .arg("deploy")
            .args(args)
            .arg("--dir")
            .arg(&self.repo)
            .output()
            .unwrap()
    }

    fn deploy_json(&self, extra: &[&str]) -> (i32, Value) {
        let mut a = vec!["--json"];
        a.extend_from_slice(extra);
        let out = self.atlas(&a);
        let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "no json; stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.code().unwrap(), v)
    }

    fn current(&self) -> String {
        fs::read_link(self.srv.join("current"))
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into()
    }

    fn status_of(&self, id: &str) -> String {
        let m =
            fs::read_to_string(self.srv.join("releases").join(id).join(".atlas-release")).unwrap();
        let v: Value = serde_json::from_str(&m).unwrap();
        v["status"].as_str().unwrap().into()
    }
}

#[test]
fn deploy_lifecycle() {
    let fx = Fx::new();

    // 1. first deploy
    let (code, v) = fx.deploy_json(&[]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["ok"], true);
    let first = v["release"].as_str().unwrap().to_string();
    assert_eq!(fx.current(), first);
    assert_eq!(fx.status_of(&first), "healthy");
    let audit = fs::read_to_string(fx.srv.join(".atlas/deploys.jsonl")).unwrap();
    assert_eq!(audit.lines().count(), 1);

    // 2. same HEAD → noop; --force → new release
    let (code, v) = fx.deploy_json(&[]);
    assert_eq!((code, v["phase"].as_str()), (0, Some("noop")), "{v}");
    std::thread::sleep(Duration::from_millis(1100)); // ids have 1s resolution
    let (code, v) = fx.deploy_json(&["--force"]);
    assert_eq!(code, 0, "{v}");
    let second = v["release"].as_str().unwrap().to_string();
    assert_ne!(second, first);
    assert_eq!(fx.current(), second);

    // 3. failing restart → rolled back to last healthy, new release marked failed
    fx.commit("exit 1");
    std::thread::sleep(Duration::from_millis(1100));
    let (code, v) = fx.deploy_json(&[]);
    assert_eq!(code, 1, "{v}");
    assert_eq!(v["rolled_back"], true);
    let bad = v["release"].as_str().unwrap().to_string();
    assert_eq!(fx.current(), second);
    assert_eq!(fx.status_of(&bad), "failed");

    // 4. manual rollback skips the failed release → first
    fx.commit("date >> restarts");
    let out = fx.atlas(&["rollback", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fx.current(), first);

    let out = fx.atlas(&["rollback", "--to", "../x"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid release id"));

    // 5. held lock → exit 3, LOCKED
    let mut holder = Command::new("flock")
        .arg("-o")
        .arg(fx.srv.join(".atlas/lock"))
        .args(["sleep", "30"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let out = fx.atlas(&["--force"]);
    holder.kill().ok();
    holder.wait().ok();
    assert_eq!(out.status.code(), Some(3));
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(all.contains("LOCKED"), "{all}");

    // 6. interrupted transaction: current points at a failed release, state says restarting
    let fake = "20000101000000-0000000";
    let fake_dir = fx.srv.join("releases").join(fake);
    fs::create_dir_all(&fake_dir).unwrap();
    fs::write(
        fake_dir.join(".atlas-release"),
        r#"{"id":"20000101000000-0000000","sha":"0","ts":"","user":"","status":"failed"}"#,
    )
    .unwrap();
    sh(&fx.srv, &format!("ln -sfn releases/{fake} current"));
    fs::write(
        fx.srv.join(".atlas/state.json"),
        format!(r#"{{"release":"{fake}","phase":"restarting","pid":999999}}"#),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(1100));
    let out = fx.atlas(&["--force"]);
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{all}");
    assert!(all.contains("recovering interrupted deploy"), "{all}");
    assert_eq!(fx.status_of(&fx.current()), "healthy");

    // 7. dirty tree
    fs::write(fx.repo.join("dirty.txt"), "x").unwrap();
    let out = fx.atlas(&[]);
    assert_ne!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("working tree dirty"));
    fs::remove_file(fx.repo.join("dirty.txt")).unwrap();

    // 8. --no-follow returns before the driver finishes; status turns healthy
    fx.commit("sleep 2; date >> restarts");
    std::thread::sleep(Duration::from_millis(1100));
    let t0 = Instant::now();
    let out = fx.atlas(&["--no-follow"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t0.elapsed() < Duration::from_secs(3));
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let st = fx.atlas(&["status", "--json"]);
        let v: Value = serde_json::from_slice(&st.stdout).unwrap();
        if st.status.code() == Some(0) && v["current"] == id.as_str() {
            break;
        }
        assert!(Instant::now() < deadline, "never healthy: {v}");
        std::thread::sleep(Duration::from_millis(500));
    }
}
