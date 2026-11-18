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
    report(skip, None)
}

/// As [`full_report`], and also a `<key>.findings.json` with `findings` (JSON text) next to every
/// PNG, listed in the report: the runner when the spec asks for `checks`, which the UI gate does.
pub(crate) fn full_report_with_findings(skip: &[&str], findings: &str) -> String {
    report(skip, Some(findings))
}

fn report(skip: &[&str], findings: Option<&str>) -> String {
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
                let mut extra = String::new();
                if let Some(f) = findings {
                    sh.push_str(&format!(
                        "cat > \"$OUT/{key}.findings.json\" <<'EOF'\n{f}\nEOF\n"
                    ));
                    extra = format!(",\"findings\":\"'\"$OUT\"'/{key}.findings.json\"");
                }
                entries.push(format!(
                    "{{\"engine\":\"{e}\",\"width\":{w},\"theme\":\"{t}\",\"path\":\"'\"$OUT\"'/{key}.png\"{extra}}}"
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

// A runner that hangs is ended at its timeout, with the process killed (kill_on_drop), not left running. The
// runner records its pid and then sleeps for ten minutes, ten times what anything here waits (HANG): only the
// timeout's kill can end it, so no wall-clock budget is needed to tell "stopped" from "waited for".
#[tokio::test]
async fn a_runner_that_hangs_is_stopped_at_the_timeout_and_killed() {
    use atlas_core::scaling::HANG;
    let _g = TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let pidfile = tmp.path().join("runner.pid");
    let body = format!("echo $$ > {}\nexec sleep 600", pidfile.display());
    // The 400 ms timeout can pass before a starved machine has started the script: try again until the runner
    // got far enough to name itself.
    let mut pid = None;
    for _ in 0..20 {
        let _ = std::fs::remove_file(&pidfile);
        use_runner(&fake_runner(tmp.path(), &body), Duration::from_millis(400));
        let got = tokio::time::timeout(HANG, run(tmp.path(), &spec(), &tmp.path().join("out")))
            .await
            .expect("the timeout was not enforced: run waited for the hung runner");
        clear_runner();
        let e = got.unwrap_err();
        assert!(e.contains("was stopped"), "{e}");
        pid = std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        if pid.is_some() {
            break;
        }
    }
    let pid = pid.expect("the runner never got to name itself");
    let t0 = std::time::Instant::now();
    while atlas_core::sys::pid_alive(pid) {
        assert!(t0.elapsed() < HANG, "the runner {pid} outlived its timeout");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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
    std::fs::write(dir.join("detectors.mjs"), DETECTORS_MJS).map_err(|e| e.to_string())?;
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
@media (min-width:800px){body{background:#06c}}</style><h1>capture probe</h1>\
<main><button data-testid=\"probe-button\" data-kind=\"primary\">Probe me</button></main>\
<div style=\"height:2000px\"></div>";

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
    let s = Spec::from_args(
        &json!({"url": format!("http://127.0.0.1:{port}/"), "engines": engines, "dom": true}),
    )
    .unwrap();
    let out = tmp.path().join("out");
    match run(&dir, &s, &out).await {
        Ok(got) => {
            assert_eq!(got.files.len(), s.combos().len());
            // `dom: true`: one element-box report per cell, next to the PNG, that the ranking reads.
            assert_eq!(got.doms.len(), s.combos().len());
            for (c, p) in &got.doms {
                let dom: crate::crew::locate::rank::DomReport =
                    serde_json::from_slice(&std::fs::read(p).unwrap())
                        .unwrap_or_else(|e| panic!("{c}: the DOM report does not parse: {e}"));
                assert_eq!(dom.width, c.width, "{c}");
                assert!(
                    dom.page_h >= 2000.0,
                    "{c}: the page height is the whole page"
                );
                let button = dom
                    .elements
                    .iter()
                    .find(|e| e.testid.as_deref() == Some("probe-button"))
                    .unwrap_or_else(|| panic!("{c}: the testid'd button is in the report"));
                assert_eq!(button.tag, "button");
                assert_eq!(button.role.as_deref(), Some("button"));
                assert_eq!(button.name.as_deref(), Some("Probe me"));
                assert_eq!(button.text.as_deref(), Some("Probe me"));
                assert_eq!(button.data.get("kind").map(String::as_str), Some("primary"));
                assert_eq!(button.selector, "[data-testid=\"probe-button\"]");
                assert!(
                    button.w > 0.0 && button.h > 0.0 && button.depth >= 2,
                    "{c}: {button:?}"
                );
                assert!(
                    dom.elements.iter().any(|e| e.main && e.tag == "main"),
                    "{c}: the main landmark is flagged"
                );
                assert!(
                    dom.elements
                        .iter()
                        .all(|e| e.tag != "script" && e.tag != "style"),
                    "{c}: no script or style"
                );
                // The ranking finds the button from a rect drawn around it.
                let r = crate::crew::locate::rank::Rect::from_array([
                    button.x - 4.0,
                    button.y - 4.0,
                    button.w + 8.0,
                    button.h + 8.0,
                ])
                .unwrap();
                let top = crate::crew::locate::rank::rank(&dom, Some(r), 3);
                assert_eq!(
                    top[0].element.testid.as_deref(),
                    Some("probe-button"),
                    "{c}: {top:?}"
                );
            }
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

// ── The UI detectors (TEMPORARY, #215): every fixture, failing and fixed, in both engines ───────

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/crew/capture/fixtures");

/// `(fixture name, detector it must trip)`: each has `<name>.fail.html` and `<name>.fixed.html`.
const DETECTOR_FIXTURES: [(&str, &str); 7] = [
    ("empty-band", "empty-band"),
    ("gutter", "gutter"),
    ("sibling-overlap", "sibling-overlap"),
    ("raw-text", "raw-text"),
    ("action-hierarchy", "action-hierarchy"),
    ("cls", "cls"),
    ("focus-after-tap", "focus-after-tap"),
];

/// How long the server holds a `.svg` back: the image that arrives after the page laid out (the
/// classic layout-shift cause) in `cls.*.html`.
const SLOW_IMAGE: Duration = Duration::from_millis(300);

/// Serve `FIXTURES` on a free port: `GET /<file>` is `<FIXTURES>/<file>` (a real navigation, so
/// the layout-shift observer registered before it sees the load). `.svg` files are slow.
fn serve_fixtures() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            // One thread per connection: a held-back image must not hold the page back too.
            std::thread::spawn(move || serve_one(s));
        }
    });
    port
}

fn serve_one(mut s: std::net::TcpStream) {
    use std::io::{Read, Write};
    let mut buf = [0u8; 2048];
    let n = s.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]);
    let name = req
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_string();
    // Fixtures are flat files: nothing with a path in it is served.
    let body = if name.contains('/') || name.contains("..") {
        None
    } else {
        std::fs::read_to_string(Path::new(FIXTURES).join(&name)).ok()
    };
    let _ = match body {
        Some(b) => {
            let svg = name.ends_with(".svg");
            if svg {
                std::thread::sleep(SLOW_IMAGE);
            }
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: {}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                if svg { "image/svg+xml" } else { "text/html" },
                b.len()
            )
        }
        None => write!(
            s,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    };
}

/// A runner installed under a temp dir with the browsers cache linked in, or why not.
pub(crate) fn real_runner(tmp: &Path, who: &str) -> Result<PathBuf, String> {
    if let Some(why) = real_run_blocker() {
        return Err(why);
    }
    let dir = tmp.join("hostd");
    install_runner_package(&capture_dir(&dir))?;
    std::os::unix::fs::symlink(
        browsers_cache().unwrap(),
        capture_dir(&dir).join("browsers"),
    )
    .map_err(|e| format!("{who}: {e}"))?;
    Ok(dir)
}

/// Capture `file` with the detectors in `engines` at `width` and return each engine's findings.
async fn findings_of(
    dir: &Path,
    out: &Path,
    port: u16,
    file: &str,
    width: u32,
    engines: &Value,
) -> Result<Vec<(String, Vec<detectors::Finding>)>, String> {
    let s = Spec::from_args(&json!({
        "url": format!("http://127.0.0.1:{port}/{file}"), "engines": engines,
        "widths": [width], "themes": ["light"], "checks": true,
    }))?;
    let got = run(dir, &s, &out.join(file)).await?;
    assert_eq!(
        got.findings.len(),
        got.files.len(),
        "{file}: one findings file per shot"
    );
    got.findings
        .iter()
        .map(|(c, p)| Ok((c.to_string(), detectors::read(p)?)))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_detector_fixture_fails_and_its_fixed_twin_passes_in_chromium_and_webkit() {
    const T: &str = "every_detector_fixture_fails_and_its_fixed_twin_passes_in_chromium_and_webkit";
    let _g = TEST_LOCK.lock().await;
    clear_runner();
    let tmp = tempfile::tempdir().unwrap();
    let dir = match real_runner(tmp.path(), T) {
        Ok(d) => d,
        Err(why) => return eprintln!("SKIP {T}: {why}"),
    };
    let cache = browsers_cache().unwrap();
    if !has_browser(&cache, "webkit") {
        // The acceptance is both engines: half of it is not a pass.
        return eprintln!("SKIP {T}: no webkit in {}", cache.display());
    }
    let engines = json!(["chromium", "webkit"]);
    let port = serve_fixtures();
    let out = tmp.path().join("out");

    for (name, detector) in DETECTOR_FIXTURES {
        for (file, want) in [
            (format!("{name}.fail.html"), true),
            (format!("{name}.fixed.html"), false),
        ] {
            let per_engine = match findings_of(&dir, &out, port, &file, 390, &engines).await {
                Ok(f) => f,
                Err(e) if e.contains("Executable doesn't exist") => {
                    return eprintln!(
                        "SKIP {T}: pinned {PLAYWRIGHT_VERSION} browsers not installed: {e}"
                    );
                }
                Err(e) => panic!("{file}: {e}"),
            };
            assert_eq!(per_engine.len(), 2, "{file}: both engines ran");
            for (cell, found) in per_engine {
                let errors: Vec<_> = found
                    .iter()
                    .filter(|f| f.severity == detectors::Severity::Error)
                    .collect();
                if want {
                    assert!(
                        errors.iter().any(|f| f.detector == detector),
                        "{file} on {cell}: expected an error from `{detector}`, got {found:?}"
                    );
                    assert!(
                        errors.iter().all(|f| f.detector == detector),
                        "{file} on {cell}: a fixture trips only its own detector, got {found:?}"
                    );
                } else {
                    assert!(
                        found.is_empty(),
                        "{file} on {cell}: the fixed page is clean, got {found:?}"
                    );
                }
            }
        }
    }

    // The 490 px band is #215's acceptance: it reports the band's size and where it is.
    let found = findings_of(&dir, &out, port, "empty-band.fail.html", 390, &engines)
        .await
        .unwrap();
    for (cell, f) in found {
        let band = f.iter().find(|f| f.detector == "empty-band").unwrap();
        assert!(band.rect.unwrap().height >= 490.0, "{cell}: {band:?}");
    }
    // The CLS finding names the element that moved, not just a number.
    let found = findings_of(&dir, &out, port, "cls.fail.html", 390, &engines)
        .await
        .unwrap();
    for (cell, f) in found {
        let cls = f.iter().find(|f| f.detector == "cls").unwrap();
        assert_eq!(
            cls.selector.as_deref(),
            Some("[data-testid=\"article\"]"),
            "{cell}: {cls:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clean_page_has_no_finding_at_phone_and_desktop_widths_in_both_engines() {
    const T: &str = "a_clean_page_has_no_finding_at_phone_and_desktop_widths_in_both_engines";
    let _g = TEST_LOCK.lock().await;
    clear_runner();
    let tmp = tempfile::tempdir().unwrap();
    let dir = match real_runner(tmp.path(), T) {
        Ok(d) => d,
        Err(why) => return eprintln!("SKIP {T}: {why}"),
    };
    if !has_browser(&browsers_cache().unwrap(), "webkit") {
        return eprintln!("SKIP {T}: no webkit");
    }
    let engines = json!(["chromium", "webkit"]);
    let port = serve_fixtures();
    let out = tmp.path().join("out");
    // A detector that cries wolf on an ordinary page (nav, hero with one primary, cards, a form with
    // its own primary, footer) gets switched off: this is the guard against that.
    for width in [390, 1440] {
        match findings_of(&dir, &out, port, "clean.fixed.html", width, &engines).await {
            Ok(per_engine) => {
                for (cell, found) in per_engine {
                    assert!(
                        found.is_empty(),
                        "clean page at {width} on {cell}: {found:?}"
                    );
                }
            }
            Err(e) if e.contains("Executable doesn't exist") => {
                return eprintln!(
                    "SKIP {T}: pinned {PLAYWRIGHT_VERSION} browsers not installed: {e}"
                );
            }
            Err(e) => panic!("clean page: {e}"),
        }
    }
}

#[test]
fn every_detector_has_a_failing_and_a_fixed_fixture_and_the_names_match_the_package() {
    // The names are ui#1290's: the swap to the package must not rename a finding.
    let in_js = std::fs::read_to_string(Path::new(FIXTURES).join("../detectors.mjs")).unwrap();
    let listed: Vec<String> = in_js
        .split("export const DETECTORS = [")
        .nth(1)
        .and_then(|r| r.split(']').next())
        .unwrap()
        .split(',')
        .map(|n| n.trim().trim_matches('"').to_string())
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(
        listed,
        detectors::NAMES,
        "detectors.mjs and detectors.rs list the same detectors"
    );
    for (name, _) in DETECTOR_FIXTURES {
        for kind in ["fail", "fixed"] {
            assert!(
                Path::new(FIXTURES)
                    .join(format!("{name}.{kind}.html"))
                    .is_file(),
                "{name}.{kind}.html is missing"
            );
        }
    }
    assert_eq!(DETECTOR_FIXTURES.len(), detectors::NAMES.len());
}

// ── An isolated crewmate's capture behind the guard, in a real browser ───────────────────────────────────

/// A server on 127.0.0.1 answering `GET <path>` from `routes` (`(path, status line, extra header, body)`) and counting
/// every request it gets.
fn route_server(
    routes: Vec<(&'static str, &'static str, String, String)>,
) -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = std::sync::Arc::new(AtomicUsize::new(0));
    let n = hits.clone();
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let (routes, n) = (routes.clone(), n.clone());
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut s = s;
                let mut buf = [0u8; 4096];
                let k = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..k]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                n.fetch_add(1, Ordering::SeqCst);
                let (status, header, body) = routes
                    .iter()
                    .find(|r| r.0 == path)
                    .map(|r| (r.1, r.2.clone(), r.3.clone()))
                    .unwrap_or(("404 Not Found", String::new(), String::new()));
                let _ = write!(
                    s,
                    "HTTP/1.1 {status}\r\n{header}Content-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    (port, hits)
}

// Incident (security review 4, C1): A's capture reached B's loopback port, directly, through a redirect of A's own
// server, and from A's own page (an `<img>` and a `fetch`). The real runner is started behind the guard with a
// listener table that makes one server A's and the other B's; B must see no request at all, and A's page must load.
// The browser engine also has to prove it sends loopback through the proxy (`proxyIsUsed`), or no cell would load.
#[tokio::test(flavor = "multi_thread")]
async fn a_real_browser_behind_the_guard_loads_the_askers_page_and_never_reaches_another_crewmates_server()
 {
    use std::sync::atomic::Ordering;
    const T: &str = "a_real_browser_behind_the_guard_loads_the_askers_page_and_never_reaches_another_crewmates_server";
    let _g = TEST_LOCK.lock().await;
    clear_runner();
    let tmp = tempfile::tempdir().unwrap();
    let dir = match real_runner(tmp.path(), T) {
        Ok(d) => d,
        Err(why) => return eprintln!("SKIP {T}: {why}"),
    };
    let (b_port, b_hits) = route_server(vec![(
        "/secret",
        "200 OK",
        String::new(),
        "<h1>B's secret</h1>".into(),
    )]);
    let a_page = format!(
        "<h1>A's own page</h1><img src=\"http://127.0.0.1:{b_port}/secret\">\
         <script>fetch('http://127.0.0.1:{b_port}/secret').catch(() => {{}})</script>"
    );
    let (a_port, a_hits) = route_server(vec![
        ("/", "200 OK", String::new(), a_page),
        (
            "/redir",
            "302 Found",
            format!("Location: http://127.0.0.1:{b_port}/secret\r\n"),
            String::new(),
        ),
    ]);
    const A: u32 = 4242;
    *guard::TEST_TABLES.lock() = Some(guard::tests::tcp(&[
        ("127.0.0.1", a_port, A),
        ("127.0.0.1", b_port, 4243),
    ]));
    let capture = |path: String| {
        let mut s = Spec::from_args(&json!({
            "url": format!("http://127.0.0.1:{a_port}{path}"), "engines": ["chromium"],
            "widths": [390], "themes": ["light"], "full_page": false,
        }))
        .unwrap();
        s.reach = Some(Reach { uid: A });
        s
    };
    let out = tmp.path().join("out");
    let own = run(&dir, &capture("/".into()), &out.join("own")).await;
    let redirected = run(&dir, &capture("/redir".into()), &out.join("redir")).await;
    let direct = run(
        &dir,
        &{
            let mut s = capture("/".into());
            s.url = format!("http://127.0.0.1:{b_port}/secret");
            s
        },
        &out.join("direct"),
    )
    .await;
    let metadata = run(
        &dir,
        &{
            let mut s = capture("/".into());
            s.url = "http://169.254.169.254/latest/meta-data/".into();
            s
        },
        &out.join("meta"),
    )
    .await;
    *guard::TEST_TABLES.lock() = None;
    let b = b_hits.load(Ordering::SeqCst);
    assert_eq!(
        b, 0,
        "B's server was reached {b} time(s): own={own:?} redirected={redirected:?}"
    );
    own.unwrap_or_else(|e| panic!("A's own page did not load behind the guard: {e}"));
    assert!(
        a_hits.load(Ordering::SeqCst) >= 2,
        "A's own server was not asked"
    );
    redirected
        .unwrap_or_else(|e| panic!("the redirect cell failed instead of showing the refusal: {e}"));
    assert!(direct.unwrap_err().contains("no server of yours"));
    assert!(metadata.unwrap_err().contains("metadata"));
}
