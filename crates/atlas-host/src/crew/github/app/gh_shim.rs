//! `gh` inside a crewmate, through the GitHub App. A symlink named `gh` to the `atlas` binary sits first on the
//! crewmate's `PATH` (`shims`: for a crewmate that is not its own Linux user; the installer links it into the
//! helper's engine directory for the others). When it runs, `atlas` sees it was called as `gh` and, for each call:
//!
//! 1. works out which repository the command targets (`-R`, `GH_REPO`, the path of a `gh api repos/..` call, the
//!    `origin` of the current directory);
//! 2. asks hostd, over the crewmate's own hook endpoint (the route the git credential helper uses), for a fresh
//!    installation token for that repo;
//! 3. runs the real `gh` with `GH_TOKEN` set in that child's environment only.
//!
//! The token never sits in the crewmate's own environment or on disk. Whatever it cannot answer for (no app, a repo
//! no installation covers, a command with no repository, another host, a crewmate with a token of its own) runs the
//! real `gh` untouched, exactly as before.

use std::path::{Path, PathBuf};

use super::helper::{Endpoint, ask};
use crate::crew::project::parse_github_repo;

/// Whether the program was started under the name `gh`.
pub fn invoked_as(argv0: Option<&std::ffi::OsStr>) -> bool {
    argv0
        .map(Path::new)
        .and_then(Path::file_stem)
        .is_some_and(|s| s == "gh")
}

/// `gh` commands that act on a repository, so the `origin` of the current directory is the repository they mean
/// when no flag says otherwise. Anything else (`auth`, `config`, `search`, `gist`...) has no repository.
const REPO_COMMANDS: [&str; 12] = [
    "pr", "issue", "repo", "run", "workflow", "release", "label", "cache", "secret", "variable",
    "ruleset", "browse",
];

/// `gh api` options that take a value as the next argument.
const API_VALUE_FLAGS: [&str; 17] = [
    "-X",
    "--method",
    "-H",
    "--header",
    "-f",
    "--raw-field",
    "-F",
    "--field",
    "--input",
    "-q",
    "--jq",
    "-t",
    "--template",
    "--cache",
    "-p",
    "--preview",
    "--hostname",
];

/// `owner/repo` from what `-R` and `GH_REPO` take (`OWNER/REPO` or `HOST/OWNER/REPO`, github.com only).
fn repo_of_flag(v: &str) -> Option<String> {
    let parts: Vec<&str> = v.split('/').collect();
    match parts.as_slice() {
        [o, r] => super::service::valid_repo(&format!("{o}/{r}")).map(|_| format!("{o}/{r}")),
        [h, o, r] if h.eq_ignore_ascii_case("github.com") => {
            super::service::valid_repo(&format!("{o}/{r}")).map(|_| format!("{o}/{r}"))
        }
        _ => None,
    }
}

/// The repository a `gh` command line targets, `None` when it names none (or not one on github.com).
pub fn target_repo(
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
) -> Option<String> {
    if env("GH_HOST").is_some_and(|h| !h.eq_ignore_ascii_case("github.com")) {
        return None;
    }
    // An explicit repository wins, whatever the command.
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--" {
            break;
        }
        if a == "-R" || a == "--repo" {
            return it.next().and_then(|v| repo_of_flag(v));
        }
        if let Some(v) = a.strip_prefix("--repo=").or_else(|| a.strip_prefix("-R")) {
            return repo_of_flag(v);
        }
    }
    if let Some(r) = env("GH_REPO").filter(|r| !r.is_empty()) {
        return repo_of_flag(&r);
    }
    let first = args.iter().find(|a| !a.starts_with('-'))?;
    if first == "api" {
        return api_repo(args, cwd);
    }
    REPO_COMMANDS
        .contains(&first.as_str())
        .then(|| origin_repo(cwd))
        .flatten()
}

/// The repository of a `gh api` call: the `repos/<owner>/<repo>` of its endpoint, or, with the `{owner}/{repo}`
/// placeholders gh fills in itself, the current directory's. Any other endpoint (`user`, `graphql`, `orgs/..`) has none.
fn api_repo(args: &[String], cwd: &Path) -> Option<String> {
    let mut it = args.iter().skip_while(|a| *a != "api").skip(1);
    let endpoint = loop {
        let a = it.next()?;
        if a.starts_with('-') {
            if API_VALUE_FLAGS.contains(&a.as_str()) {
                it.next();
            }
            continue;
        }
        break a;
    };
    let mut seg = endpoint.trim_start_matches('/').split('/');
    if seg.next()? != "repos" {
        return None;
    }
    let (owner, repo) = (seg.next()?, seg.next()?);
    if owner == "{owner}" && repo == "{repo}" {
        return origin_repo(cwd);
    }
    super::service::valid_repo(&format!("{owner}/{repo}")).map(|_| format!("{owner}/{repo}"))
}

