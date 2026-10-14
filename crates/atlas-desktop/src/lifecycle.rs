//! What closing the window and leaving the app mean, decided in one place.
//!
//! Atlas keeps stacks running in their runners, so closing the main window only hides it and the
//! tray (menu bar on macOS) is how you get back. Three rules follow, kept here as pure functions
//! so they run in the macOS test suite although the interesting half is Linux:
//!
//! * [`close`]: hide only while something can bring the window back. On Linux the tray is a
//!   StatusNotifier item that a desktop may simply not host (stock GNOME without the AppIndicator
//!   extension); hiding the only window there strands the app with no way to reach or quit it, so
//!   closing quits.
//! * [`exit`]: an exit nobody asked for (`code: None`, Tauri's "last window destroyed") is the
//!   only one the app ever blocks, and only while the tray can reopen it. An explicit
//!   `AppHandle::exit` (tray Quit, ⌘Q, the updater's restart) is never blocked.
//! * [`tray_menu`]: which items the tray menu has. Linux delivers no tray click events (only the
//!   menu works: tray-icon docs, "Platform-specific"), so the menu is the entire tray UI there.

/// What a close request on the main window does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    /// Keep the window object, hide it; the tray brings it back.
    Hide,
    /// Let it close, which ends the app (it is the last window).
    Quit,
}

/// Closing the main window. `tray_reachable`: a tray icon exists and the desktop hosts it.
pub fn close(tray_reachable: bool) -> Close {
    if tray_reachable {
        Close::Hide
    } else {
        Close::Quit
    }
}

/// What a `RunEvent::ExitRequested` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Allow,
    /// Stay alive with no window (hidden to the tray).
    Prevent,
}

/// `code`: `Some` for an explicit `AppHandle::exit(code)`, `None` when Tauri asks because the last
/// window was destroyed.
pub fn exit(code: Option<i32>, tray_reachable: bool) -> Exit {
    match code {
        Some(_) => Exit::Allow,
        None if tray_reachable => Exit::Prevent,
        None => Exit::Allow,
    }
}

/// One entry of the tray's native menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayItem {
    /// Show the main window when it is hidden, hide it when it is showing.
    ToggleWindow,
    /// Always bring the main window forward.
    Open,
    Separator,
    Quit,
}

/// The tray menu for a platform. macOS opens the popover on left click and keeps a small
/// right-click menu; Linux gets Show/Hide and Quit because the menu is all it has.
pub fn tray_menu(linux: bool) -> &'static [TrayItem] {
    if linux {
        &[TrayItem::ToggleWindow, TrayItem::Separator, TrayItem::Quit]
    } else {
        &[TrayItem::Open, TrayItem::Separator, TrayItem::Quit]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Incident: on Linux the tray's Quit did nothing. It was `PredefinedMenuItem::quit`, which muda's
    // GTK backend renders as a disabled item (only Separator/Copy/Cut/Paste/SelectAll/About are
    // supported there), and the window's close handler hid the only window, so the app could not be
    // exited at all. Whatever the platform, an explicit exit must go through.
    #[test]
    fn an_explicit_quit_is_never_prevented() {
        for reachable in [true, false] {
            for code in [0, 1, 3, -1] {
                assert_eq!(exit(Some(code), reachable), Exit::Allow, "code {code}");
            }
        }
    }

    #[test]
    fn the_last_window_going_away_keeps_the_app_only_while_the_tray_can_reopen_it() {
        assert_eq!(exit(None, true), Exit::Prevent);
        assert_eq!(exit(None, false), Exit::Allow);
    }

    #[test]
    fn closing_the_window_hides_it_only_when_the_tray_can_bring_it_back() {
        assert_eq!(close(true), Close::Hide);
        // GNOME without the AppIndicator extension: hiding would leave a running app nobody can see.
        assert_eq!(close(false), Close::Quit);
    }

    // The same fact, stated across both decisions: never hide a window and also block the exit
    // unless the tray is there; that pair is exactly "running, invisible, unquittable".
    #[test]
    fn no_tray_never_means_hidden_and_unquittable() {
        let hidden_and_blocking = close(false) == Close::Hide && exit(None, false) == Exit::Prevent;
        assert!(!hidden_and_blocking);
    }

    #[test]
    fn every_tray_menu_ends_in_quit_and_has_no_adjacent_separators() {
        for linux in [true, false] {
            let m = tray_menu(linux);
            assert_eq!(m.last(), Some(&TrayItem::Quit), "linux={linux}");
            assert!(
                m.windows(2)
                    .all(|w| !(w[0] == TrayItem::Separator && w[1] == TrayItem::Separator)),
                "linux={linux}"
            );
            assert_ne!(m.first(), Some(&TrayItem::Separator), "linux={linux}");
        }
    }

    // Linux has no tray click, so Show/Hide must be in the menu or a hidden window cannot be
    // reopened or put away from the tray.
    #[test]
    fn the_linux_menu_can_toggle_the_window_and_macos_keeps_open() {
        assert!(tray_menu(true).contains(&TrayItem::ToggleWindow));
        assert!(!tray_menu(true).contains(&TrayItem::Open));
        assert_eq!(tray_menu(false)[0], TrayItem::Open);
        assert!(!tray_menu(false).contains(&TrayItem::ToggleWindow));
    }
}
