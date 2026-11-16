//! hostd's own bare mirror of a crewmate's project: how a crewmate that runs as its own user gets a private
//! repository without ever holding the operator's GitHub token.
//!
//! The crew user's `gh` login reaches every repository the operator's account can, and a crewmate controls its
//! checkout (`.git/config`, hooks, its `~/.gitconfig`, `/proc/<its git>/environ`), so a process that runs as a
//! crewmate must never be given that token (security review of the crew isolation, N2). Instead:
//!
//! - a crewmate with a `GITHUB_TOKEN` secret of its own uses that (`Git::with_token`): its own credential, its own
//!   reach;
//! - any other gets a mirror: a bare repository at `<crewmate dir>/crew-runtime/<project>.git`, made and updated by
//!   **hostd itself** with the token, in hostd's own process with a cleared environment, no hooks and no
//!   configuration but its own (`core.hooksPath=/dev/null`, `GIT_CONFIG_GLOBAL=/dev/null`): none of it is code the
//!   crewmate wrote. `crew-runtime/` belongs to the crew user and is `0750` with the crewmate's group, so the
//!   crewmate can walk into the mirror and read it and can change nothing in it. The crewmate clones and fetches
//!   from `file://<mirror>` with no token at all, and its `origin` pushes to the real URL (`remote.origin.pushurl`).
//!
//! The mirror holds `refs/heads/*` and `refs/tags/*` (not GitHub's pull refs) and is per crewmate, so a private
//! project is readable only by the crewmates that have it.

use std::path::{Path, PathBuf};

use super::proc;
use super::spawn::layout::Isolated;

/// Answers any host's password prompt from the environment. Reset first: hostd's git must use nothing of the
/// crew user's own setup.
const HELPER: &str =
    "!f() { echo username=x-access-token; echo \"password=$ATLAS_GIT_TOKEN\"; }; f";
/// What hostd's mirror git may talk: never `ext::` or anything else that runs a command named in a URL.
const PROTOCOLS: &str = "file:http:https:ssh:git";

/// The mirror of one project for one isolated crewmate.
#[derive(Debug, Clone)]
pub struct Mirror {
    url: String,
    dir: PathBuf,
}

impl Mirror {
    /// The mirror of `url` (already `https`, see `project::ssh_to_https`) for crewmate `iso`.
    pub fn of(url: &str, iso: &Isolated) -> Self {
        Self {
            url: url.to_string(),
            dir: iso
                .runtime()
                .join(format!("{}.git", super::project::git_slug(url))),
        }
    }

    #[cfg(all(test, unix))]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the crewmate clones and fetches from.
    pub fn local_url(&self) -> String {
        format!("file://{}", self.dir.display())
    }

    /// Bring the mirror up to date with the crew user's `gh` login (none: anonymously), making it first if it is
    /// not there.
    pub async fn refresh(&self) -> Result<(), String> {
        self.refresh_with(login_token().await).await
    }

    /// [`Mirror::refresh`] with `token` (hostd's, never a crewmate's).
    pub async fn refresh_with(&self, token: Option<String>) -> Result<(), String> {
        if self.dir.join("HEAD").is_file() {
            let dir = self.dir.to_string_lossy().into_owned();
            return git(
                &[
                    "--git-dir",
                    &dir,
                    "fetch",
                    "--prune",
                    "--quiet",
                    "origin",
                    "+refs/heads/*:refs/heads/*",
                    "+refs/tags/*:refs/tags/*",
                ],
                token,
            )
            .await;
        }
        // Into a name of its own, renamed when complete: a clone that failed halfway is never a mirror a crewmate reads.
        let partial = self.dir.with_extension("git.partial");
        let _ = std::fs::remove_dir_all(&partial);
        std::fs::create_dir_all(self.dir.parent().unwrap_or(Path::new("/")))
            .map_err(|e| format!("mkdir error: {e}"))?;
        let (from, to) = (self.url.clone(), partial.to_string_lossy().into_owned());
        let made = git(&["clone", "--bare", "--quiet", &from, &to], token).await;
        if let Err(e) = made {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(e);
        }
        // The crewmate clones with `--filter=blob:none` and fetches blobs from here later.
        for (k, v) in [
            ("uploadpack.allowFilter", "true"),
            ("uploadpack.allowAnySHA1InWant", "true"),
        ] {
            git(&["--git-dir", &to, "config", k, v], None).await?;
        }
        std::fs::rename(&partial, &self.dir).map_err(|e| format!("mirror: {e}"))
    }
}

/// The crew user's own `gh` login, for hostd's own git only.
async fn login_token() -> Option<String> {
    let out = proc::gh_output(&["auth", "token"], "gh auth token")
        .await
        .ok()?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !t.is_empty()).then_some(t)
}

/// hostd's own `git <args>`: cleared environment, no hooks, no global or system configuration, the token (if
/// any) only in this process's environment, files readable by the crewmate's group tree (umask 022).
async fn git(args: &[&str], token: Option<String>) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=",
        "-c",
        "credential.helper=",
        "-c",
        &format!("credential.helper={HELPER}"),
    ])
    .args(args)
    .env_clear()
    .env("PATH", "/usr/local/bin:/usr/bin:/bin")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_ALLOW_PROTOCOL", PROTOCOLS);
    if let Some(t) = token {
        cmd.env("ATLAS_GIT_TOKEN", t);
    }
    #[cfg(unix)]
    // SAFETY: only `umask(2)`, which is async-signal-safe, runs between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::umask(0o022);
            Ok(())
        });
    }
    let out = proc::run_bounded(&mut cmd, "git", proc::call_deadline()).await?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// `Some(mirror)` when `crewmate`'s project is a Git repository, the crewmate runs as its own user and has no
/// GitHub token of its own: the case where hostd, not the crewmate, talks to GitHub.
pub fn mirror_for(
    project: &super::store::ProjectRef,
    iso: Option<&Isolated>,
    git: &super::gitrun::Git,
) -> Option<Mirror> {
    let super::store::ProjectRef::Git { url, .. } = project else {
        return None;
    };
    let iso = iso?;
    (git.is_crewmate() && !git.has_own_token())
        .then(|| Mirror::of(&super::project::ssh_to_https(url), iso))
}

#[cfg(all(test, unix))]
mod tests;
