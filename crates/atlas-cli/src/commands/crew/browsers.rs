//! `atlas crew browsers install|status <ssh-target>`: the browsers behind a crewmate's `capture` tool.
//!
//! Playwright's own chromium and webkit (not the distro's), pinned to one exact version
//! (`atlas_host::crew::capture::PLAYWRIGHT_VERSION`), live under `<crew home>/.atlas/capture/`
//! next to the runner script hostd runs. Only the apt part of the install needs root
//! (`--with-deps`); every other step runs as the crew user. The scripts are pure functions, pinned
//! by tests; `atlas crew install --browsers` runs the same [`install`].

use anyhow::{Result, bail};
use atlas_host::crew::capture::{PLAYWRIGHT_VERSION, RUNNER_MJS, package_json};
use clap::{Args, Subcommand};

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::Remote;

use super::layout::WHICH_USER;

#[derive(Debug, Args)]
pub struct BrowsersArgs {
    #[command(subcommand)]
    pub command: BrowsersCommands,
}

#[derive(Debug, Subcommand)]
pub enum BrowsersCommands {
    /// Install Playwright's chromium and webkit (pinned) for the `capture` tool; needs sudo once, for the apt deps
    Install(TargetArgs),
    /// Say what the capture tool is missing on a server: node, npm, the runner, the browsers, their libraries
    Status(TargetArgs),
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    /// SSH target for the crew server (e.g. user@hostname or hostname)
    pub ssh_target: String,
}

impl AtlasCommand for BrowsersArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            BrowsersCommands::Install(a) => {
                install(&Remote::new(&a.ssh_target), &a.ssh_target).await
            }
            BrowsersCommands::Status(a) => status(&Remote::new(&a.ssh_target), &a.ssh_target).await,
        }
    }
}

/// Shell prelude of every script: `u` is the crew user, `d` the capture dir (hostd's
/// `<hostd dir>/capture`, the hostd dir being `<crew home>/.atlas`).
fn prelude() -> String {
    format!("u=$({WHICH_USER}); d=/var/lib/$u/.atlas/capture; ")
}

/// Fails with what to do when node, npm or npx is missing: the runner is a Node script and
/// Playwright installs through npm. Checked before anything is written, so the message is
/// "install node", not an npx stack trace.
pub fn need_node_script() -> String {
    "for t in node npm npx; do \
     command -v $t >/dev/null 2>&1 || { echo \"$t is not installed: install Node.js 18 or newer with npm \
     (apt install nodejs npm, or nodesource) and run this again\" >&2; exit 1; }; done"
        .into()
}

/// As the crew user: the capture dir and its `browsers/` exist. Idempotent.
pub fn mkdir_script() -> String {
    format!("{}sudo -u \"$u\" mkdir -p \"$d/browsers\"", prelude())
}

/// As the crew user: write `file` of the capture dir from stdin (the caller pipes the text; a
/// rewrite of the same text is harmless). `file` is one of our two constants, never user input.
pub fn write_file_script(file: &str) -> String {
    format!(
        "{}sudo -u \"$u\" sh -c 'cat > \"$1\"' _ \"$d/{file}\"",
        prelude()
    )
}

/// As the crew user: install the pinned package into the capture dir. No browser download here
/// (the root step does it, with the system libraries). A no-op when the pin is already there.
pub fn npm_install_script() -> String {
    format!(
        "{}cd \"$d\" && sudo -u \"$u\" env PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install --no-audit --no-fund --silent",
        prelude()
    )
}

/// The one root step: Playwright's browsers and the system libraries they need (apt) into the
/// capture dir's `browsers/`, then readable by every user (crewmates may run as their own users).
/// The pinned version is written here and in `package.json` and nowhere else.
pub fn root_deps_script() -> String {
    format!(
        "{}cd \"$d\" && \
         sudo env PLAYWRIGHT_BROWSERS_PATH=\"$d/browsers\" npx -y playwright@{PLAYWRIGHT_VERSION} install --with-deps chromium webkit && \
         sudo chown -R \"$u\": \"$d/browsers\" && sudo chmod -R a+rX \"$d/browsers\"",
        prelude()
    )
}

