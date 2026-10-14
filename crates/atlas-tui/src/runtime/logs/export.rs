//! Turning log entries into text for the clipboard and for AI prompts.

use super::{LogEntry, LogLevel};
use crate::runtime::clock::hms;
use crate::runtime::text::strip_ansi;

pub fn format_for_clipboard(entries: &[&LogEntry], context: &ClipboardContext) -> String {
    let mut output = String::new();

    output.push_str(&format!("# {} — Dev Logs\n", context.project_name));
    output.push_str(&format!(
        "# Generated: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    if !context.services.is_empty() {
        output.push_str(&format!("# Services: {}\n", context.services.join(", ")));
    }
    if let Some(ref filter) = context.filter {
        output.push_str(&format!("# Filter: {}\n", filter));
    }
    output.push('\n');

    let mut current_service = "";
    for entry in entries {
        if entry.service != current_service {
            if !current_service.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("── {} ──\n", entry.service));
            current_service = &entry.service;
        }

        let ts = format!(
            "{}.{:03}",
            hms(entry.timestamp as i64),
            entry.timestamp % 1000
        );
        let level_marker = match entry.level {
            LogLevel::Error => "[ERR]",
            LogLevel::Warn => "[WRN]",
            LogLevel::Debug => "[DBG]",
            LogLevel::Info => "",
        };

        if level_marker.is_empty() {
            output.push_str(&format!("{} {}\n", ts, strip_ansi(&entry.content)));
        } else {
            output.push_str(&format!(
                "{} {} {}\n",
                ts,
                level_marker,
                strip_ansi(&entry.content)
            ));
        }
    }

    output
}

pub fn format_for_ai_prompt(entries: &[&LogEntry], context: &ClipboardContext) -> String {
    let mut output = String::new();

    output.push_str("I'm working on a development environment and encountering issues. ");
    output.push_str("Here are the recent logs from my services:\n\n");

    output.push_str(&format!("Project: {}\n", context.project_name));
    output.push_str(&format!("Services: {}\n", context.services.join(", ")));
    output.push_str("\n```\n");

    let error_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.level == LogLevel::Error || e.level == LogLevel::Warn)
        .map(|(i, _)| i)
        .collect();

    if error_indices.is_empty() {
        let start = entries.len().saturating_sub(30);
        for entry in &entries[start..] {
            output.push_str(&format!(
                "[{}] {}\n",
                entry.service,
                strip_ansi(&entry.content)
            ));
        }
    } else {
        let mut shown: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for &idx in &error_indices {
            let start = idx.saturating_sub(3);
            let end = (idx + 3).min(entries.len());
            #[allow(clippy::needless_range_loop)]
            for i in start..end {
                if shown.insert(i) {
                    let marker = if i == idx { ">>>" } else { "   " };
                    output.push_str(&format!(
                        "{} [{}] {}\n",
                        marker,
                        entries[i].service,
                        strip_ansi(&entries[i].content)
                    ));
                }
            }
            if end < entries.len() {
                output.push_str("   ...\n");
            }
        }
    }

    output.push_str("```\n\n");
    output.push_str("Can you help me understand what's going wrong and how to fix it?\n");

    output
}

#[derive(Debug, Clone)]
pub struct ClipboardContext {
    pub project_name: String,
    pub services: Vec<String>,
    pub filter: Option<String>,
}

pub async fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let mut child = if cfg!(target_os = "macos") {
        Command::new("pbcopy").stdin(Stdio::piped()).spawn()?
    } else if cfg!(windows) {
        Command::new("clip.exe").stdin(Stdio::piped()).spawn()?
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()?
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }

    let status = child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("clipboard command failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::logs::{LogBuffer, LogStream};

    #[test]
    fn test_format_for_clipboard() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "Started on :3000".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: ECONNREFUSED".to_string(),
            LogStream::Stderr,
        );
        buf.push("web".to_string(), "Compiled".to_string(), LogStream::Stdout);

        let entries: Vec<&LogEntry> = buf.all();
        let ctx = ClipboardContext {
            project_name: "myapp".to_string(),
            services: vec!["api".to_string(), "web".to_string()],
            filter: None,
        };

        let output = format_for_clipboard(&entries, &ctx);
        assert!(output.contains("# myapp"));
        assert!(output.contains("── api ──"));
        assert!(output.contains("[ERR]"));
        assert!(output.contains("ECONNREFUSED"));
    }

    #[test]
    fn test_format_for_ai_prompt() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "Listening on :3000".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "GET /health 200".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: Cannot read property 'id' of undefined".to_string(),
            LogStream::Stderr,
        );
        buf.push(
            "api".to_string(),
            "    at UserController.get (/app/src/user.ts:42)".to_string(),
            LogStream::Stderr,
        );

        let entries: Vec<&LogEntry> = buf.all();
        let ctx = ClipboardContext {
            project_name: "myapp".to_string(),
            services: vec!["api".to_string()],
            filter: None,
        };

        let output = format_for_ai_prompt(&entries, &ctx);
        assert!(output.contains("I'm working on a development environment"));
        assert!(output.contains(">>>"));
        assert!(output.contains("Cannot read property"));
        assert!(output.contains("Can you help me"));
    }

    #[test]
    fn test_format_for_ai_prompt_no_errors() {
        let mut buf = LogBuffer::new(100);
        for i in 0..50 {
            buf.push(
                "api".to_string(),
                format!("Normal log line {i}"),
                LogStream::Stdout,
            );
        }

        let entries: Vec<&LogEntry> = buf.all();
        let ctx = ClipboardContext {
            project_name: "test".to_string(),
            services: vec!["api".to_string()],
            filter: None,
        };

        let output = format_for_ai_prompt(&entries, &ctx);
        assert!(output.contains("Normal log line 49"));
        assert!(output.contains("Normal log line 20"));
    }
}
