//! The link cursor in the log pane: step between links, open or copy one.
use crate::runtime::loglinks::Target;
use crate::tui::widgets::log_viewer::{self, Cursor};
use atlas_i18n::t;

use super::super::{App, Effect, ToastVariant};

impl App {
    /// Entry the link cursor starts from: the one under the viewport's
    /// bottom row (what the eye is on while following).
    fn link_anchor_entry(&self) -> Option<u64> {
        self.sync_logs();
        self.seq_at_viewport_row(|_| 1)
    }

    /// `Tab` on the dashboard log pane: next/prev link, walking entries from
    /// the newest visible one upward (forward = toward newer).
    pub(in crate::tui::app) fn step_link(&mut self, forward: bool) {
        self.sync_logs();
        let json = self.logs.log_json_pretty;
        let start = match &self.logs.link_cursor {
            Some(c) => c.clone(),
            None => {
                let Some(seq) = self.link_anchor_entry() else {
                    return;
                };
                Cursor {
                    seq,
                    link: usize::MAX,
                }
            }
        };
        let found = {
            let ix = self.logs.index.borrow();
            let Some(mut i) = ix.index_of(start.seq) else {
                return;
            };
            let at = |seq: u64| self.logs.log_buffer.get_any(seq);
            let mut link = start.link;
            let mut result = None;
            // Within the starting entry first, then outward, bounded so a
            // link-less buffer does not scan everything per keypress.
            for step in 0..4096 {
                if let Some(e) = ix.seq_at(i).and_then(at) {
                    let n = log_viewer::entry_links(e, json).len();
                    let candidate = match (forward, link) {
                        (true, usize::MAX) => (n > 0).then_some(0),
                        (true, l) => (l + 1 < n).then_some(l + 1),
                        (false, usize::MAX) => n.checked_sub(1),
                        (false, l) => l.checked_sub(1),
                    };
                    // First visit of the start entry with an existing cursor
                    // continues from it; later entries start at their edge.
                    if let Some(l) = candidate
                        && !(step == 0 && start.link == usize::MAX && !forward && n == 0)
                    {
                        result = Some(Cursor {
                            seq: e.seq,
                            link: l,
                        });
                        break;
                    }
                }
                // Move to the neighbouring entry; entering it from its far edge.
                if forward {
                    i += 1;
                    if i >= ix.len() {
                        break;
                    }
                    link = usize::MAX;
                } else {
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                    link = usize::MAX;
                }
            }
            result
        };
        match found {
            Some(c) => {
                let seq = c.seq;
                self.logs.link_cursor = Some(c);
                self.scroll_to_seq(seq);
            }
            None => self.set_toast(t!("tui_runtime_app_toast_no_links"), ToastVariant::Info),
        }
    }

    /// The link under the cursor, resolved against the project root.
    fn cursor_target(&self) -> Option<Target> {
        let c = self.logs.link_cursor.as_ref()?;
        let e = self.logs.log_buffer.get_any(c.seq)?;
        let l = log_viewer::entry_links(e, self.logs.log_json_pretty)
            .into_iter()
            .nth(c.link)?;
        Some(l.target)
    }

    /// `o`: open the focused link without blocking the event loop.
    pub(in crate::tui::app) fn open_link(&mut self) {
        let Some(target) = self.cursor_target() else {
            self.set_toast(
                t!(
                    "tui_runtime_app_toast_select_link",
                    key = t!("common_keys_tab")
                ),
                ToastVariant::Info,
            );
            return;
        };
        match target {
            Target::Url(u) => self.effects.push_back(Effect::OpenUrl(u)),
            Target::Path { path, line, col } => {
                match crate::runtime::loglinks::resolve(&self.manager.root_dir, &path) {
                    Some(full) => self.effects.push_back(Effect::OpenPath {
                        path: full.to_string_lossy().into_owned(),
                        line,
                        col,
                    }),
                    // Not a file we can find: the next best thing is the text.
                    None => self.copy_text(
                        format!("{path}:{line}"),
                        t!(
                            "tui_runtime_app_toast_path_missing",
                            path = path.as_str(),
                            location = format!("{path}:{line}")
                        ),
                    ),
                }
            }
        }
    }

    /// `y`: copy the focused link's text.
    pub(in crate::tui::app) fn copy_link(&mut self) {
        let Some(target) = self.cursor_target() else {
            self.set_toast(
                t!(
                    "tui_runtime_app_toast_select_link",
                    key = t!("common_keys_tab")
                ),
                ToastVariant::Info,
            );
            return;
        };
        let text = match target {
            Target::Url(u) => u,
            Target::Path { path, line, col } => match col {
                Some(c) => format!("{path}:{line}:{c}"),
                None => format!("{path}:{line}"),
            },
        };
        self.copy_text(
            text.clone(),
            t!("tui_runtime_app_toast_copied", text = text.as_str()),
        );
    }
}