/// What `status` runs: one line per missing piece on stdout, nothing when all is there; exit 1
/// when anything is. The last check launches each installed engine once, which is what finds a
/// missing system library (the browser is there but cannot start).
pub fn status_script() -> String {
    format!(
        "{prelude}miss=0; m() {{ echo \"$1\"; miss=1; }}; \
         command -v node >/dev/null 2>&1 || m 'node: missing (install Node.js 18 or newer)'; \
         command -v npm >/dev/null 2>&1 || m 'npm: missing (apt install npm)'; \
         [ -f \"$d/runner.mjs\" ] || m 'runner: missing, run atlas crew browsers install'; \
         if [ ! -d \"$d/node_modules/playwright\" ]; then m 'playwright: package missing, run atlas crew browsers install'; \
         else v=$(sed -n 's/.*\"version\": *\"\\([^\"]*\\)\".*/\\1/p' \"$d/node_modules/playwright/package.json\" | head -n1); \
         [ \"$v\" = {pin} ] || m \"playwright: $v installed, atlas wants {pin}: run atlas crew browsers install\"; fi; \
         for e in chromium webkit; do \
           ls \"$d/browsers\" 2>/dev/null | grep -q \"^$e\" || m \"browsers: $e missing, run atlas crew browsers install\"; done; \
         if [ $miss = 0 ]; then \
           for e in chromium webkit; do \
             err=$(cd \"$d\" && sudo -u \"$u\" env PLAYWRIGHT_BROWSERS_PATH=\"$d/browsers\" node -e \
               \"require('playwright').$e.launch().then(b=>b.close()).catch(e=>{{console.error(String(e.message).split('\\n')[0]);process.exit(1)}})\" 2>&1) \
               || m \"deps: $e does not start ($err): run atlas crew browsers install\"; \
           done; \
         fi; \
         exit $miss",
        prelude = prelude(),
        pin = PLAYWRIGHT_VERSION,
    )
}

fn fail(what: &str, target: &str, o: &crate::commands::deploy::remote::Output) -> anyhow::Error {
    let why = if o.stderr.trim().is_empty() {
        o.stdout.trim()
    } else {
        o.stderr.trim()
    };
    anyhow::anyhow!("{what} on {target} failed: {why}")
}

/// Install the capture runtime on the host. Idempotent: run it again to repair or to move to a new
/// pinned version. Shared with `atlas crew install --browsers`.
pub async fn install(r: &Remote, target: &str) -> Result<()> {
    let step = |m: &str| eprintln!("\x1b[36m▸\x1b[0m {m}");

    step("checking node and npm");
    let o = r.exec(&need_node_script()).await?;
    if o.code != 0 {
        bail!("{}", o.stderr.trim());
    }

    step("preparing the capture directory");
    let o = r.exec(&mkdir_script()).await?;
    if o.code != 0 {
        return Err(fail("creating the capture directory", target, &o));
    }
    for (file, text) in [
        ("package.json", package_json()),
        ("runner.mjs", RUNNER_MJS.to_string()),
    ] {
        let o = r
            .exec_stdin(&write_file_script(file), Some(text.as_bytes()))
            .await?;
        if o.code != 0 {
            return Err(fail(&format!("writing {file}"), target, &o));
        }
    }

    step(&format!("installing playwright {PLAYWRIGHT_VERSION}"));
    let o = r.exec(&npm_install_script()).await?;
    if o.code != 0 {
        return Err(fail("npm install", target, &o));
    }

    step("installing chromium and webkit with their system libraries (sudo, a few minutes)");
    let o = r.exec(&root_deps_script()).await?;
    if o.code != 0 {
        return Err(fail("playwright install --with-deps", target, &o));
    }

    println!("\x1b[32m✓\x1b[0m Browsers ready on {target}: crewmates can use the capture tool.");
    Ok(())
}

