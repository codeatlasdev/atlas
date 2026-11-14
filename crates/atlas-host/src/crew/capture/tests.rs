//! The capture contract with a fake runner (a shell script that prints a canned report, so no
//! browser is needed), the MCP tool round trip through a real hostd, and one real Playwright run
//! that skips, saying why, where node or the browsers are absent.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::*;

/// A fake runner. `body` runs with `$OUT` set to the spec's `out_dir`; whatever it prints is the
/// "report". `png` makes an `$OUT/<engine>-<width>-<theme>.png` for each cell named in it.
pub(crate) fn fake_runner(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join("fake-runner.sh");
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\nSPEC=$(cat)\n\
             OUT=$(printf '%s' \"$SPEC\" | sed -n 's/.*\"out_dir\":\"\\([^\"]*\\)\".*/\\1/p')\n\
             mkdir -p \"$OUT\"\n{body}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// Shell that writes a fake PNG for every combination and prints the report listing them all,
/// except the ones in `skip` (`engine-width-theme`).
pub(crate) fn full_report(skip: &[&str]) -> String {
    let mut sh = String::new();
    let mut entries = vec![];
    for e in ["chromium", "webkit"] {
        for w in [390, 1440] {
            for t in ["light", "dark"] {
                let key = format!("{e}-{w}-{t}");
                if skip.contains(&key.as_str()) {
                    continue;
                }
                sh.push_str(&format!(
                    "printf '\\211PNG\\r\\n\\032\\n{key}' > \"$OUT/{key}.png\"\n"
                ));
                entries.push(format!(
                    "{{\"engine\":\"{e}\",\"width\":{w},\"theme\":\"{t}\",\"path\":\"'\"$OUT\"'/{key}.png\"}}"
                ));
            }
        }
    }
    sh.push_str(&format!("printf '{{\"shots\":[{}]}}'\n", entries.join(",")));
    sh
}

fn use_runner(prog: &Path, timeout: Duration) {
    *TEST_RUNNER.lock() = Some((prog.to_path_buf(), timeout));
}

fn clear_runner() {
    *TEST_RUNNER.lock() = None;
}

fn spec() -> Spec {
    Spec::from_args(&json!({"url": "http://localhost:3000/"})).unwrap()
}

#[tokio::test]
async fn a_full_report_lists_every_combination_as_a_file_on_disk() {
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    use_runner(
        &fake_runner(tmp.path(), &full_report(&[])),
        Duration::from_secs(20),
    );
    let out = tmp.path().join("out");

    let got = run(tmp.path(), &spec(), &out).await;
    clear_runner();
    let got = got.unwrap();

    assert_eq!(got.files.len(), 8);
    let mut seen: Vec<String> = got.files.iter().map(|(c, _)| c.to_string()).collect();
    seen.sort();
    assert!(seen.contains(&"webkit/390px/dark".to_string()));
    assert!(seen.contains(&"chromium/1440px/light".to_string()));
    for (c, p) in &got.files {
        assert!(p.is_file(), "{c}: {} missing", p.display());
        assert!(p.starts_with(&out));
    }
}

#[tokio::test]
async fn a_missing_combination_is_an_error_naming_it() {
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    use_runner(
        &fake_runner(tmp.path(), &full_report(&["webkit-390-dark"])),
        Duration::from_secs(20),
    );
    let got = run(tmp.path(), &spec(), &tmp.path().join("out")).await;
    clear_runner();
    let e = got.unwrap_err();
    assert!(e.contains("Missing combinations: webkit/390px/dark"), "{e}");
}

#[tokio::test]
async fn a_report_pointing_at_a_file_that_was_never_written_is_an_error() {
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let body = "printf '{\"shots\":[{\"engine\":\"webkit\",\"width\":390,\"theme\":\"light\",\"path\":\"/nonexistent/x.png\"}]}'";
    use_runner(&fake_runner(tmp.path(), body), Duration::from_secs(20));
    let s = Spec::from_args(
        &json!({"url": "http://x/", "engines": ["webkit"], "widths": [390], "themes": ["light"]}),
    )
    .unwrap();
    let got = run(tmp.path(), &s, &tmp.path().join("out")).await;
    clear_runner();
    let e = got.unwrap_err();
    assert!(
        e.contains("webkit/390px/light") && e.contains("does not exist"),
        "{e}"
    );
}

#[tokio::test]
async fn a_runner_crash_says_why() {
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let body = "echo \"browserType.launch: Executable doesn't exist at /x/webkit\" >&2\nexit 3";
    use_runner(&fake_runner(tmp.path(), body), Duration::from_secs(20));
    let got = run(tmp.path(), &spec(), &tmp.path().join("out")).await;
    clear_runner();
    let e = got.unwrap_err();
    assert!(
        e.contains("exited with") && e.contains("Executable doesn't exist"),
        "{e}"
    );
}

#[tokio::test]
async fn garbage_on_stdout_is_an_error_that_quotes_it() {
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    use_runner(
        &fake_runner(tmp.path(), "echo 'not json at all'"),
        Duration::from_secs(20),
    );
    let got = run(tmp.path(), &spec(), &tmp.path().join("out")).await;
    clear_runner();
    assert!(got.unwrap_err().contains("not json at all"));
}

#[tokio::test]
async fn a_runner_that_hangs_is_stopped_at_the_timeout_and_killed() {
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    // The marker file proves the process died: a live one would write it after the sleep.
    let marker = tmp.path().join("survived");
    let body = format!("sleep 4\ntouch {}", marker.display());
    use_runner(&fake_runner(tmp.path(), &body), Duration::from_millis(400));
    let started = std::time::Instant::now();
    let got = run(tmp.path(), &spec(), &tmp.path().join("out")).await;
    clear_runner();
    let e = got.unwrap_err();
    assert!(e.contains("was stopped"), "{e}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "timeout was not enforced"
    );
    tokio::time::sleep(Duration::from_millis(4500)).await;
    assert!(!marker.exists(), "the runner outlived its timeout");
}

#[tokio::test]
async fn without_the_browsers_installed_the_error_says_how_to_install_them() {
    let _g = TEST_LOCK.lock().await;
    clear_runner();
    let tmp = tempfile::tempdir().unwrap();
    let e = run(tmp.path(), &spec(), &tmp.path().join("out"))
        .await
        .unwrap_err();
    assert!(e.contains("atlas crew browsers install"), "{e}");
}

/// A PNG as far as `attachments::sniff` is concerned: the signature, then `tag`.
fn png(tag: &str) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(tag.as_bytes());
    v
}

#[test]
fn attach_captures_stores_the_files_as_the_agents_attachments_with_their_names() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CrewStore::new(&tmp.path().join("hostd"));
    let mut task = crate::crew::store::Task::new("test", "look");
    task.id = "t1".into();
    store.append_task("c1", &task).unwrap();
    let src = tmp.path().join("shots");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.png"), png("A")).unwrap();
    std::fs::write(src.join("b.png"), png("B")).unwrap();

    let names =
        attach_captures(&store, "c1", "t1", &[src.join("a.png"), src.join("b.png")]).unwrap();

    assert_eq!(names, vec!["a.png", "b.png"]);
    let t = store.read_tasks("c1", 10).remove(0);
    assert_eq!(t.attachments.len(), 2);
    assert!(t.attachments.iter().all(|a| a.source == "agent"));
    let dir = crate::crew::attachments::task_dir(&store, "c1", "t1");
    assert_eq!(std::fs::read(dir.join("a.png")).unwrap(), png("A"));
    assert!(dir.ends_with("crew/c1/attachments/t1"));

    // A second capture of the same page adds to them, with names that do not collide.
    let again = attach_captures(&store, "c1", "t1", &[src.join("a.png")]).unwrap();
    assert_eq!(again, vec!["a-2.png"]);
    assert_eq!(store.read_tasks("c1", 10)[0].attachments.len(), 3);
}

