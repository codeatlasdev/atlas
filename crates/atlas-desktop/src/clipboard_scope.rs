//! Which webview may read the system clipboard (#740).
//!
//! WebKitGTK denies `navigator.clipboard.readText()` unless the view's `javascript-can-access-clipboard`
//! setting is on, so the terminal's Ctrl+Shift+V did nothing on Linux. The setting is per webview and gives the
//! page the user's clipboard, so it is on for the app's own page only: never for a tab of the embedded browser
//! or any other webview that loads a page of someone else.

use tauri::{Manager, Runtime, WebviewWindowBuilder};

/// The app's own main window: the Svelte app bundled with the binary (Linux; macOS always allows it, and
/// Windows keeps WebView2's own permission prompt).
pub fn own_page<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(target_os = "linux")]
    let builder = builder.enable_clipboard_access();
    builder
}

#[cfg(test)]
mod tests {
    /// Incident (#740): the main webview was built without clipboard access. The reverse mistake is worse
    /// (a browser tab that can read the user's clipboard), so this pins both: the main window asks for it, and
    /// no other place sets the clipboard flag or calls [`super::own_page`].
    #[test]
    fn only_the_main_window_gets_clipboard_access() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut main_calls = 0;
        let mut stray = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") || p.ends_with("clipboard_scope.rs") {
                    continue;
                }
                let code = std::fs::read_to_string(&p).unwrap();
                let is_main = p.ends_with("main.rs");
                for (i, l) in code.lines().enumerate() {
                    let flag = ["enable_clipboard_access", "can_access_clipboard"]
                        .iter()
                        .any(|f| l.contains(f));
                    let scope = l.contains("clipboard_scope::own_page(");
                    if is_main && scope {
                        main_calls += 1;
                    } else if flag || scope {
                        stray.push(format!(
                            "{}:{}",
                            p.strip_prefix(&src).unwrap().display(),
                            i + 1
                        ));
                    }
                }
            }
        }
        assert!(
            stray.is_empty(),
            "clipboard access outside the main window: {stray:?}"
        );
        assert_eq!(
            main_calls, 1,
            "the main window must be built through clipboard_scope::own_page"
        );
    }
}
