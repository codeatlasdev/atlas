//! Short replies: a long reply is rewritten by a cheap model (`omp`/`claude` one-shot) into a few
//! lines, when the crew and the crewmate allow it. The full text stays on the task as `reply_full`.

use std::collections::BTreeMap;

use super::action::visible_reply;
#[cfg(test)]
use super::testing::TEST_SUMMARIZER_BIN;
use crate::crew::Crewmate;
use crate::server::Hub;

fn default_cheap_model(engine_kind: &str) -> &'static str {
    match engine_kind {
        "claude" => "claude-haiku-4-5",
        _ => "cc/claude-haiku-5-5",
    }
}

async fn summarize(
    text: &str,
    engine_kind: &str,
    model: &str,
    agent_env: &BTreeMap<String, String>,
    crewmate_dir: &std::path::Path,
) -> Option<String> {
    use std::process::Stdio;
    use tokio::time::timeout;
    const TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(25);
    let system = "Same language as the reply. \
        Keep every file path, command, number, error and every question or decision asked of the user. \
        Say what was done, what changed, what is left, what the user must do. \
        No new facts, no preamble, no emojis. At most 6 short lines or bullets.";
    let prompt = format!("{system}\n\n---\n{text}");

    // Copied out first: the lock guard must not live across the await below.
    #[cfg(test)]
    {
        let test_bin = TEST_SUMMARIZER_BIN.lock().clone();
        if let Some(bin) = test_bin {
            let out = timeout(
                TIMEOUT,
                tokio::process::Command::new(bin)
                    .arg(&prompt)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output(),
            )
            .await
            .ok()?
            .ok()?;
            return shorter(&out, text);
        }
    }

    let mut cmd = match engine_kind {
        "omp" => {
            let mut c = tokio::process::Command::new("omp");
            // A one-shot rewrite: no tools, LSP, extensions, skills or rules to load.
            c.args(["-p", &prompt, "--model", model])
                .args([
                    "--no-tools",
                    "--no-lsp",
                    "--no-extensions",
                    "--no-skills",
                    "--no-rules",
                ])
                .args(crate::crew::engine::omp::overlay_args(crewmate_dir));
            c
        }
        "claude" => {
            let mut c = tokio::process::Command::new("claude");
            c.args(["-p", &prompt, "--model", model, "--output-format", "text"]);
            c
        }
        _ => return None,
    };
    for (k, v) in agent_env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());
    // Every failure is logged: a silent fallback hid for days that every summary timed out (omp waited 25 s on D-Bus).
    let out = match timeout(TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            eprintln!("atlas crew: summary: could not start {engine_kind}: {e}");
            return None;
        }
        Err(_) => {
            eprintln!(
                "atlas crew: summary: {engine_kind} took over {}s, sending the full reply",
                TIMEOUT.as_secs()
            );
            return None;
        }
    };
    if !out.status.success() {
        eprintln!(
            "atlas crew: summary: {engine_kind} exited with {}",
            out.status
        );
        return None;
    }
    shorter(&out, text)
}

/// The summarizer's stdout when it succeeded and is really shorter than `text`; otherwise `None`
/// (the full reply is sent: a "summary" that is empty or longer helps nobody).
fn shorter(out: &std::process::Output, text: &str) -> Option<String> {
    if !out.status.success() {
        return None;
    }
    let summary = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!summary.is_empty() && summary.len() < text.len()).then_some(summary)
}