// What is not an image by its bytes is refused: a capture runner that wrote garbage cannot put it on the task.
#[test]
fn attach_captures_refuses_a_file_that_is_not_an_image() {
    let tmp = tempfile::tempdir().unwrap();
    let store = CrewStore::new(&tmp.path().join("hostd"));
    let mut task = crate::crew::store::Task::new("test", "look");
    task.id = "t1".into();
    store.append_task("c1", &task).unwrap();
    std::fs::write(tmp.path().join("x.png"), b"not a png").unwrap();
    let e = attach_captures(&store, "c1", "t1", &[tmp.path().join("x.png")]).unwrap_err();
    assert!(e.contains("not a png"), "{e}");
    assert!(store.read_tasks("c1", 10)[0].attachments.is_empty());
}

#[test]
fn the_package_pins_playwright_exactly() {
    let v: Value = serde_json::from_str(&package_json()).unwrap();
    assert_eq!(v["dependencies"]["playwright"], PLAYWRIGHT_VERSION);
    assert_eq!(v["type"], "module");
    assert!(
        PLAYWRIGHT_VERSION
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.'),
        "an exact version, not a range"
    );
}

// `npm ci` refuses a lock that disagrees with package.json, and trusts a lock without hashes: the
// crew server would then fail to install, or install whatever the registry serves. Both ends of that
// are checked here, so moving the pin without regenerating the lock fails in CI, not on a server.
#[test]
fn the_lock_matches_the_package_and_hashes_every_tarball() {
    let pkg: Value = serde_json::from_str(&package_json()).unwrap();
    let lock: Value = serde_json::from_str(PACKAGE_LOCK).unwrap();
    assert_eq!(lock["lockfileVersion"], 3);
    assert_eq!(lock["name"], pkg["name"]);
    let root = &lock["packages"][""];
    assert_eq!(
        root["dependencies"], pkg["dependencies"],
        "root deps = package.json"
    );
    let packages = lock["packages"].as_object().unwrap();
    for name in ["playwright", "playwright-core"] {
        let p = &packages[&format!("node_modules/{name}")];
        assert_eq!(p["version"], PLAYWRIGHT_VERSION, "{name} is at the pin");
        let url = p["resolved"].as_str().unwrap();
        assert!(
            url.starts_with("https://registry.npmjs.org/"),
            "{name} comes from the npm registry: {url}"
        );
        assert!(
            p["integrity"].as_str().unwrap().starts_with("sha512-"),
            "{name} has an integrity hash"
        );
    }
    for (path, p) in packages.iter().filter(|(k, _)| !k.is_empty()) {
        assert!(p["integrity"].is_string(), "{path} has no integrity hash");
        assert!(
            p["resolved"]
                .as_str()
                .is_some_and(|u| u.starts_with("https://")),
            "{path} is not fetched over https"
        );
    }
    assert_eq!(
        packages.len(),
        3,
        "the root and the two playwright packages, nothing else"
    );
}

