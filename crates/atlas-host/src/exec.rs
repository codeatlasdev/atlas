//! One-shot commands (`git`, `gh`, `du`…) with a timeout and capped output.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const OUT_CAP: usize = 10 * 1024 * 1024;

/// Run `cmd`'s child in its own session (`setsid`): it survives our terminal/app and
/// has no controlling tty.
pub(crate) fn new_session(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
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
    let mut child = tokio::process::Command::new(bin)
        .args(args)
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
