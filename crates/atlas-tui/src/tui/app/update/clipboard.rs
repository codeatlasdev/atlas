//! Copying log text to the clipboard (recent logs, AI prompt, errors only).

use super::super::messages::Effect;
use super::super::{App, ToastVariant};
use crate::runtime::logs::{self, ClipboardContext, LogEntry};

/// "Copy logs": how far back the clipboard copy reaches.
pub(super) const COPY_RECENT_SECS: u64 = 180;
/// "Copy for AI": wider window so the prompt carries context around the failure.
pub(super) const COPY_CONTEXT_SECS: u64 = 300;

/// How one clipboard copy words its outcome.
pub(super) struct CopyKind {
    /// Toast text from the number of copied entries.
    pub(super) success: fn(usize) -> String,
    /// Toast shown instead of copying when nothing matched (None = copy anyway).
    pub(super) empty_toast: Option<fn() -> String>,
    /// Header filter label used when no tab narrows the copy.
    pub(super) fallback_label: Option<&'static str>,
}

impl CopyKind {
    pub(in crate::tui::app) fn plain(success: fn(usize) -> String) -> Self {
        Self {
            success,
            empty_toast: None,
            fallback_label: None,
        }
    }
}

impl App {
    /// Queue a clipboard copy of the entries `select` picks from the buffer, narrowed to
    /// the active tab, formatted by `format`; `copy` words the outcome (see `CopyKind`).
    pub(super) fn copy_logs(
        &mut self,
        select: impl for<'a> FnOnce(&'a logs::LogBuffer) -> Vec<&'a LogEntry>,
        format: fn(&[&LogEntry], &ClipboardContext) -> String,
        copy: CopyKind,
    ) {
        let filter = self.tab_filter();
        let entries: Vec<&LogEntry> = select(&self.logs.log_buffer)
            .into_iter()
            .filter(|e| filter.as_deref().is_none_or(|s| e.service == s))
            .collect();
        if entries.is_empty()
            && let Some(msg) = copy.empty_toast
        {
            self.set_toast(msg(), ToastVariant::Info);
            return;
        }
        let filter = match copy.fallback_label {
            Some(label) => filter.or_else(|| Some(label.to_string())),
            None => filter,
        };
        let ctx = self.clipboard_ctx(filter);
        let text = format(&entries, &ctx);
        let success = (copy.success)(entries.len());
        self.effects
            .push_back(Effect::CopyToClipboard { text, success });
    }

    /// Current tab filter: Some(service_name) when on a service tab, None on All.
    fn tab_filter(&self) -> Option<String> {
        if self.active_tab > 0 {
            self.tabs.get(self.active_tab).cloned()
        } else {
            None
        }
    }

    /// Queue a clipboard copy of `text`, toasting `success` on completion.
    pub(in crate::tui::app) fn copy_text(&mut self, text: String, success: String) {
        self.effects
            .push_back(Effect::CopyToClipboard { text, success });
    }

    /// Build a ClipboardContext scoped to the active tab.
    fn clipboard_ctx(&self, filter: Option<String>) -> ClipboardContext {
        ClipboardContext {
            project_name: self.config.name.clone(),
            services: self
                .manager
                .service_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            filter,
        }
    }
}
