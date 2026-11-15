//! `atlas crew browsers install|status <ssh-target>`: the browsers behind a crewmate's `capture` tool.
//!
//! Playwright's own chromium and webkit (not the distro's), pinned to one exact version
//! (`atlas_host::crew::capture::PLAYWRIGHT_VERSION`), live under `<crew home>/.atlas/capture/` next
//! to the runner script hostd runs.
//!
//! **No npm code runs as root.** Everything that touches Playwright runs as the crew user: `npm ci`
//! from an embedded lockfile (npm verifies every tarball against its integrity hash), the browser
//! download, and `playwright install-deps --dry-run`, which only reports which system libraries are
//! missing. Atlas reads that report ([`super::apt`]) and the single command that runs as root is an
//! `apt-get install` that atlas builds itself from the validated package names. The scripts are pure
//! functions, pinned by tests; `atlas crew install --browsers` runs the same [`install`].

use anyhow::{Result, bail};
use atlas_host::crew::capture::{
    DETECTORS_MJS, PACKAGE_LOCK, PLAYWRIGHT_VERSION, RUNNER_MJS, package_json,
};
use clap::{Args, Subcommand};

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::{Output, Remote};

use super::apt::{self, Deps};
use super::layout::WHICH_USER;
use super::privileged::Priv;

/// The oldest Node the pinned Playwright runs on (its `engines`: `>=20`). Ubuntu 24.04's
/// `apt install nodejs` is 18 and is refused by Playwright itself.
pub const MIN_NODE_MAJOR: u32 = 20;

#[derive(Debug, Args)]
pub struct BrowsersArgs {
    #[command(subcommand)]
    pub command: BrowsersCommands,
}

#[derive(Debug, Subcommand)]
pub enum BrowsersCommands {
    /// Install Playwright's chromium and webkit (pinned) for the `capture` tool; sudo only for `apt-get install` of the libraries
    Install(TargetArgs),
    /// Say what the capture tool is missing on a server: node (>= 20), npm, the runner, the browsers, their libraries
    Status(TargetArgs),
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    /// SSH target for the crew server (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// SSH port (default 22). Used when the server listens on a non-standard port.
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    /// Read the sudo password from stdin instead of requiring passwordless sudo.
    /// Used by Atlas Desktop; the password is NEVER placed on argv.
    #[arg(long, hide = true)]
    pub sudo_password_stdin: bool,
}

impl TargetArgs {
    fn remote(&self) -> Remote {
        Remote::crew(&self.ssh_target, self.port)
    }

    /// `sudo -u <crew user>` runs in `install` and `status`: make sudo work before either, with the
    /// password on every call when Atlas Desktop sent one.
    async fn privileged<'a>(&self, r: &'a Remote, verb: &str) -> Result<Priv<'a>> {
        Priv::prepare(
            r,
            &self.ssh_target,
            self.sudo_password_stdin,
            &format!("atlas crew browsers {verb} {}", self.ssh_target),
        )
        .await
    }
}

impl AtlasCommand for BrowsersArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            BrowsersCommands::Install(a) => {
                let r = a.remote();
                let p = a.privileged(&r, "install").await?;
                install(&p, &a.ssh_target).await
            }
            BrowsersCommands::Status(a) => {
                let r = a.remote();
                let p = a.privileged(&r, "status").await?;
                status(&p, &a.ssh_target).await
            }
        }
    }
}

/// Shell prelude of every script: `u` is the crew user, `d` the capture dir (hostd's
/// `<hostd dir>/capture`, the hostd dir being `<crew home>/.atlas`), `as` runs a command as the
/// crew user with the browsers in the capture dir (the same environment hostd gives the runner).
fn prelude() -> String {
    format!(
        "u=$({WHICH_USER}); d=/var/lib/$u/.atlas/capture; \
         as() {{ sudo -u \"$u\" env PLAYWRIGHT_BROWSERS_PATH=\"$d/browsers\" \"$@\"; }}; "
    )
}