/// `owner/repo` of `origin` in `cwd`.
fn origin_repo(cwd: &Path) -> Option<String> {
    // tree-access: this runs as the crewmate, in its own checkout: it is its git, answering its own question
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| parse_github_repo(String::from_utf8_lossy(&out.stdout).trim()))
        .flatten()
}

/// The first `gh` on `path` that is not this binary under another name (the shim's own symlink).
pub fn find_real(path: &std::ffi::OsStr, self_exe: &Path) -> Option<PathBuf> {
    // tree-access: this runs as the crewmate (`gh` itself), looking along its own PATH
    let me = std::fs::canonicalize(self_exe).ok();
    std::env::split_paths(path)
        .map(|d| d.join("gh"))
        // tree-access: as above
        .filter(|c| c.is_file())
        // tree-access: as above
        .find(|c| me.is_none() || std::fs::canonicalize(c).ok() != me)
}

/// What to run.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The installation token for the child's `GH_TOKEN`, when the app covers the repo the command targets.
    pub token: Option<String>,
    /// The repository the token is for.
    pub repo: Option<String>,
}

/// Decide how to run `gh <args>` in `cwd`. `Err` only when there is no real `gh` to run.
pub fn plan(
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    path: &std::ffi::OsStr,
    self_exe: &Path,
) -> Result<Launch, String> {
    let program =
        find_real(path, self_exe).ok_or("gh is not installed (no gh found on PATH)".to_string())?;
    let untouched = |program: PathBuf| Launch {
        program,
        args: args.to_vec(),
        token: None,
        repo: None,
    };
    // A token of the crewmate's own stays its own.
    if env("GH_TOKEN").is_some() || env("GITHUB_TOKEN").is_some() {
        return Ok(untouched(program));
    }
    let (Some(ep), Some(repo)) = (Endpoint::from_env(env), target_repo(args, env, cwd)) else {
        return Ok(untouched(program));
    };
    let body = serde_json::json!({"pane_key": ep.pane_key, "host": "github.com", "path": repo});
    // Not covered (404), no app, hostd gone: the real gh, untouched. It may have a login of its own.
    let token = ask(&ep, &body.to_string(), std::time::Duration::from_secs(25))
        .ok()
        .and_then(|v| v["password"].as_str().map(str::to_string));
    Ok(Launch {
        program,
        args: args.to_vec(),
        repo: token.as_ref().map(|_| repo),
        token,
    })
}

/// Run as `gh`: plan, then become the real `gh` (on Unix `exec`, so signals and the terminal are its own), with
/// `GH_TOKEN` in its environment only. Returns the exit code when it could not become it.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Ok(cwd), Ok(me)) = (std::env::current_dir(), std::env::current_exe()) else {
        // i18n-ignore: stderr of the gh shim, read in the agent's log
        eprintln!("gh: cannot tell where it runs");
        return 127;
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    let launch = match plan(&args, &|k| std::env::var(k).ok(), &cwd, &path, &me) {
        Ok(l) => l,
        Err(e) => {
            // i18n-ignore: stderr of the gh shim, read in the agent's log
            eprintln!("{e}");
            return 127;
        }
    };
    // tree-access: becomes the real `gh` as the crewmate; never run by hostd
    let mut cmd = std::process::Command::new(&launch.program);
    cmd.args(&launch.args);
    if let Some(t) = &launch.token {
        cmd.env("GH_TOKEN", t);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        // i18n-ignore: stderr of the gh shim, read in the agent's log
        eprintln!("gh: {e}");
        126
    }
    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                // i18n-ignore: stderr of the gh shim, read in the agent's log
                eprintln!("gh: {e}");
                126
            }
        }
    }
}

/// `<crew dir>/shims`, with `gh` in it linked to `exe` (the `atlas` binary): the directory a crewmate that is not its
/// own Linux user gets first on its `PATH`. A crewmate that is its own user cannot have `PATH` set by hostd (the spawn
/// helper pins it), so the installer links `gh` into the helper's engine directory instead (`atlas crew install`).
#[cfg(unix)]
pub(crate) fn ensure_dir(crew_dir: &Path, exe: &Path) -> std::io::Result<PathBuf> {
    let dir = crew_dir.join("shims");
    // tree-access: hostd's own directory in the crew dir, never a crewmate's tree
    std::fs::create_dir_all(&dir)?;
    let link = dir.join("gh");
    // tree-access: hostd's own symlink in its own shims directory
    if std::fs::read_link(&link).is_ok_and(|t| t == exe) {
        return Ok(dir);
    }
    // tree-access: replaced in place (a new binary path after an upgrade), still hostd's own directory
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(exe, &link)?;
    Ok(dir)
}
