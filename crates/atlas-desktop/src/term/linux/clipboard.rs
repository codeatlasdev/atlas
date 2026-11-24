//! The system clipboard for the native terminal, through GTK (the same one the webview and every other app use).
//!
//! GTK clipboard calls are main-thread only. `term_paste_clipboard` and `term_copy` are sync commands (they run on the
//! main thread); OSC 52 writes arrive from a reader task, so [`write_clipboard`] hops to the main thread when it is not
//! already on it. The paste decision is `atlas_term::input::Clipboard::classify`, shared with macOS: copied files first,
//! then text, then an image with no text.

/// File paths of a `text/uri-list` (a file manager's copy): `file://` entries only, percent-decoded, comments skipped.
pub fn parse_uri_list(list: &str) -> Vec<String> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("file://"))
        // `file://host/path` names a host; only the local form (empty host) is a path here.
        .filter_map(|rest| rest.strip_prefix('/').map(|p| format!("/{p}")))
        .map(|p| percent_decode(&p))
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (
                b.get(i + 1).and_then(|c| hex(*c)),
                b.get(i + 2).and_then(|c| hex(*c)),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn clipboard() -> gtk::Clipboard {
    gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD)
}

/// The clipboard as a paste decision. Main thread.
pub fn read_clipboard() -> atlas_term::input::Clipboard {
    let cb = clipboard();
    let files = cb
        .wait_for_contents(&gtk::gdk::Atom::intern("text/uri-list"))
        .map(|d| parse_uri_list(&String::from_utf8_lossy(&d.data())))
        .unwrap_or_default();
    let text = cb.wait_for_text().map(|t| t.to_string());
    atlas_term::input::Clipboard::classify(files, text, cb.wait_is_image_available())
}

/// Text to the clipboard, on any thread.
pub fn write_clipboard(text: &str) {
    on_main(move |text| clipboard().set_text(text), text);
}

/// The PRIMARY selection (what was last selected in any app: a middle click pastes it). Main thread.
pub fn read_primary() -> Option<String> {
    primary().wait_for_text().map(|t| t.to_string())
}

/// Make `text` the PRIMARY selection, on any thread.
pub fn write_primary(text: &str) {
    on_main(move |text| primary().set_text(text), text);
}

fn primary() -> gtk::Clipboard {
    gtk::Clipboard::get(&gtk::gdk::SELECTION_PRIMARY)
}

/// Run `f(text)` on the main thread (GTK clipboards are main-thread only), now when already there.
fn on_main(f: impl FnOnce(&str) + Send + 'static, text: &str) {
    if gtk::is_initialized_main_thread() {
        f(text);
        return;
    }
    let text = text.to_string();
    if let Some(app) = super::super::APP.get() {
        let _ = app.run_on_main_thread(move || f(&text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A file manager's copy: what GTK hands over for "Copy" on two files, one with a space and one with non-ASCII.
    #[test]
    fn a_file_managers_uri_list_becomes_decoded_local_paths() {
        let list = "# comment\r\nfile:///home/me/a%20b.txt\r\nfile:///home/me/caf%C3%A9.md\r\n";
        assert_eq!(
            parse_uri_list(list),
            ["/home/me/a b.txt", "/home/me/café.md"]
        );
    }

    #[test]
    fn remote_hosts_and_other_schemes_are_not_paths() {
        let list = "file://other-host/etc/passwd\nhttps://example.com/x\nfile:///ok\n";
        assert_eq!(parse_uri_list(list), ["/ok"]);
    }

    #[test]
    fn a_stray_percent_is_kept_not_dropped() {
        assert_eq!(
            parse_uri_list("file:///100%/x\nfile:///a%2"),
            ["/100%/x", "/a%2"]
        );
    }
}
