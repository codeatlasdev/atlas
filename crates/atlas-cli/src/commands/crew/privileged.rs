//! Running privileged steps on a crew server, with or without a sudo password.
//!
//! Passwordless sudo: the [`Remote`] is used as it is. Password sudo (Atlas Desktop sends the password on
//! stdin): `sudo -v` once is not enough, because Debian and Ubuntu default to `tty_tickets` and every ssh
//! exec has its own tty, so the cached credential of one call is useless in the next. Every call here carries
//! the password itself, as the first line of its stdin (never argv, never in the script text); see
//! [`binary::with_sudo_password`].

use anyhow::{Context, Result};

use crate::commands::deploy::remote::{Output, Remote};

use super::binary;

pub struct Priv<'a> {
    pub r: &'a Remote,
    password: Option<String>,
}

impl<'a> Priv<'a> {
    pub fn passwordless(r: &'a Remote) -> Self {
        Self { r, password: None }
    }

    fn wrap(&self, script: &str, data: Option<&[u8]>) -> (String, Option<Vec<u8>>) {
        match &self.password {
            None => (script.to_owned(), data.map(<[u8]>::to_vec)),
            Some(pw) => {
                let mut stdin = format!("{pw}\n").into_bytes();
                stdin.extend_from_slice(data.unwrap_or_default());
                (binary::with_sudo_password(script), Some(stdin))
            }
        }
    }

    /// Retried (1s, 3s) only when ssh itself fails (255). Every script run through here must be safe to run
    /// twice, the swap included ([`binary::swap_script`] never overwrites `atlas.prev` on a re-run).
    pub async fn exec(&self, script: &str) -> Result<Output> {
        self.exec_stdin(script, None).await
    }

    pub async fn exec_stdin(&self, script: &str, data: Option<&[u8]>) -> Result<Output> {
        let (s, d) = self.wrap(script, data);
        self.r.exec_stdin(&s, d.as_deref()).await
    }

    /// Make sudo usable for the rest of the run and return the wrapper to use for it.
    ///
    /// `password_stdin` (Atlas Desktop): read one line from stdin and check it against the server with a real
    /// `sudo` call; every later call re-sends it. Otherwise require passwordless sudo and point at `terminal_cmd`.
    pub async fn prepare(
        r: &'a Remote,
        ssh_target: &str,
        password_stdin: bool,
        terminal_cmd: &str,
    ) -> Result<Priv<'a>> {
        if !password_stdin {
            let check = r.exec("sudo -n true").await?;
            if check.code != 0 {
                return Err(atlas_core::progress::Coded::new(
                    "sudo.password_required",
                    serde_json::json!({ "target": ssh_target, "command": terminal_cmd }),
                    format!(
                        "sudo on {ssh_target} asks for a password — grant passwordless sudo to this user, \
or run it from a terminal: {terminal_cmd}"
                    ),
                )
                .into());
            }
            return Ok(Self::passwordless(r));
        }
        use tokio::io::AsyncBufReadExt as _;
        let mut pw = String::new();
        tokio::io::BufReader::new(tokio::io::stdin())
            .read_line(&mut pw)
            .await
            .context("reading sudo password from stdin")?;
        let pw = pw.trim_end_matches('\n').trim_end_matches('\r').to_owned();
        let p = Self {
            r,
            password: Some(pw),
        };
        if p.exec("sudo true").await?.code != 0 {
            return Err(atlas_core::progress::Coded::new(
                "sudo.password_rejected",
                serde_json::json!({ "target": ssh_target }),
                "sudo password rejected — check your password and try again",
            )
            .into());
        }
        Ok(p)
    }
}
