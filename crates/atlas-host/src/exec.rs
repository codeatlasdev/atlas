//! One-shot commands (`git`, `gh`, `du`…) with a timeout and capped output.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use atlas_core::sys::CommandExt;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const OUT_CAP: usize = 10 * 1024 * 1024;

/// Run `cmd`'s child in its own session (`setsid`): it survives our terminal/app and
/// has no controlling tty. Windows: its own process group and a hidden console.
pub(crate) fn new_session(cmd: &mut std::process::Command) {
    atlas_core::sys::detach(cmd);
}

/// The program to start for `bin`. Windows: a bare name is looked up on the request's `PATH`
/// (else ours) with its executable extension, so `npm` finds `npm.cmd`; Rust would only try
/// `npm.exe`.
fn program(bin: &str, env: &BTreeMap<String, String>) -> std::path::PathBuf {
    if cfg!(windows) && !bin.contains(['/', '\\']) {
        let path = env
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
            .map(|(_, v)| std::ffi::OsString::from(v))
            .or_else(|| std::env::var_os("PATH"));
        if let Some(found) = path.and_then(|p| atlas_core::paths::which_in(bin, &p)) {
            return found;
        }
    }
    bin.into()
}

async fn read_capped(mut r: impl AsyncRead + Unpin) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    while let Ok(n) = r.read(&mut chunk).await {
        if n == 0 {
            break;
        }
        // Keep draining past the cap so the child never blocks on a full pipe.
        let room = OUT_CAP.saturating_sub(out.len());
        out.extend_from_slice(&chunk[..n.min(room)]);
    }
    out
}

pub async fn run(
    cwd: &str,
    argv: &[String],
    env: &BTreeMap<String, String>,
    stdin: Option<&str>,
    timeout_ms: u64,
) -> Result<Value, String> {
    let (bin, args) = argv.split_first().ok_or("empty argv")?;
    let input = stdin
        .map(|s| B64.decode(s).map_err(|e| format!("stdin: {e}")))
        .transpose()?;
    // Windows: a shell-form argv goes to cmd verbatim (its quoting is not MSVC's).
    let mut cmd = match atlas_core::sys::shell_script(argv) {
        Some(script) => atlas_core::sys::shell_tokio(script),
        None => {
            let mut c = tokio::process::Command::new(program(bin, env));
            c.args(args);
            c
        }
    };
    let mut child = cmd
        .no_window()
        .current_dir(cwd)
        .envs(atlas_core::login_env::vars())
        .envs(env)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("{bin}: {e}"))?;
    if let (Some(data), Some(mut w)) = (input, child.stdin.take()) {
        tokio::spawn(async move {
            let _ = w.write_all(&data).await;
        });
    }
    let out = tokio::spawn(read_capped(child.stdout.take().expect("piped")));
    let err = tokio::spawn(read_capped(child.stderr.take().expect("piped")));
    let (code, timed_out) =
        match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait()).await {
            Ok(st) => (st.ok().and_then(|s| s.code()), false),
            Err(_) => {
                let _ = child.kill().await;
                (None, true)
            }
        };
    let stdout = out.await.unwrap_or_default();
    let stderr = err.await.unwrap_or_default();
    Ok(json!({
        "code": code,
        "stdout": B64.encode(stdout),
        "stderr": B64.encode(stderr),
        "timed_out": timed_out,
    }))
}
