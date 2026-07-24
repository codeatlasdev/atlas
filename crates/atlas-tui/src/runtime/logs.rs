use std::collections::VecDeque;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,
    pub service: String,
    pub stream: LogStream,
    pub content: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn detect(content: &str) -> Self {
        let lower = content.to_lowercase();
        if lower.contains("error")
            || lower.contains("err ")
            || lower.contains("\x1b[31m")
            || lower.contains("fatal")
            || lower.contains("panic")
        {
            Self::Error
        } else if lower.contains("warn") || lower.contains("\x1b[33m") {
            Self::Warn
        } else if lower.contains("debug") || lower.contains("\x1b[90m") || lower.contains("trace")
        {
            Self::Debug
        } else {
            Self::Info
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, service: String, content: String, stream: LogStream) {
        let level = LogLevel::detect(&content);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let entry = LogEntry {
            timestamp,
            service,
            stream,
            content,
            level,
        };

        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn all(&self) -> &VecDeque<LogEntry> {
        &self.entries
    }

    pub fn filter_service(&self, service: &str) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.service == service)
            .collect()
    }

    pub fn filter_level(&self, min_level: LogLevel) -> Vec<&LogEntry> {
        let min = level_to_u8(min_level);
        self.entries
            .iter()
            .filter(|e| level_to_u8(e.level) >= min)
            .collect()
    }

    pub fn errors(&self) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.level == LogLevel::Error)
            .collect()
    }

    pub fn since(&self, seconds: u64) -> Vec<&LogEntry> {
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
            - (seconds * 1000);
        self.entries
            .iter()
            .filter(|e| e.timestamp >= cutoff)
            .collect()
    }

    pub fn tail(&self, n: usize) -> Vec<&LogEntry> {
        let start = self.entries.len().saturating_sub(n);
        self.entries.iter().skip(start).collect()
    }

    pub fn tail_service(&self, service: &str, n: usize) -> Vec<&LogEntry> {
        let filtered: Vec<&LogEntry> = self
            .entries
            .iter()
            .filter(|e| e.service == service)
            .collect();
        let start = filtered.len().saturating_sub(n);
        filtered[start..].to_vec()
    }

    pub fn search(&self, pattern: &str) -> Vec<&LogEntry> {
        let lower_pattern = pattern.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&lower_pattern))
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

fn level_to_u8(level: LogLevel) -> u8 {
    match level {
        LogLevel::Debug => 0,
        LogLevel::Info => 1,
        LogLevel::Warn => 2,
        LogLevel::Error => 3,
    }
}

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

        let ts = format_timestamp(entry.timestamp);
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

pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;

    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c == 'm' || c == 'K' || c == 'H' || c == 'J' {
                in_escape = false;
            }
            continue;
        }
        result.push(c);
    }
    result
}

pub fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = if cfg!(target_os = "macos") {
        Command::new("pbcopy").stdin(Stdio::piped()).spawn()?
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()?
    };

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

fn format_timestamp(millis: u64) -> String {
    let secs = millis / 1000;
    let ms = millis % 1000;
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_detect_error() {
        assert_eq!(
            LogLevel::detect("Error: connection refused"),
            LogLevel::Error
        );
        assert_eq!(LogLevel::detect("FATAL: out of memory"), LogLevel::Error);
        assert_eq!(
            LogLevel::detect("panic: index out of bounds"),
            LogLevel::Error
        );
    }

    #[test]
    fn test_log_level_detect_warn() {
        assert_eq!(LogLevel::detect("Warning: deprecated API"), LogLevel::Warn);
        assert_eq!(LogLevel::detect("WARN slow query detected"), LogLevel::Warn);
    }

    #[test]
    fn test_log_level_detect_info() {
        assert_eq!(
            LogLevel::detect("Server listening on port 3000"),
            LogLevel::Info
        );
        assert_eq!(LogLevel::detect("GET /api/users 200"), LogLevel::Info);
    }

    #[test]
    fn test_log_level_detect_debug() {
        assert_eq!(
            LogLevel::detect("DEBUG: cache hit for key xyz"),
            LogLevel::Debug
        );
        assert_eq!(LogLevel::detect("trace entering function"), LogLevel::Debug);
    }

    #[test]
    fn test_log_buffer_push_and_tail() {
        let mut buf = LogBuffer::new(100);
        buf.push("api".to_string(), "line 1".to_string(), LogStream::Stdout);
        buf.push("api".to_string(), "line 2".to_string(), LogStream::Stdout);
        buf.push("web".to_string(), "line 3".to_string(), LogStream::Stdout);

        assert_eq!(buf.len(), 3);
        assert_eq!(buf.tail(2).len(), 2);
        assert_eq!(buf.tail(2)[0].content, "line 2");
    }

    #[test]
    fn test_log_buffer_capacity() {
        let mut buf = LogBuffer::new(3);
        for i in 0..5 {
            buf.push("svc".to_string(), format!("line {i}"), LogStream::Stdout);
        }
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.all().front().unwrap().content, "line 2");
        assert_eq!(buf.all().back().unwrap().content, "line 4");
    }

    #[test]
    fn test_log_buffer_filter_service() {
        let mut buf = LogBuffer::new(100);
        buf.push("api".to_string(), "api line".to_string(), LogStream::Stdout);
        buf.push("web".to_string(), "web line".to_string(), LogStream::Stdout);
        buf.push(
            "api".to_string(),
            "api line 2".to_string(),
            LogStream::Stdout,
        );

        let filtered = buf.filter_service("api");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_log_buffer_errors() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "normal log".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: something broke".to_string(),
            LogStream::Stderr,
        );
        buf.push(
            "api".to_string(),
            "another normal".to_string(),
            LogStream::Stdout,
        );

        let errors = buf.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].content, "Error: something broke");
    }

    #[test]
    fn test_log_buffer_search() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "Connected to database".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: database timeout".to_string(),
            LogStream::Stderr,
        );
        buf.push(
            "web".to_string(),
            "Compiled successfully".to_string(),
            LogStream::Stdout,
        );

        let results = buf.search("database");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[31mError\x1b[0m"), "Error");
        assert_eq!(strip_ansi("no escapes here"), "no escapes here");
        assert_eq!(strip_ansi("\x1b[1;34mblue\x1b[0m text"), "blue text");
    }

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
        buf.push(
            "web".to_string(),
            "Compiled".to_string(),
            LogStream::Stdout,
        );

        let entries: Vec<&LogEntry> = buf.all().iter().collect();
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

        let entries: Vec<&LogEntry> = buf.all().iter().collect();
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

        let entries: Vec<&LogEntry> = buf.all().iter().collect();
        let ctx = ClipboardContext {
            project_name: "test".to_string(),
            services: vec!["api".to_string()],
            filter: None,
        };

        let output = format_for_ai_prompt(&entries, &ctx);
        assert!(output.contains("Normal log line 49"));
        assert!(output.contains("Normal log line 20"));
    }

    #[test]
    fn test_log_buffer_tail_service() {
        let mut buf = LogBuffer::new(100);
        for i in 0..10 {
            buf.push("api".to_string(), format!("api {i}"), LogStream::Stdout);
            buf.push("web".to_string(), format!("web {i}"), LogStream::Stdout);
        }

        let tail = buf.tail_service("api", 3);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail[0].content, "api 7");
        assert_eq!(tail[2].content, "api 9");
    }
}