pub(super) async fn maybe_summarize(
    hub: &Hub,
    crewmate: &Crewmate,
    outcome: Result<String, String>,
    agent_env: &BTreeMap<String, String>,
) -> (Result<String, String>, Option<String>, bool) {
    const THRESHOLD: usize = 600;
    let text = match &outcome {
        Ok(t) => t.clone(),
        Err(_) => return (outcome, None, false),
    };
    let visible = visible_reply(&text);
    if visible.len() <= THRESHOLD {
        return (outcome, None, false);
    }
    let crew = hub.crew.store.load_crew_settings();
    if !crew.short_replies {
        return (outcome, None, false);
    }
    if crewmate.short_replies == Some(false) {
        return (outcome, None, false);
    }
    let engine_kind = crewmate
        .engine
        .as_ref()
        .map(|e| e.kind.as_str())
        .unwrap_or(&crewmate.agent);
    let model = crew
        .short_replies_model
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_cheap_model(engine_kind));
    let crewmate_dir = hub.crew.store.crewmate_dir(&crewmate.id);
    match summarize(&visible, engine_kind, model, agent_env, &crewmate_dir).await {
        Some(summary) => (Ok(summary), Some(visible), true),
        None => (outcome, None, false),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::testing::TEST_BIN_LOCK;
    use super::super::testing::set_test_summarizer_bin;
    use super::*;

    // Helper: write a shell script to a tempdir/bin/<name> and return its path.
    fn script(dir: &std::path::Path, name: &str, body: &str) -> String {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let p = bin.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    // summarize() returns None when the summarizer binary exits non-zero.
    #[tokio::test]
    async fn summarizer_failure_falls_back_to_original() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(dir.path(), "fail-sum", "exit 1");
        let env: std::collections::BTreeMap<String, String> = Default::default();
        // Hold lock to avoid racing with other tests that mutate TEST_SUMMARIZER_BIN.
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize(
            "some long text",
            "omp",
            "model",
            &env,
            std::path::Path::new("/tmp"),
        )
        .await;
        set_test_summarizer_bin(None);
        assert!(result.is_none(), "non-zero exit must return None");
    }

    // summarize() returns None when the summarizer output is longer than the input.
    #[tokio::test]
    async fn summarizer_longer_output_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        // Emit a string longer than the input "hi" (2 chars).
        let bin = script(
            dir.path(),
            "long-sum",
            "printf 'this is much longer than hi'",
        );
        let env: std::collections::BTreeMap<String, String> = Default::default();
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize("hi", "omp", "model", &env, std::path::Path::new("/tmp")).await;
        set_test_summarizer_bin(None);
        assert!(result.is_none(), "longer output must return None");
    }

    // summarize() passes rules + original text as argv[1] to the binary.
    #[tokio::test]
    async fn summarizer_prompt_contains_rules_and_original() {
        let dir = tempfile::tempdir().unwrap();
        let out_file = dir.path().join("prompt.txt");
        let out_path = out_file.to_string_lossy().into_owned();
        // Write argv[1] to a file, then emit a short summary.
        let bin = script(
            dir.path(),
            "record-sum",
            &format!("printf '%s' \"$1\" > {out_path}\nprintf 'ok'"),
        );
        let original = "A".repeat(700);
        let env: std::collections::BTreeMap<String, String> = Default::default();
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize(
            &original,
            "omp",
            "model",
            &env,
            std::path::Path::new("/tmp"),
        )
        .await;
        set_test_summarizer_bin(None);
        assert_eq!(result.as_deref(), Some("ok"));
        let prompt = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            prompt.contains("No new facts, no preamble, no emojis"),
            "prompt must contain the summarizer rules"
        );
        assert!(
            prompt.contains(&original),
            "prompt must contain the original text"
        );
    }

    // summarize() returns Some(summary) for a successful binary and shorter output.
    #[tokio::test]
    async fn summarize_succeeds_with_shorter_output() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(dir.path(), "ok-sum", "printf 'short'");
        let long_text = "word ".repeat(200); // much longer than "short"
        let env: std::collections::BTreeMap<String, String> = Default::default();
        let _guard = TEST_BIN_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;
        set_test_summarizer_bin(Some(&bin));
        let result = summarize(
            &long_text,
            "omp",
            "model",
            &env,
            std::path::Path::new("/tmp"),
        )
        .await;
        set_test_summarizer_bin(None);
        assert_eq!(result.as_deref(), Some("short"));
    }
}
