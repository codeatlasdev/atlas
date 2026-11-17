//! The `SSH_ASKPASS` helper for "sign in to the server with a password" (#517).
//!
//! `ssh` asks a program for a password when `SSH_ASKPASS` names one (with `SSH_ASKPASS_REQUIRE=force`, even with
//! no terminal). That program must be an executable file on every OS, and it must get the password from somewhere
//! that is not argv (visible to every process) and not a file (a script with the password in it, on disk).
//!
//! The program is **this very executable** (Atlas Desktop): started by `ssh` with [`SOCK_ENV`] set, `main` hands
//! over to [`run_if_helper`] before anything else, which fetches the password from a one-shot local endpoint
//! ([`Askpass`]: a Unix socket in a private directory, a named pipe on Windows, both through `atlas_core::ipc`),
//! prints it and exits. Why this and not the alternatives:
//! - a `#!/bin/sh` script cannot run on Windows, and a `.cmd` needs the password written to disk (and quoting that
//!   `cmd.exe` gets wrong for `%`, `^`, `"`);
//! - an inherited pipe or the stdin of ssh does not reach the helper: OpenSSH closes every descriptor above 2 in the
//!   askpass child, and the stdin of ssh carries the key being installed;
//! - the environment would hand the password to anything of the same user that reads `/proc/<pid>/environ` or runs
//!   `ps eww`; here the environment carries only the socket path, and the password crosses the socket once.
//!
//! What it does not defend against: a process of the same user that connects to the socket before the helper does
//! (it can read the memory of Atlas as well). The endpoint serves at most [`MAX_SERVES`] connections (the
//! keyboard-interactive and the password method of one login) and goes away when the ssh call ends.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas_core::ipc;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Set on the `ssh` child: the endpoint the helper fetches the password from.
pub const SOCK_ENV: &str = "ATLAS_ASKPASS_SOCK";

/// One login asks for the password at most twice (keyboard-interactive, then password): one more is slack.
const MAX_SERVES: usize = 3;

/// What the helper accepts as a password (bytes): far more than anyone types, far less than memory.
const MAX_PASSWORD: usize = 1 << 20;

