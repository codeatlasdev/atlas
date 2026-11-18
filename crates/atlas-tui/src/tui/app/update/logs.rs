//! Log pane messages: ingestion, copy, search, time range, view toggles, links.

use super::super::{App, Message, ToastVariant};
use super::clipboard::{COPY_CONTEXT_SECS, COPY_RECENT_SECS, CopyKind};
use crate::runtime::logs;
use atlas_i18n::t;

impl App {
    /// Handle the logs messages; anything else is handed back for the next domain.
    pub(super) fn reduce_logs(&mut self, msg: Message) -> Option<Message> {
        match msg {
            Message::LogLineFrom { name, line, stream } => {
                self.logs.log_buffer.push(name, line, stream);
                // Paused: grow the offset by the new rows so the view stays put.
                if !self.logs.follow_logs {
                    self.bump_paused_scroll_for_last();
                }
            }
            Message::CopyLogs => self.copy_logs(
                |b| b.since(COPY_RECENT_SECS),
                logs::format_for_clipboard,
                CopyKind::plain(|_| t!("tui_runtime_app_toast_logs_copied")),
            ),
            Message::CopyLogsForAI => self.copy_logs(
                |b| b.since(COPY_CONTEXT_SECS),
                logs::format_for_ai_prompt,
                CopyKind::plain(|_| t!("tui_runtime_app_toast_ai_copied")),
            ),
            Message::CopyErrors => self.copy_logs(
                |b| {
                    // One `── service ──` header per service, not per alternation.
                    let mut errors = b.errors();
                    errors.sort_by(|a, b| a.service.cmp(&b.service));
                    errors
                },
                logs::format_for_clipboard,
                CopyKind {
                    success: |n| t!("tui_runtime_app_toast_errors_copied", count = n),
                    empty_toast: Some(|| t!("tui_runtime_app_toast_no_errors")),
                    fallback_label: Some("errors only"),
                },
            ),
            Message::ClipboardResult { success, ok } => {
                let variant = if ok {
                    ToastVariant::Success
                } else {
                    ToastVariant::Error
                };
                self.set_toast(success, variant);
            }
            Message::ClearLogs => {
                self.logs.log_buffer.clear();
                self.reset_log_view();
                self.set_toast(t!("tui_runtime_app_toast_logs_cleared"), ToastVariant::Info);
            }
            Message::StartLogSearch => {
                self.logs.log_search_input.clear();
                self.logs.log_searching = true;
                self.find_requery();
            }
            Message::LogSearchInput(c) => {
                self.logs.log_search_input.push(c);
                self.find_requery();
            }
            Message::LogSearchBackspace => {
                self.logs.log_search_input.pop();
                self.find_requery();
            }
            Message::LogSearchCommit => {
                self.logs.log_searching = false;
            }
            Message::LogSearchClear => {
                self.logs.log_search_input.clear();
                self.reset_log_view();
                self.logs.log_searching = false;
            }
            Message::CycleTimeRange => self.cycle_time_range(),
            Message::TimeInput(c) => self.time_input(c),
            Message::TimeBackspace => self.time_backspace(),
            Message::TimeCommit => self.time_commit(),
            Message::TimeCancel => self.time_cancel(),
            Message::ToggleSearchMode => self.toggle_search_mode(),
            Message::ToggleSearchRegex => self.toggle_search_regex(),
            Message::FindNext => self.find_step(true),
            Message::FindPrev => self.find_step(false),
            Message::ToggleWrap => self.toggle_wrap(),
            Message::ToggleTimestamps => self.toggle_timestamps(),
            Message::ToggleJson => self.toggle_json(),
            Message::PanLogs { right } => self.pan_logs(right),
            Message::InsertMark => self.insert_mark(),
            Message::OpenTabErrors(idx) => self.open_tab_errors(idx),
            Message::JumpProblem { forward, warn } => {
                self.logs.chord = None;
                self.jump_problem(forward, warn);
            }
            Message::StepLink { forward } => self.step_link(forward),
            Message::OpenLink => self.open_link(),
            Message::CopyLink => self.copy_link(),
            Message::ChordStart(c) => self.logs.chord = (c != '\0').then_some(c),
            Message::CycleLevelFilter => {
                self.logs.level_filter = self.logs.level_filter.next();
                self.reset_log_view();
                self.save_ui_state();
            }
            other => return Some(other),
        }
        None
    }
}
