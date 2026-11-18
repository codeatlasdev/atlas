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

/// The app matches git's own words (`gitErrors.ts` turns them into sentences of the catalog), so `git` is run with
/// its messages in English, and only its messages: `LC_ALL` also decides `LC_CTYPE` (character handling, non-ASCII
/// paths), so its value moves to `LC_CTYPE`, `LC_ALL` and `LANGUAGE` (which outranks every `LC_*` for messages) go,
/// and `LC_MESSAGES` is `C`. `inherited_lc_all` is this process's own `LC_ALL`, which a child gets when `env` does
/// not say otherwise.
fn english_messages(env: &mut BTreeMap<String, String>, inherited_lc_all: Option<String>) {
    let lc_all = env.remove("LC_ALL").or(inherited_lc_all).filter(|v| !v.is_empty());
    if let Some(v) = lc_all {
        env.insert("LC_CTYPE".into(), v);
    }
    env.remove("LANGUAGE");
    env.insert("LC_MESSAGES".into(), "C".into());
}

fn is_git(bin: &str) -> bool {
    std::path::Path::new(bin)
        .file_stem()
        .is_some_and(|s| s.eq_ignore_ascii_case("git"))
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
    // Cold cache = the probe runs here; never on a runtime worker.
    let login = tokio::task::spawn_blocking(atlas_core::login_env::vars)
        .await
        .map_err(|e| e.to_string())?;
    let mut envs: BTreeMap<String, String> = login.into_iter().collect();
    envs.extend(env.iter().map(|(k, v)| (k.clone(), v.clone())));
    if is_git(bin) {
        english_messages(&mut envs, std::env::var("LC_ALL").ok());
        cmd.env_remove("LC_ALL").env_remove("LANGUAGE");
    }
    let mut child = cmd
        .no_window()
        .current_dir(cwd)
        .envs(envs)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn git_messages_are_english_and_character_handling_is_kept() {
        let mut env = map(&[("LC_ALL", "pt_BR.UTF-8"), ("LANGUAGE", "pt_BR"), ("LANG", "pt_BR.UTF-8"), ("PATH", "/bin")]);
        english_messages(&mut env, None);
        assert_eq!(env.get("LC_CTYPE").map(String::as_str), Some("pt_BR.UTF-8"), "LC_ALL moved to LC_CTYPE");
        assert_eq!(env.get("LC_MESSAGES").map(String::as_str), Some("C"));
        assert!(!env.contains_key("LC_ALL") && !env.contains_key("LANGUAGE"));
        assert_eq!(env.get("LANG").map(String::as_str), Some("pt_BR.UTF-8"), "the rest is untouched");
        // The process's own LC_ALL counts when the request does not set one; an empty one is no setting.
        let mut env = map(&[]);
        english_messages(&mut env, Some("C.UTF-8".into()));
        assert_eq!(env.get("LC_CTYPE").map(String::as_str), Some("C.UTF-8"));
        let mut env = map(&[]);
        english_messages(&mut env, Some(String::new()));
        assert!(!env.contains_key("LC_CTYPE"));
    }

    #[test]
    fn only_git_is_touched() {
        assert!(is_git("git") && is_git("/usr/bin/git") && is_git("git.exe"));
        assert!(!is_git("gh") && !is_git("gitk") && !is_git("omp"));
    }

    async fn git_in(dir: &str, parent: &BTreeMap<String, String>, args: &[&str]) -> (Option<i64>, String, String) {
        let mut argv = vec!["git".to_string(), "-C".into(), dir.into()];
        argv.extend(args.iter().map(|s| s.to_string()));
        let v = run(dir, &argv, parent, None, 20_000).await.unwrap();
        let text = |k: &str| String::from_utf8(B64.decode(v[k].as_str().unwrap()).unwrap()).unwrap();
        (v["code"].as_i64(), text("stdout"), text("stderr"))
    }

    /// A real `git` under a pt_BR.UTF-8 parent: its error is the English one `gitErrors.ts` classifies, and a
    /// non-ASCII file name survives `status` and `diff`.
    #[cfg(unix)]
    #[tokio::test]
    async fn git_runs_with_english_messages_and_still_reads_non_ascii_paths() {
        let parent = map(&[("LC_ALL", "pt_BR.UTF-8"), ("LANGUAGE", "pt_BR"), ("GIT_CONFIG_GLOBAL", "/dev/null")]);
        let outside = tempfile::tempdir().unwrap();
        let (code, _, err) = git_in(outside.path().to_str().unwrap(), &parent, &["status"]).await;
        assert_ne!(code, Some(0));
        assert!(err.contains("not a git repository"), "{err}");

        let d = tempfile::tempdir().unwrap();
        let dir = d.path().to_str().unwrap();
        git_in(dir, &parent, &["init", "-q", "."]).await;
        std::fs::write(d.path().join("ação.txt"), "um\n").unwrap();
        let (_, status, _) = git_in(dir, &parent, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).await;
        assert!(status.contains("ação.txt"), "{status:?}");
        git_in(dir, &parent, &["add", "ação.txt"]).await;
        let (_, diff, _) = git_in(dir, &parent, &["-c", "core.quotePath=false", "diff", "--cached", "--no-color"]).await;
        assert!(diff.contains("ação.txt"), "{diff:?}");
    }
}
