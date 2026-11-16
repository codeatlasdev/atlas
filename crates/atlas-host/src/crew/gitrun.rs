//! Every git command hostd runs on a crew checkout goes through [`Git`], which decides who runs it.
//!
//! A crewmate that runs as its own Linux user controls its tree, `.git/config` included, and git runs
//! what that config names (`core.fsmonitor`, filters, `core.sshCommand`, credential helpers). So hostd, the
//! crew user, never runs git in such a tree: [`Git::crewmate`] asks the spawn helper to run it as the
//! crewmate (`Request::Git`), where the crewmate's own command is only the crewmate's. Everywhere else
//! (no isolation, a project path on the server) git runs here as it always did. Security review of #448, B2.

use std::path::Path;
use std::process::Output;

use super::proc;
use super::project::git_program;
#[cfg(unix)]
use super::spawn::{client::Client, protocol::GitReq};

/// Who runs git.
#[derive(Debug, Clone)]
pub struct Git(How);

#[derive(Debug, Clone)]
enum How {
    /// hostd itself: a tree nobody else controls.
    Local,
    /// The crewmate `user`, through the spawn helper.
    #[cfg(unix)]
    Crewmate { client: Client, user: String },
}

impl Git {
    /// Git run by hostd, for a tree that is not an isolated crewmate's.
    pub fn local() -> Self {
        Self(How::Local)
    }

    /// Git run as `user` by the spawn helper listening on `socket`.
    #[cfg(unix)]
    pub fn crewmate(socket: &Path, user: String) -> Self {
        Self(How::Crewmate {
            client: Client::at(socket),
            user,
        })
    }

    /// Whether the helper runs it as a crewmate.
    pub fn is_crewmate(&self) -> bool {
        match &self.0 {
            How::Local => false,
            #[cfg(unix)]
            How::Crewmate { .. } => true,
        }
    }

    /// `git <args>` in `cwd` to the end, without network credentials. Blocking: for the callers that
    /// are not async (the guards of an attachment, the repo check).
    pub fn run_sync(&self, cwd: &Path, args: &[&str]) -> Result<Output, String> {
        match &self.0 {
            // Not `git_program()`: that is the test override of the checkout, which a fake may hang.
            How::Local => std::process::Command::new("git")
                .args(args)
                .current_dir(cwd)
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| format!("git error: {e}")),
            #[cfg(unix)]
            How::Crewmate { client, user } => ask(client, request(user, cwd, args, None)),
        }
    }

    /// [`Git::run_sync`], bounded by [`proc::call_deadline`] and ended with its process tree when the
    /// future is dropped (a cancelled task). `cwd: None` is hostd's own directory (only for
    /// [`Git::local`]: a crewmate's git always has a tree).
    pub async fn run(&self, cwd: Option<&Path>, args: &[&str]) -> Result<Output, String> {
        self.run_with(cwd, args, false).await
    }

    /// [`Git::run`] for a command that talks to the network (`clone`, `fetch`, `pull`). Run as a
    /// crewmate it gets the server's GitHub token for this one process (the crew user's `gh` login is
    /// what private repos were always cloned with; the crewmate has no login of its own).
    pub async fn run_net(&self, cwd: Option<&Path>, args: &[&str]) -> Result<Output, String> {
        self.run_with(cwd, args, true).await
    }

    async fn run_with(
        &self,
        cwd: Option<&Path>,
        args: &[&str],
        net: bool,
    ) -> Result<Output, String> {
        match &self.0 {
            How::Local => {
                // Only a crewmate's git gets the server's token.
                let _ = net;
                let mut cmd = tokio::process::Command::new(git_program());
                cmd.args(args);
                if let Some(d) = cwd {
                    cmd.current_dir(d);
                }
                proc::run_bounded(&mut cmd, "git", proc::call_deadline())
                    .await
                    .map_err(|e| format!("git error: {e}"))
            }
            #[cfg(unix)]
            How::Crewmate { client, user } => {
                let cwd = cwd.ok_or("git error: no directory to run in")?;
                let token = if net { github_token().await } else { None };
                let (client, req) = (client.clone(), request(user, cwd, args, token));
                tokio::task::spawn_blocking(move || ask(&client, req))
                    .await
                    .map_err(|e| format!("git error: {e}"))?
            }
        }
    }
}

#[cfg(unix)]
fn request(user: &str, cwd: &Path, args: &[&str], token: Option<String>) -> GitReq {
    GitReq {
        user: user.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: cwd.to_string_lossy().into_owned(),
        token,
        timeout_secs: proc::call_deadline().as_secs().max(1),
    }
}

#[cfg(unix)]
fn ask(client: &Client, req: GitReq) -> Result<Output, String> {
    use std::os::unix::process::ExitStatusExt;
    let secs = req.timeout_secs;
    let out = client.git(&req).map_err(|e| format!("git error: {e}"))?;
    if out.timed_out {
        return Err(format!("git error: git: timed out after {secs}s"));
    }
    Ok(Output {
        // The wait status: the code in the high byte, or the signal in the low one.
        status: std::process::ExitStatus::from_raw(out.code.map_or(9, |c| (c & 0xff) << 8)),
        stdout: out.stdout,
        stderr: out.stderr.into_bytes(),
    })
}

/// The GitHub token of the crew user's `gh` login, if there is one.
#[cfg(unix)]
async fn github_token() -> Option<String> {
    let out = proc::gh_output(&["auth", "token"], "gh auth token")
        .await
        .ok()?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !t.is_empty()).then_some(t)
}