/// Prints `node: Node X found, need >= 20` (or `node: missing`, `npm: missing`) for each problem
/// with the toolchain and exits 1 when there is one; prints nothing and exits 0 otherwise. The
/// runner is a Node script and Playwright installs through npm, so this is checked before anything
/// is written, and the message says what to install, not an npx stack trace.
pub fn node_check_script() -> String {
    format!(
        "bad=0; \
         if command -v node >/dev/null 2>&1; then \
           v=$(node --version 2>/dev/null); maj=${{v#v}}; maj=${{maj%%.*}}; \
           [ \"$maj\" -ge 0 ] 2>/dev/null || maj=0; \
           if [ \"$maj\" -lt {MIN_NODE_MAJOR} ]; then echo \"node: Node ${{v#v}} found, need >= {MIN_NODE_MAJOR} (install a newer Node.js, for example from nodesource)\"; bad=1; fi; \
         else echo 'node: missing (install Node.js >= {MIN_NODE_MAJOR}, for example from nodesource)'; bad=1; fi; \
         command -v npm >/dev/null 2>&1 || {{ echo 'npm: missing (it ships with Node.js from nodesource; or apt install npm)'; bad=1; }}; \
         exit $bad"
    )
}

/// As the crew user: the capture dir and its `browsers/` exist. Idempotent.
pub fn mkdir_script() -> String {
    format!("{}as mkdir -p \"$d/browsers\"", prelude())
}

/// As the crew user: write `file` of the capture dir from stdin (the caller pipes the text; a
/// rewrite of the same text is harmless). `file` is one of our four constants, never user input.
pub fn write_file_script(file: &str) -> String {
    format!("{}as sh -c 'cat > \"$1\"' _ \"$d/{file}\"", prelude())
}

