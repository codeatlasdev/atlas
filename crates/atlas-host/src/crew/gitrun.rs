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
    Crewmate {
        client: Client,
        user: String,
        /// The crewmate's **own** GitHub token (its `GITHUB_TOKEN` secret), the only one its git processes ever get.
        token: Option<String>,
    },
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
            token: None,
        })
    }

    /// This git with the crewmate's own GitHub token, for the commands that talk to the network. Nothing else is
    /// ever a crewmate's token: the crew user's `gh` login is never given to a process that runs as a crewmate (it
    /// reaches every repository the operator's account can); without a token of its own the crewmate clones and
    /// fetches from a mirror hostd keeps ([`super::mirror`]). No effect on [`Git::local`].
    pub fn with_token(self, own: Option<String>) -> Self {
        #[cfg(unix)]
        {
            let mut git = self;
            if let How::Crewmate { token, .. } = &mut git.0 {
                *token = own.filter(|t| !t.is_empty());
            }
            git
        }
        #[cfg(not(unix))]
        {
            let _ = own;
            self
        }
    }

    /// Whether this crewmate has a GitHub token of its own (always `false` for [`Git::local`]).
    pub fn has_own_token(&self) -> bool {
        match &self.0 {
            How::Local => false,
            #[cfg(unix)]
            How::Crewmate { token, .. } => token.is_some(),
        }
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
            How::Crewmate { client, user, .. } => ask(client, request(user, cwd, args, None)),
        }
    }

    /// [`Git::run_sync`], bounded by [`proc::call_deadline`] and ended with its process tree when the
    /// future is dropped (a cancelled task). `cwd: None` is hostd's own directory (only for
    /// [`Git::local`]: a crewmate's git always has a tree).
    pub async fn run(&self, cwd: Option<&Path>, args: &[&str]) -> Result<Output, String> {
        self.run_with(cwd, args, false).await
    }

    /// [`Git::run`] for a command that talks to the network (`clone`, `fetch`, `pull`). Run as a
    /// crewmate it gets the crewmate's own GitHub token if it has one ([`Git::with_token`]) and none otherwise.
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
                // hostd's own git: its own setup (the `gh` credential helper) answers a password prompt.
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
            How::Crewmate {
                client,
                user,
                token,
            } => {
                let cwd = cwd.ok_or("git error: no directory to run in")?;
                let token = if net { token.clone() } else { None };
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
