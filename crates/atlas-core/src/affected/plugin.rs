//! The project's plugin command: how a project that already has a test graph or a route-impact
//! script (ui's `test-graph.ts`, `route-impact.ts`) plugs it into `atlas_affected`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::Impact;

/// Run `cmd` (a shell line, cwd = `root`) with the changed files on stdin, one per line, and
/// parse its stdout as an [`Impact`] (every key optional). A non-zero exit, output that is not
/// that JSON, or a spawn failure is an error carrying the reason.
pub fn from_command(cmd: &str, root: &Path, files: &[String]) -> Result<Impact, String> {
    let mut shell = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/d", "/s", "/c", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    let mut child = shell
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run the affected command `{cmd}`: {e}"))?;
    // Feed stdin from a thread: a plugin that prints before it reads must not deadlock on a full pipe.
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let input: String = files.iter().map(|f| format!("{f}\n")).collect();
    let feeder = std::thread::spawn(move || {
        // A plugin that ignores stdin closes it early: a broken pipe is not an error.
        let _ = stdin.write_all(input.as_bytes());
    });
    let out = child
        .wait_with_output()
        .map_err(|e| format!("the affected command `{cmd}` failed: {e}"))?;
    let _ = feeder.join();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "the affected command `{cmd}` exited with {}: {}",
            out.status
                .code()
                .map_or_else(|| "a signal".to_string(), |c| format!("exit {c}")),
            err.trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| {
        format!(
            "the affected command `{cmd}` must print JSON {{tasks, tests, routes, builds}}: {e}"
        )
    })
}