/// Every file of the capture dir that atlas writes, as `(name, text)`: what `install` puts there.
pub fn capture_files() -> [(&'static str, String); 4] {
    [
        ("package.json", package_json()),
        ("package-lock.json", PACKAGE_LOCK.to_string()),
        ("runner.mjs", RUNNER_MJS.to_string()),
        ("detectors.mjs", DETECTORS_MJS.to_string()),
    ]
}

/// The files `atlas crew update` rewrites on a server that already has the capture dir: the runner
/// and what it imports. Not the package or its lock: they only move together with `npm ci` (which
/// `install` runs; `status` says when the pinned Playwright differs), and the runner and the
/// detectors need no npm. Taken from [`capture_files`], so the two never disagree about the text.
pub fn refresh_files() -> Vec<(&'static str, String)> {
    capture_files()
        .into_iter()
        .filter(|(n, _)| matches!(*n, "runner.mjs" | "detectors.mjs"))
        .collect()
}

/// As the crew user: exit 0 when the capture dir has a runner (browsers were installed here), 1
/// otherwise. `update` refreshes the runner only where there is one.
pub fn capture_present_script() -> String {
    format!("{}as test -f \"$d/runner.mjs\"", prelude())
}

/// Bring the runner of a server that has the capture dir up to this atlas: its `runner.mjs` and
/// `detectors.mjs`, written as the crew user with the script `install` uses. No root, no npm, no
/// browser download. A server without the capture dir is left alone. `true` when it wrote them.
pub async fn refresh(r: &Priv<'_>, target: &str) -> Result<bool> {
    let o = r.exec(&capture_present_script()).await?;
    if o.code == 255 {
        bail!("cannot reach {target}: {}", o.stderr.trim());
    }
    if o.code != 0 {
        return Ok(false);
    }
    for (file, text) in refresh_files() {
        let o = r
            .exec_stdin(&write_file_script(file), Some(text.as_bytes()))
            .await?;
        if o.code != 0 {
            return Err(fail(&format!("writing {file}"), target, &o));
        }
    }
    Ok(true)
}

/// As the crew user: install exactly what `package-lock.json` says. `npm ci`, never `npm install`:
/// it refuses a lock that disagrees with `package.json`, deletes `node_modules` first, and checks
/// every tarball against the integrity hash in the lock. No browser download here.
pub fn npm_ci_script() -> String {
    format!(
        "{}cd \"$d\" && as env PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci --no-audit --no-fund --silent",
        prelude()
    )
}

/// As the crew user: download chromium and webkit into the capture dir's `browsers/` with the
/// locally installed (locked) Playwright, `--no-install` so npx can never fetch another one.
pub fn browsers_download_script() -> String {
    format!(
        "{}cd \"$d\" && as npx --no-install playwright install chromium webkit && chmod -R a+rX \"$d/browsers\"",
        prelude()
    )
}

/// As the crew user: ask the locked Playwright which system libraries are missing. It only reads
/// (`apt-get install -s` underneath) and prints a list, whose exit code is 1 when something is
/// missing, so the script always exits 0 and atlas reads the text ([`apt::parse_dry_run`]).
pub fn deps_dry_run_script() -> String {
    format!(
        "{}cd \"$d\" && as npx --no-install playwright install-deps --dry-run chromium webkit; exit 0",
        prelude()
    )
}

/// What `status` runs: one line per missing piece on stdout, nothing when all is there; exit 1
/// when anything is. The last check launches each installed engine once, which is what finds a
/// missing system library (the browser is there but cannot start).
pub fn status_script() -> String {
    format!(
        "{prelude}miss=0; m() {{ echo \"$1\"; miss=1; }}; \
         nc=$({node_check}); [ -z \"$nc\" ] || {{ echo \"$nc\"; miss=1; }}; \
         [ -f \"$d/runner.mjs\" ] || m 'runner: missing, run atlas crew browsers install'; \
         [ -f \"$d/detectors.mjs\" ] || m 'detectors: missing (the runner imports it), run atlas crew browsers install'; \
         if [ ! -d \"$d/node_modules/playwright\" ]; then m 'playwright: package missing, run atlas crew browsers install'; \
         else v=$(sed -n 's/.*\"version\": *\"\\([^\"]*\\)\".*/\\1/p' \"$d/node_modules/playwright/package.json\" | head -n1); \
         [ \"$v\" = {pin} ] || m \"playwright: $v installed, atlas wants {pin}: run atlas crew browsers install\"; fi; \
         for e in chromium webkit; do \
           ls \"$d/browsers\" 2>/dev/null | grep -q \"^$e\" || m \"browsers: $e missing, run atlas crew browsers install\"; done; \
         if [ $miss = 0 ]; then \
           for e in chromium webkit; do \
             err=$(cd \"$d\" && as node -e \
               \"require('playwright').$e.launch().then(b=>b.close()).catch(e=>{{console.error(String(e.message).split('\\n')[0]);process.exit(1)}})\" 2>&1) \
               || m \"deps: $e does not start ($err): run atlas crew browsers install\"; \
           done; \
         fi; \
         exit $miss",
        prelude = prelude(),
        node_check = node_check_script(),
        pin = PLAYWRIGHT_VERSION,
    )
}

fn fail(what: &str, target: &str, o: &Output) -> anyhow::Error {
    let why = if o.stderr.trim().is_empty() {
        o.stdout.trim()
    } else {
        o.stderr.trim()
    };
    anyhow::anyhow!("{what} on {target} failed: {why}")
}

/// Install the capture runtime on the host. Idempotent: run it again to repair or to move to a new
/// pinned version. Shared with `atlas crew install --browsers`.
pub async fn install(r: &Priv<'_>, target: &str) -> Result<()> {
    let step = |m: &str| eprintln!("\x1b[36m▸\x1b[0m {m}");

    step("checking node and npm");
    let o = r.exec(&node_check_script()).await?;
    if o.code != 0 {
        bail!("{}", o.stdout.trim());
    }

    step("preparing the capture directory (as the crew user)");
    let o = r.exec(&mkdir_script()).await?;
    if o.code != 0 {
        return Err(fail("creating the capture directory", target, &o));
    }
    for (file, text) in capture_files() {
        let o = r
            .exec_stdin(&write_file_script(file), Some(text.as_bytes()))
            .await?;
        if o.code != 0 {
            return Err(fail(&format!("writing {file}"), target, &o));
        }
    }

    step(&format!(
        "installing playwright {PLAYWRIGHT_VERSION} from the lockfile (npm ci, as the crew user)"
    ));
    let o = r.exec(&npm_ci_script()).await?;
    if o.code != 0 {
        return Err(fail("npm ci", target, &o));
    }

    step("downloading chromium and webkit (as the crew user, a few minutes)");
    let o = r.exec(&browsers_download_script()).await?;
    if o.code != 0 {
        return Err(fail("playwright install", target, &o));
    }

    step(
        "checking the system libraries the browsers need (as the crew user, nothing is installed)",
    );
    let o = r.exec(&deps_dry_run_script()).await?;
    if o.code != 0 {
        return Err(fail("playwright install-deps --dry-run", target, &o));
    }
    match apt::parse_dry_run(&o.stdout)? {
        Deps::Ready => step("system libraries already installed"),
        Deps::Missing(names) => {
            step(&format!(
                "installing {} system libraries with apt-get (sudo; the only step that runs as root)",
                names.len()
            ));
            let o = r.exec(&apt::root_script(&names)?).await?;
            if o.code != 0 {
                return Err(fail("apt-get install", target, &o));
            }
        }
    }

    println!("\x1b[32m✓\x1b[0m Browsers ready on {target}: crewmates can use the capture tool.");
    Ok(())
}

/// What `status` prints on stdout for the host's answer (`code`, `stdout` of the status script): the ready line, or
/// one red `✗ ` line per missing piece. Not a success when the script said nothing and failed, or ssh failed (255).
/// Atlas Desktop reads exactly this text (`apps/desktop/src/lib/crew/browsers.ts`).
fn status_report(target: &str, code: Option<i32>, stdout: &str) -> Result<String> {
    let lines = stdout.trim();
    if code == Some(255) {
        bail!("cannot reach {target}");
    }
    if lines.is_empty() {
        if code == Some(0) {
            return Ok(format!(
                "\x1b[32m✓\x1b[0m capture is ready on {target} (playwright {PLAYWRIGHT_VERSION}: chromium, webkit).\n"
            ));
        }
        bail!("checking {target} failed");
    }
    Ok(lines
        .lines()
        .map(|l| format!("\x1b[31m✗\x1b[0m {l}\n"))
        .collect())
}

async fn status(r: &Priv<'_>, target: &str) -> Result<()> {
    let o = r.exec(&status_script()).await?;
    if o.code == 255 {
        bail!("cannot reach {target}: {}", o.stderr.trim());
    }
    if o.stdout.trim().is_empty() && o.code != 0 {
        bail!("checking {target} failed: {}", o.stderr.trim());
    }
    print!("{}", status_report(target, Some(o.code), &o.stdout)?);
    if o.code != 0 {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(hay: &str, needle: &str) -> usize {
        hay.matches(needle).count()
    }

    /// Every script `install` runs, in order, with the root one built from the real dry-run fixture.
    fn all_scripts() -> Vec<(&'static str, String)> {
        let missing = include_str!("fixtures/playwright-1.65.1-install-deps-missing.txt");
        let Deps::Missing(names) = apt::parse_dry_run(missing).unwrap() else {
            panic!()
        };
        vec![
            ("node_check", node_check_script()),
            ("mkdir", mkdir_script()),
            ("write", write_file_script("runner.mjs")),
            ("npm_ci", npm_ci_script()),
            ("download", browsers_download_script()),
            ("dry_run", deps_dry_run_script()),
            ("root", apt::root_script(&names).unwrap()),
            ("status", status_script()),
        ]
    }

    // Playwright is npm code: nothing it runs may be root. The only script that uses sudo for
    // anything but "become the crew user" is the apt-get one atlas builds itself; every other
    // `sudo` is `sudo -u "$u"`, and no `npm`, `npx`, `node` or `playwright` is ever the program root runs.
    #[test]
    fn root_runs_nothing_but_apt_get() {
        for (name, s) in all_scripts() {
            if name == "root" {
                assert!(
                    s.starts_with(
                        "sudo apt-get update && sudo apt-get install -y --no-install-recommends "
                    ),
                    "{s}"
                );
                for prog in [
                    "npm",
                    "npx",
                    "node",
                    "playwright",
                    "sh ",
                    "bash",
                    "env",
                    "curl",
                    "tee",
                ] {
                    assert!(
                        !s.contains(prog),
                        "root script mentions {prog}: {}",
                        &s[..200]
                    );
                }
                continue;
            }
            // Strip the helper that is `sudo -u "$u" env … "$@"`, then no bare sudo may be left.
            assert_eq!(
                count(&s, "sudo "),
                count(&s, "sudo -u \"$u\" "),
                "{name} has a sudo that is not 'sudo -u \"$u\"': {s}"
            );
            assert!(
                !s.contains("--with-deps"),
                "{name}: --with-deps runs apt as root through npm"
            );
            assert!(
                !s.contains("install-deps ") || s.contains("--dry-run"),
                "{name}: install-deps without --dry-run"
            );
        }
    }

    // The package comes from the lock: `npm ci`, never `npm install` (which would resolve afresh and
    // rewrite the lock), and `npx --no-install` so a missing package is an error, not a download of
    // whatever the registry has under that name.
    #[test]
    fn the_lock_is_used_npm_ci_never_install() {
        assert!(npm_ci_script().contains("npm ci "));
        for (name, s) in all_scripts() {
            assert!(!s.contains("npm install"), "{name}: {s}");
            assert!(
                !s.contains("npx -y"),
                "{name}: npx -y fetches whatever it is told"
            );
            assert!(
                !s.contains("playwright@"),
                "{name}: no version spelled in a script"
            );
            if s.contains("npx ") {
                assert!(s.contains("npx --no-install"), "{name}");
            }
        }
        // And the lock is written before it is used.
        let install = include_str!("browsers.rs");
        let (lock, ci) = (
            install.find("(\"package-lock.json\"").unwrap(),
            install.find("npm_ci_script()).await").unwrap(),
        );
        assert!(lock < ci, "package-lock.json is written before npm ci");
    }

    // The pin is one constant in Rust: package.json and the status check, no install script spells it.
    #[test]
    fn the_pinned_version_is_in_package_json_and_status_only() {
        assert_eq!(count(&package_json(), PLAYWRIGHT_VERSION), 1);
        for (name, s) in all_scripts() {
            let want = usize::from(name == "status") * 2;
            assert_eq!(count(&s, PLAYWRIGHT_VERSION), want, "{name}");
        }
    }

    // Browsers live inside the capture dir, where hostd looks (`PLAYWRIGHT_BROWSERS_PATH` of the
    // runner is `<capture dir>/browsers`), readable by other users.
    #[test]
    fn browsers_go_to_the_capture_dir_hostd_uses_and_are_world_readable() {
        let dl = browsers_download_script();
        assert!(dl.contains("d=/var/lib/$u/.atlas/capture"));
        assert!(dl.contains("PLAYWRIGHT_BROWSERS_PATH=\"$d/browsers\""));
        assert!(dl.contains("chmod -R a+rX \"$d/browsers\""));
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
        for (name, s) in all_scripts() {
            if matches!(name, "node_check" | "root") {
                continue;
            }
            assert!(s.starts_with(&format!("u=$({WHICH_USER});")), "{name}: {s}");
        }
    }

    // Safe to run twice: directories are `mkdir -p`, files are rewritten by `cat >`, `npm ci` starts
    // from a clean node_modules, `playwright install` skips what is downloaded, `apt-get install` of
    // installed packages is a no-op. Nothing appends, nothing deletes.
    #[test]
    fn install_is_idempotent_nothing_appends_or_deletes() {
        let all: String = all_scripts()
            .into_iter()
            .map(|(_, s)| s)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(mkdir_script().contains("mkdir -p"));
        assert!(!all.contains(">>"), "appends are not idempotent");
        assert!(!all.contains("rm "), "install never deletes");
        assert!(!all.contains("--force"));
    }

    // The toolchain check says plainly what is wrong, with the version it found.
    #[test]
    fn the_node_check_names_the_version_it_found_and_the_one_it_needs() {
        let s = node_check_script();
        assert!(s.contains("Node ${v#v} found, need >= 20"));
        assert!(s.contains("node: missing"));
        assert!(s.contains("npm: missing"));
        assert!(s.ends_with("exit $bad"));
    }

    // Status says one line per missing piece (node, npm, runner, package, version, each engine,
    // each engine's libraries) and exits non-zero when any is missing.
    #[test]
    fn status_reports_each_missing_piece_on_its_own_line() {
        let s = status_script();
        for piece in [
            "runner: missing",
            "detectors: missing",
            "playwright: package missing",
            "browsers: $e missing",
            "deps: $e does not start",
        ] {
            assert!(s.contains(piece), "status never says '{piece}'");
        }
        assert!(s.contains(&format!("atlas wants {PLAYWRIGHT_VERSION}")));
        assert!(s.contains("need >= 20"), "the node check is part of status");
        assert!(s.ends_with("exit $miss"));
        // The launch probe only runs when everything else is there: it would only repeat the news.
        assert!(s.contains("if [ $miss = 0 ]"));
    }

    // `atlas crew update` refreshes the capture runner of a server that already has the browsers
    // (hostd now asks the runner for the detectors, and an old runner writes no findings). It writes
    // exactly the two files the runner is made of, with the same script install uses, and touches
    // neither the package, its lock, npm nor the browsers.
    #[test]
    fn the_refresh_rewrites_the_runner_and_the_detectors_and_nothing_else() {
        let files = refresh_files();
        let names: Vec<_> = files.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["runner.mjs", "detectors.mjs"]);
        assert_eq!(files[0].1, RUNNER_MJS);
        assert_eq!(files[1].1, DETECTORS_MJS);
        // Install writes these same two, from the same list, plus the package and its lock.
        let installed: Vec<_> = capture_files().iter().map(|(n, _)| *n).collect();
        assert_eq!(
            installed,
            [
                "package.json",
                "package-lock.json",
                "runner.mjs",
                "detectors.mjs"
            ]
        );
        for (n, text) in &files {
            assert!(capture_files().iter().any(|(m, t)| m == n && t == text));
        }
    }

    // No root, no npm, no download, no delete: the refresh is `capture_present_script` (a `test -f`)
    // and `write_file_script` per file, which is `sudo -u "$u" … cat >`.
    #[test]
    fn the_refresh_scripts_run_nothing_as_root_and_install_nothing() {
        let mut scripts = vec![("present", capture_present_script())];
        for (n, _) in refresh_files() {
            scripts.push(("write", write_file_script(n)));
        }
        for (name, s) in scripts {
            assert!(s.starts_with(&format!("u=$({WHICH_USER});")), "{name}: {s}");
            assert_eq!(
                count(&s, "sudo "),
                count(&s, "sudo -u \"$u\" "),
                "{name}: {s}"
            );
            for bad in [
                "npm",
                "npx",
                "playwright install",
                "apt-get",
                "rm ",
                "mkdir",
                ">>",
            ] {
                assert!(!s.contains(bad), "{name} contains {bad}: {s}");
            }
        }
    }

    // On a real shell against a temp "server": with the capture dir present an old runner is
    // replaced by this atlas's two files; without it nothing is written, not even the directory.
    #[cfg(unix)]
    #[test]
    fn the_refresh_replaces_an_old_runner_and_leaves_a_server_without_capture_alone() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        let cap = tmp.path().join("cap");
        std::fs::create_dir_all(&bin).unwrap();
        for (name, body) in [
            ("sudo", "[ \"$1\" = -u ] && shift && shift; exec \"$@\""),
            ("id", "exit 0"),
        ] {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
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
            c.stdin
                .take()
                .unwrap()
                .write_all(stdin.unwrap_or("").as_bytes())
                .unwrap();
            c.wait_with_output().unwrap()
        };

        // No capture dir: the probe says so and the caller writes nothing.
        assert!(!sh(&capture_present_script(), None).status.success());
        assert!(!cap.exists());

        // An old install: a runner from before the detectors, a package, a lock, a browsers dir.
        std::fs::create_dir_all(cap.join("browsers")).unwrap();
        std::fs::write(cap.join("runner.mjs"), "// old runner").unwrap();
        std::fs::write(cap.join("package.json"), "{\"old\":true}").unwrap();
        assert!(sh(&capture_present_script(), None).status.success());
        for (n, text) in refresh_files() {
            let o = sh(&write_file_script(n), Some(&text));
            assert!(
                o.status.success(),
                "{n}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        }
        assert_eq!(
            std::fs::read_to_string(cap.join("runner.mjs")).unwrap(),
            RUNNER_MJS
        );
        assert_eq!(
            std::fs::read_to_string(cap.join("detectors.mjs")).unwrap(),
            DETECTORS_MJS
        );
        assert_eq!(
            std::fs::read_to_string(cap.join("package.json")).unwrap(),
            "{\"old\":true}",
            "the package is not the refresh's: it is moved with npm ci by install"
        );
        assert!(cap.join("browsers").is_dir());
    }

    // The scripts are real shell: `sh -n` parses them.
    #[test]
    fn every_script_parses_as_shell() {
        for (name, s) in all_scripts() {
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

    /// `sudo` and `id` stubs in a dir, plus a fake `node` that reports `version`, run the real
    /// node check against it.
    #[cfg(unix)]
    fn run_node_check(version: Option<&str>) -> (Option<i32>, String) {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let stub = |name: &str, body: &str| {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        if let Some(v) = version {
            stub("node", &format!("echo {v}"));
            stub("npm", "echo 10.0.0");
        }
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(node_check_script())
            // Only the stubs and the basics `sh` needs: a real node on this machine is not visible.
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    #[cfg(unix)]
    #[test]
    fn the_node_check_runs_node_18_is_refused_20_and_22_pass() {
        // Skip, saying why, if this machine has node in /usr/bin or /bin (it would hide the stub).
        if std::path::Path::new("/usr/bin/node").exists()
            || std::path::Path::new("/bin/node").exists()
        {
            eprintln!("SKIP the_node_check_runs: a real node in /usr/bin hides the stub");
            return;
        }
        let (code, out) = run_node_check(Some("v18.19.1"));
        assert_eq!(code, Some(1));
        assert_eq!(
            out.trim(),
            "node: Node 18.19.1 found, need >= 20 (install a newer Node.js, for example from nodesource)"
        );
        for ok in ["v20.0.0", "v22.12.0", "v24.19.0"] {
            assert_eq!(run_node_check(Some(ok)), (Some(0), String::new()), "{ok}");
        }
        let (code, out) = run_node_check(None);
        assert_eq!(code, Some(1));
        assert!(
            out.contains("node: missing") && out.contains("npm: missing"),
            "{out}"
        );
        // Garbage from `node --version` is "too old", not a shell error.
        let (code, out) = run_node_check(Some("not-a-version"));
        assert_eq!(code, Some(1), "{out}");
        assert!(out.contains("need >= 20"), "{out}");
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
        let stub = |name: &str, body: &str| {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        // `sudo -u <user> cmd…` -> `cmd…`; `id <user>` succeeds so WHICH_USER prints atlas-crew.
        stub("sudo", "shift; shift; exec \"$@\"");
        stub("id", "exit 0");
        stub("node", "echo v18.19.1");
        stub("npm", "echo 10.0.0");
        let script = status_script().replace(
            "/var/lib/$u/.atlas/capture",
            &tmp.path().join("none").to_string_lossy(),
        );
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&script)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(
            text.contains("node: Node 18.19.1 found, need >= 20"),
            "{text}"
        );
        assert!(text.contains("runner: missing"), "{text}");
        assert!(text.contains("playwright: package missing"), "{text}");
        assert!(text.contains("browsers: chromium missing"), "{text}");
        assert!(text.contains("browsers: webkit missing"), "{text}");
        assert!(
            !text.contains("deps:"),
            "no launch probe while pieces are missing: {text}"
        );
        // Contract with Atlas Desktop (`parseBrowsersStatus`, `crew_browsers_status`): what the command prints for
        // this real output is committed as a fixture both read. `UPDATE_FIXTURES=1` regenerates it.
        let printed = status_report("ubuntu@srv", out.status.code(), &text).unwrap();
        check_fixture("browsers-status-missing.txt", &printed);
        check_fixture(
            "browsers-status-ready.txt",
            &status_report("ubuntu@srv", Some(0), "").unwrap(),
        );
    }

    #[cfg(unix)]
    fn check_fixture(name: &str, fresh: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/commands/crew/fixtures")
            .join(name);
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it");
        assert_eq!(
            committed, fresh,
            "stale fixture {name}: re-run with UPDATE_FIXTURES=1"
        );
    }

    // The text the command prints is a pure function of the host's answer: pieces missing (one line each),
    // all there, and a failure that is not "missing" (nothing on stdout).
    #[test]
    fn status_report_says_what_the_command_prints() {
        assert_eq!(
            status_report("h", Some(1), "node: missing (x)\nrunner: missing\n").unwrap(),
            "\x1b[31m✗\x1b[0m node: missing (x)\n\x1b[31m✗\x1b[0m runner: missing\n"
        );
        assert!(
            status_report("h", Some(0), "")
                .unwrap()
                .contains("capture is ready on h (playwright 1.65.1: chromium, webkit)")
        );
        assert!(status_report("h", Some(1), "").is_err());
        assert!(status_report("h", Some(255), "").is_err());
    }

    /// Run the install scripts for real (node, npm, `npm ci` from the embedded lock against the
    /// registry, the cached browsers linked in for the download) against a temp "server", with
    /// `sudo` and `id` stubbed to run unprivileged, then ask `status`: it must say nothing is
    /// missing, and say what is missing again once a browser is taken away. Skips, printing why,
    /// where node, npm, the network or a Playwright browsers cache is absent.
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
            return skip("no Playwright browsers cache to stand in for the download");
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

        let nc = sh(&node_check_script(), None);
        if !nc.status.success() {
            return skip(&format!(
                "this machine's node is too old for playwright: {}",
                String::from_utf8_lossy(&nc.stdout).trim()
            ));
        }
        // Twice: the second run must change nothing and still succeed.
        for _ in 0..2 {
            ok(&sh(&mkdir_script(), None), "mkdir");
            for (file, text) in [
                ("package.json", package_json()),
                ("package-lock.json", PACKAGE_LOCK.to_string()),
                ("runner.mjs", RUNNER_MJS.to_string()),
                ("detectors.mjs", DETECTORS_MJS.to_string()),
            ] {
                ok(&sh(&write_file_script(file), Some(&text)), file);
            }
            let npm = sh(&npm_ci_script(), None);
            if !npm.status.success() {
                return skip(&format!(
                    "npm ci failed (no network?): {}",
                    String::from_utf8_lossy(&npm.stderr).trim()
                ));
            }
        }
        assert_eq!(
            std::fs::read_to_string(cap.join("runner.mjs")).unwrap(),
            RUNNER_MJS
        );
        assert_eq!(
            std::fs::read_to_string(cap.join("detectors.mjs")).unwrap(),
            DETECTORS_MJS,
            "the runner imports it: without the file the whole capture dies at load"
        );
        // The lock was honoured: the installed package is exactly the pin.
        let pkg =
            std::fs::read_to_string(cap.join("node_modules/playwright/package.json")).unwrap();
        assert!(
            pkg.contains(&format!("\"version\": \"{PLAYWRIGHT_VERSION}\"")),
            "{pkg}"
        );

        // The download is replaced by the cache.
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

        // The dry run is runnable as the crew user and its output is something atlas can read
        // (on macOS it prints nothing, which is not a shape atlas accepts: that is the point of
        // refusing what it does not know).
        let dry = sh(&deps_dry_run_script(), None);
        assert!(dry.status.success(), "the dry-run script always exits 0");

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