/// A password that `ssh` can ask for, for as long as this value lives.
pub struct Askpass {
    // Held for its Drop: removes the directory (and the socket in it).
    _dir: tempfile::TempDir,
    sock: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

impl Askpass {
    /// Start serving `password`. Must be called inside the tokio runtime.
    pub fn serve(password: String) -> io::Result<Self> {
        // Owner-only (0700) from the first moment, so nobody else can reach the socket in it. (`tempdir` alone
        // made it 0755: the test of this module caught that.)
        let mut builder = tempfile::Builder::new();
        builder.prefix("atlas-askpass-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let dir = builder.tempdir()?;
        let sock = dir.path().join("s");
        let listener = ipc::Listener::bind(&sock)?;
        let task = tokio::spawn(async move {
            for _ in 0..MAX_SERVES {
                let Ok(mut conn) = listener.accept().await else {
                    return;
                };
                // Length first, so the client knows where the password ends without waiting for the server to
                // close; and the server then waits for the CLIENT to close. A Windows named pipe discards unread
                // data when the server end disconnects (`DisconnectNamedPipe`), so closing first lost the password.
                let mut frame = (password.len() as u32).to_be_bytes().to_vec();
                frame.extend_from_slice(password.as_bytes());
                let _ = conn.write_all(&frame).await;
                let _ = conn.flush().await;
                let _ =
                    tokio::time::timeout(Duration::from_secs(10), conn.read(&mut [0u8; 1])).await;
            }
        });
        Ok(Self {
            _dir: dir,
            sock,
            task,
        })
    }

    /// The endpoint, for [`SOCK_ENV`].
    pub fn sock(&self) -> &Path {
        &self.sock
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The first thing `main` does: when `ssh` started this process as its askpass program, answer and return the exit
/// code; `None` for a normal launch of the app.
pub fn run_if_helper() -> Option<i32> {
    helper(
        std::env::var_os(SOCK_ENV),
        std::env::args().nth(1),
        &mut io::stdout(),
    )
}

/// `ssh` passes the prompt as the only argument. Only a password prompt gets the password: a passphrase prompt, or
/// "are you sure you want to continue connecting", must never receive it (it would be sent to the wrong question).
fn is_password_prompt(prompt: &str) -> bool {
    prompt.to_lowercase().contains("password")
}

fn helper(sock: Option<OsString>, prompt: Option<String>, out: &mut impl Write) -> Option<i32> {
    let sock = sock?;
    if !is_password_prompt(&prompt.unwrap_or_default()) {
        return Some(1);
    }
    match fetch(Path::new(&sock))
        .and_then(|pw| out.write_all(pw.as_bytes()).and_then(|()| out.flush()))
    {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("atlas askpass: {e}");
            Some(1)
        }
    }
}

fn fetch(sock: &Path) -> io::Result<String> {
    let mut conn = ipc::connect_sync(sock)?;
    conn.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut len = [0u8; 4];
    conn.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_PASSWORD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "password too long",
        ));
    }
    let mut pw = vec![0u8; len];
    conn.read_exact(&mut pw)?;
    String::from_utf8(pw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the helper against `sock` off the async threads, like a separate process would.
    async fn ask(sock: &Path, prompt: &str) -> (Option<i32>, Vec<u8>) {
        let (sock, prompt) = (sock.as_os_str().to_owned(), prompt.to_owned());
        tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            let code = helper(Some(sock), Some(prompt), &mut out);
            (code, out)
        })
        .await
        .unwrap()
    }

    // The password reaches ssh byte for byte: quotes, `%`, `^`, `"` and non-ASCII are what a .cmd or .sh helper
    // gets wrong, and ssh strips the trailing newline itself, so none is added.
    #[tokio::test]
    async fn the_helper_prints_the_password_exactly_as_typed() {
        let pw = "pa'ss \"%^& ç 密\\";
        let askpass = Askpass::serve(pw.to_string()).unwrap();
        let (code, out) = ask(askpass.sock(), "ubuntu@10.0.0.5's password: ").await;
        assert_eq!(code, Some(0));
        assert_eq!(String::from_utf8(out).unwrap(), pw);
    }

    // A passphrase or host-key question must never get the password, and asking it does not use up the endpoint.
    #[tokio::test]
    async fn only_a_password_prompt_gets_the_password() {
        let askpass = Askpass::serve("s3cret".into()).unwrap();
        for prompt in [
            "Enter passphrase for key '/home/me/.ssh/id_ed25519': ",
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
            "",
        ] {
            let (code, out) = ask(askpass.sock(), prompt).await;
            assert_eq!((code, out.as_slice()), (Some(1), &b""[..]), "{prompt:?}");
        }
        let (code, out) = ask(askpass.sock(), "Password: ").await;
        assert_eq!((code, out.as_slice()), (Some(0), &b"s3cret"[..]));
    }

    // Bounded: a login asks twice at most; a process that keeps knocking does not get the password forever.
    #[tokio::test]
    async fn the_endpoint_serves_a_few_times_and_then_nothing() {
        let askpass = Askpass::serve("s3cret".into()).unwrap();
        for _ in 0..MAX_SERVES {
            assert_eq!(ask(askpass.sock(), "Password: ").await.0, Some(0));
        }
        let (code, out) = ask(askpass.sock(), "Password: ").await;
        assert_eq!((code, out.as_slice()), (Some(1), &b""[..]));
    }

    // When the ssh call is over nothing is left: the directory with the endpoint is gone, so there is nothing to
    // connect to and nothing on disk. (On Windows the pipe ends with the task.)
    #[tokio::test]
    async fn dropping_the_endpoint_leaves_nothing_behind() {
        let askpass = Askpass::serve("s3cret".into()).unwrap();
        let sock = askpass.sock().to_owned();
        let dir = sock.parent().unwrap().to_owned();
        assert!(dir.exists());
        drop(askpass);
        assert!(!dir.exists());
        assert_eq!(ask(&sock, "Password: ").await.0, Some(1));
    }

    // The app is the helper only when ssh started it so: a normal launch has no SOCK_ENV.
    #[test]
    fn a_normal_launch_is_not_the_helper() {
        let mut out = Vec::new();
        assert_eq!(helper(None, Some("Password: ".into()), &mut out), None);
        assert!(out.is_empty());
    }

    // The password is read from a socket the user owns alone: nobody else can connect to it (Unix).
    #[cfg(unix)]
    #[tokio::test]
    async fn the_endpoint_lives_in_a_directory_only_the_user_can_enter() {
        use std::os::unix::fs::PermissionsExt as _;
        let askpass = Askpass::serve("x".into()).unwrap();
        let mode = std::fs::metadata(askpass.sock().parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
}