async fn status(r: &Remote, target: &str) -> Result<()> {
    let o = r.exec(&status_script()).await?;
    if o.code == 255 {
        bail!("cannot reach {target}: {}", o.stderr.trim());
    }
    let lines = o.stdout.trim();
    if lines.is_empty() && o.code == 0 {
        println!(
            "\x1b[32m✓\x1b[0m capture is ready on {target} (playwright {PLAYWRIGHT_VERSION}: chromium, webkit)."
        );
        return Ok(());
    }
    for l in lines.lines() {
        println!("\x1b[31m✗\x1b[0m {l}");
    }
    if lines.is_empty() {
        bail!("checking {target} failed: {}", o.stderr.trim());
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(hay: &str, needle: &str) -> usize {
        hay.matches(needle).count()
    }

    // The pin is one constant: it is in the root install step and in package.json, and no other
    // script spells a version, so moving it is a one-line change that moves everything.
    #[test]
    fn the_pinned_version_is_in_the_root_install_and_package_json_only() {
        assert_eq!(count(&root_deps_script(), PLAYWRIGHT_VERSION), 1);
        assert!(root_deps_script().contains(&format!(
            "npx -y playwright@{PLAYWRIGHT_VERSION} install --with-deps chromium webkit"
        )));
        assert_eq!(count(&package_json(), PLAYWRIGHT_VERSION), 1);
        for s in [
            need_node_script(),
            mkdir_script(),
            write_file_script("runner.mjs"),
            npm_install_script(),
        ] {
            assert_eq!(count(&s, PLAYWRIGHT_VERSION), 0, "{s}");
        }
        assert!(!root_deps_script().contains("playwright@latest"));
    }

    // Root is for apt and nothing else: only the --with-deps step uses plain sudo, every other
    // step that writes in the capture dir runs as the crew user.
    #[test]
    fn only_the_with_deps_step_runs_as_root() {
        let root = root_deps_script();
        assert!(root.contains("sudo env PLAYWRIGHT_BROWSERS_PATH"));
        assert!(root.contains("--with-deps"));
        for s in [
            mkdir_script(),
            write_file_script("package.json"),
            write_file_script("runner.mjs"),
            npm_install_script(),
        ] {
            assert!(s.contains("sudo -u \"$u\""), "runs as the crew user: {s}");
            assert!(!s.contains("--with-deps"), "{s}");
            assert!(!s.contains("sudo env"), "no root env step: {s}");
        }
        // The probe in status also runs as the crew user: it must see what hostd sees.
        assert!(status_script().contains("sudo -u \"$u\" env PLAYWRIGHT_BROWSERS_PATH"));
        assert!(!status_script().contains("--with-deps"));
    }

    // Browsers live inside the capture dir, where hostd looks (`PLAYWRIGHT_BROWSERS_PATH` of the
    // runner is `<capture dir>/browsers`), and are made readable for other users.
    #[test]
    fn browsers_go_to_the_capture_dir_hostd_uses_and_are_world_readable() {
        let root = root_deps_script();
        assert!(root.contains("d=/var/lib/$u/.atlas/capture"));
        assert!(root.contains("PLAYWRIGHT_BROWSERS_PATH=\"$d/browsers\""));
        assert!(root.contains("chmod -R a+rX \"$d/browsers\""));
        assert!(root.contains("chown -R \"$u\": \"$d/browsers\""));
        let hostd = atlas_host::crew::capture::capture_dir(std::path::Path::new(
            "/var/lib/atlas-crew/.atlas",
        ));
        assert_eq!(
            hostd,
            std::path::Path::new("/var/lib/atlas-crew/.atlas/capture")
        );
    }

    // The crew user is asked of the host (a server installed as atlas-bot keeps that user), never
    // assumed.
    #[test]
    fn every_script_resolves_the_crew_user_on_the_host() {
        for s in [
            mkdir_script(),
            write_file_script("runner.mjs"),
            npm_install_script(),
            root_deps_script(),
            status_script(),
        ] {
            assert!(s.starts_with(&format!("u=$({WHICH_USER});")), "{s}");
        }
    }

    // Safe to run twice: directories are `mkdir -p`, files are rewritten by `cat >`, `npm install`
    // is a no-op at the pin, `playwright install` skips what is downloaded. Nothing appends, nothing
    // deletes, nothing fails on "already exists".
    #[test]
    fn install_is_idempotent_nothing_appends_or_deletes() {
        let all = [
            mkdir_script(),
            write_file_script("package.json"),
            write_file_script("runner.mjs"),
            npm_install_script(),
            root_deps_script(),
        ]
        .join("\n");
        assert!(mkdir_script().contains("mkdir -p"));
        assert!(!all.contains(">>"), "appends are not idempotent");
        assert!(!all.contains("rm "), "install never deletes");
        assert!(!all.contains("--force"));
    }

    // Missing node is a sentence that says what to install, before anything is written.
    #[test]
    fn a_host_without_node_is_told_to_install_it() {
        let s = need_node_script();
        for t in ["node", "npm", "npx"] {
            assert!(s.contains(t));
        }
        assert!(s.contains("install Node.js 18 or newer"));
        assert!(s.contains("exit 1"));
    }

    // Status says one line per missing piece (node, npm, runner, package, version, each engine,
    // each engine's libraries) and exits non-zero when any is missing.
    #[test]
    fn status_reports_each_missing_piece_on_its_own_line() {
        let s = status_script();
        for piece in [
            "node: missing",
            "npm: missing",
            "runner: missing",
            "playwright: package missing",
            "browsers: $e missing",
            "deps: $e does not start",
        ] {
            assert!(s.contains(piece), "status never says '{piece}'");
        }
        assert!(
            s.contains(&format!("atlas wants {PLAYWRIGHT_VERSION}")),
            "the version it wants is named"
        );
        assert!(s.ends_with("exit $miss"));
        // The launch probe only runs when everything else is there: it would only repeat the news.
        assert!(s.contains("if [ $miss = 0 ]"));
    }

    // The scripts are real shell: `sh -n` parses them. (The install ones are only parsed; status
    // is also run below against a fake host.)
    #[test]
    fn every_script_parses_as_shell() {
        for (name, s) in [
            ("need_node", need_node_script()),
            ("mkdir", mkdir_script()),
            ("write", write_file_script("runner.mjs")),
            ("npm", npm_install_script()),
            ("root", root_deps_script()),
            ("status", status_script()),
        ] {
            let out = std::process::Command::new("sh")
                .args(["-n", "-c", &s])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    // Status against a host that has nothing: `sudo` and the crew user are stubbed so the real
    // script runs unchanged and must name what is missing, one line each.
    #[cfg(unix)]
    #[test]
    fn status_on_an_empty_host_names_what_is_missing() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        // `sudo -u <user> cmd…` -> `cmd…`; `id <user>` succeeds so WHICH_USER prints atlas-crew.
        let stub = |name: &str, body: &str| {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        stub("sudo", "shift; shift; exec \"$@\"");
        stub("id", "exit 0");
        // The script's `d=/var/lib/$u/...` is absolute: a home that does not exist is an empty host.
        let script = status_script().replace(
            "/var/lib/$u/.atlas/capture",
            &tmp.path().join("none").to_string_lossy(),
        );
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&script)
            // node and npm are hidden: only the stubs dir and the basics are on PATH.
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(text.contains("runner: missing"), "{text}");
        assert!(text.contains("playwright: package missing"), "{text}");
        assert!(text.contains("browsers: chromium missing"), "{text}");
        assert!(text.contains("browsers: webkit missing"), "{text}");
        assert!(
            !text.contains("deps:"),
            "no launch probe while pieces are missing: {text}"
        );
    }

    /// Run the install scripts for real (node, npm, the pinned package from the registry, the
    /// cached browsers linked in for the root step's download) against a temp "server", with `sudo`
    /// and `id` stubbed to run unprivileged, then ask `status`: it must say nothing is missing, and
    /// say what is missing again once a browser is taken away. Skips, printing why, where node,
    /// npm, the network or a Playwright browsers cache is absent.
    #[cfg(unix)]
    #[test]
    fn the_install_scripts_leave_a_host_that_status_calls_ready() {
        use std::os::unix::fs::PermissionsExt;
        let skip = |why: &str| {
            eprintln!("SKIP the_install_scripts_leave_a_host_that_status_calls_ready: {why}")
        };
        let have = |t: &str| {
            std::process::Command::new(t)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        };
        if !have("node") || !have("npm") {
            return skip("node or npm is not installed");
        }
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let Some(cache) = [
            home.join("Library/Caches/ms-playwright"),
            home.join(".cache/ms-playwright"),
        ]
        .into_iter()
        .find(|p| p.is_dir()) else {
            return skip("no Playwright browsers cache to stand in for the root download");
        };
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        let cap = tmp.path().join("cap");
        std::fs::create_dir_all(&bin).unwrap();
        let stub = |name: &str, body: &str| {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        stub("sudo", "[ \"$1\" = -u ] && shift && shift; exec \"$@\"");
        stub("id", "exit 0");
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let sh = |script: &str, stdin: Option<&str>| {
            use std::io::Write;
            let script = script.replace("/var/lib/$u/.atlas/capture", &cap.to_string_lossy());
            let mut c = std::process::Command::new("/bin/sh")
                .args(["-c", &script])
                .env("PATH", &path)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            if let Some(s) = stdin {
                c.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
            }
            c.wait_with_output().unwrap()
        };
        let ok = |o: &std::process::Output, what: &str| {
            assert!(
                o.status.success(),
                "{what}: {}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
        };

        // Twice: the second run must change nothing and still succeed.
        for _ in 0..2 {
            ok(&sh(&mkdir_script(), None), "mkdir");
            ok(
                &sh(&write_file_script("package.json"), Some(&package_json())),
                "package.json",
            );
            ok(
                &sh(&write_file_script("runner.mjs"), Some(RUNNER_MJS)),
                "runner.mjs",
            );
            let npm = sh(&npm_install_script(), None);
            if !npm.status.success() {
                return skip(&format!(
                    "npm install failed (no network?): {}",
                    String::from_utf8_lossy(&npm.stderr).trim()
                ));
            }
        }
        assert_eq!(
            std::fs::read_to_string(cap.join("runner.mjs")).unwrap(),
            RUNNER_MJS
        );

        // The root step's download is replaced by the cache.
        for e in std::fs::read_dir(&cache).unwrap().flatten() {
            let n = e.file_name();
            let n = n.to_string_lossy();
            if n.starts_with("chromium") || n.starts_with("webkit") {
                let _ = std::os::unix::fs::symlink(e.path(), cap.join("browsers").join(&*n));
            }
        }
        let st = sh(&status_script(), None);
        let text = String::from_utf8_lossy(&st.stdout);
        if text.contains("does not start") && text.contains("Executable doesn't exist") {
            return skip(&format!(
                "the cache lacks the browsers of playwright {PLAYWRIGHT_VERSION}: {text}"
            ));
        }
        assert!(
            st.status.success() && text.trim().is_empty(),
            "status on an installed host: {text}"
        );

        // Take webkit away: status says so, in one line.
        for e in std::fs::read_dir(cap.join("browsers")).unwrap().flatten() {
            if e.file_name().to_string_lossy().starts_with("webkit-") {
                std::fs::remove_file(e.path()).unwrap();
            }
        }
        let st = sh(&status_script(), None);
        let text = String::from_utf8_lossy(&st.stdout);
        assert_eq!(st.status.code(), Some(1));
        assert_eq!(text.trim().lines().count(), 1, "{text}");
        assert!(text.contains("browsers: webkit missing"), "{text}");
    }
}