// ── The real thing: node + Playwright + browsers ────────────────────────────────────────────────

/// Where Playwright's browsers are, if any: `PLAYWRIGHT_BROWSERS_PATH`, else the per-user cache.
fn browsers_cache() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    [
        home.join("Library/Caches/ms-playwright"),
        home.join(".cache/ms-playwright"),
    ]
    .into_iter()
    .find(|p| p.is_dir())
}

fn has_browser(cache: &Path, prefix: &str) -> bool {
    std::fs::read_dir(cache)
        .map(|d| {
            d.flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(prefix))
        })
        .unwrap_or(false)
}

/// Why the real run cannot happen here, or `None` when it can.
fn real_run_blocker() -> Option<String> {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        return Some("node is not installed".into());
    }
    let Some(cache) = browsers_cache() else {
        return Some("no Playwright browsers cache (PLAYWRIGHT_BROWSERS_PATH, ~/Library/Caches/ms-playwright, ~/.cache/ms-playwright)".into());
    };
    if !has_browser(&cache, "chromium") {
        return Some(format!("no chromium in {}", cache.display()));
    }
    None
}

/// Install the pinned package where `runner.mjs` can `require("playwright")`, from the lock like the
/// real install (`npm ci`), without downloading browsers (they come from the cache found above).
/// `Err` with the reason when that cannot be done.
fn install_runner_package(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("package.json"), package_json()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("package-lock.json"), PACKAGE_LOCK).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("runner.mjs"), RUNNER_MJS).map_err(|e| e.to_string())?;
    let out = std::process::Command::new("npm")
        .args(["ci", "--no-audit", "--no-fund", "--silent"])
        .env("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1")
        .current_dir(dir)
        .output()
        .map_err(|e| format!("npm: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "npm ci failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// A page that is different at 390 and 1440 px and in the dark theme, so a screenshot that ignored
/// the matrix would show.
const PAGE: &str = "<!doctype html><meta name=viewport content='width=device-width'>\
<style>body{margin:0;font:20px sans-serif;background:#fff;color:#000}\
@media (prefers-color-scheme:dark){body{background:#000;color:#fff}}\
@media (min-width:800px){body{background:#06c}}</style><h1>capture probe</h1>";

#[tokio::test(flavor = "multi_thread")]
async fn a_real_playwright_run_captures_the_matrix() {
    if let Some(why) = real_run_blocker() {
        eprintln!("SKIP a_real_playwright_run_captures_the_matrix: {why}");
        return;
    }
    let _g = TEST_LOCK.lock().await;
    clear_runner();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let cap = capture_dir(&dir);
    if let Err(why) = install_runner_package(&cap) {
        eprintln!("SKIP a_real_playwright_run_captures_the_matrix: {why}");
        return;
    }
    // The cache holds whatever version the machine has; the pinned one may want a different build.
    // Point the runner at the cache and skip when the pinned version cannot launch.
    std::os::unix::fs::symlink(browsers_cache().unwrap(), cap.join("browsers")).unwrap();

    let page = tmp.path().join("page.html");
    std::fs::write(&page, PAGE).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for s in listener.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
                PAGE.len()
            );
        }
    });

    let engines = if has_browser(&browsers_cache().unwrap(), "webkit") {
        json!(["chromium", "webkit"])
    } else {
        json!(["chromium"])
    };
    let s =
        Spec::from_args(&json!({"url": format!("http://127.0.0.1:{port}/"), "engines": engines}))
            .unwrap();
    let out = tmp.path().join("out");
    match run(&dir, &s, &out).await {
        Ok(got) => {
            assert_eq!(got.files.len(), s.combos().len());
            let mut sizes = std::collections::BTreeSet::new();
            for (c, p) in &got.files {
                let bytes = std::fs::read(p).unwrap();
                assert_eq!(&bytes[..4], b"\x89PNG", "{c} is a PNG");
                sizes.insert(bytes.len());
            }
            assert!(
                sizes.len() > 1,
                "every cell produced the same image: the matrix was ignored"
            );
        }
        // The pinned Playwright wants its own browser build; a cache from another version cannot
        // launch it. That is a machine without the pinned browsers: skip, saying so.
        Err(e) if e.contains("Executable doesn't exist") => {
            eprintln!(
                "SKIP a_real_playwright_run_captures_the_matrix: pinned {PLAYWRIGHT_VERSION} browsers not installed: {e}"
            );
        }
        Err(e) => panic!("real capture failed: {e}"),
    }
}
