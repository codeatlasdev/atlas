//! AppKit side of the dock: the Tauri window becomes a non-activating floating panel.
//!
//! Tauri's window is a `TaoWindow` (an `NSWindow`). A non-activating panel is an `NSPanel` with
//! `NSWindowStyleMaskNonactivatingPanel`: clicking it or typing in it never activates Atlas, so the app you are
//! working in keeps the keyboard focus. Only `NSPanel` honours that style bit, so the window's class is swapped for
//! a runtime subclass of `NSPanel` (`object_setClass`, the technique tauri-nspanel uses; `NSPanel` adds no instance
//! variables, which [`make_panel`] checks before it swaps). The subclass answers `canBecomeKeyWindow` from a flag:
//! false while the dock is the idle pill (it can never take the key from another app), true while the composer is
//! open (the input must receive keys).
//!
//! What the panel is: floating level above ordinary windows and (via `fullScreenAuxiliary`) over fullscreen apps,
//! on every Space (`canJoinAllSpaces`), left out of ⌘Tab and Mission Control (`ignoresCycle`, `stationary`), never
//! hidden when the app deactivates.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::sel;
use objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};

/// Whether the panel may become the key window (the composer is open).
static KEY_ALLOWED: AtomicBool = AtomicBool::new(false);

pub fn set_key_allowed(on: bool) {
    KEY_ALLOWED.store(on, Ordering::Relaxed);
}

pub fn key_allowed() -> bool {
    KEY_ALLOWED.load(Ordering::Relaxed)
}

extern "C" fn can_become_key(_this: &AnyObject, _sel: Sel) -> Bool {
    Bool::new(key_allowed())
}

/// The `NSWindow` getter AppKit asks to decide whether a window is a main window: never, for the dock.
extern "C" fn never(_this: &AnyObject, _sel: Sel) -> Bool {
    Bool::NO
}

/// `AtlasDockPanel`: an `NSPanel` whose key status follows [`KEY_ALLOWED`].
static PANEL_CLASS: LazyLock<&'static AnyClass> = LazyLock::new(|| {
    let panel = AnyClass::get(c"NSPanel").expect("AppKit has NSPanel");
    let mut b =
        ClassBuilder::new(c"AtlasDockPanel", panel).expect("AtlasDockPanel is registered once");
    // SAFETY: the signatures match `-canBecomeKeyWindow` / `-canBecomeMainWindow` (`BOOL (id, SEL)`).
    unsafe {
        b.add_method(
            sel!(canBecomeKeyWindow),
            can_become_key as extern "C" fn(_, _) -> _,
        );
        b.add_method(sel!(canBecomeMainWindow), never as extern "C" fn(_, _) -> _);
    }
    b.register()
});

/// Turn the Tauri window `ns` (an `NSWindow*`) into the dock panel. Main thread (called from `build`).
pub fn make_panel(ns: *mut std::ffi::c_void) {
    let w = ns.cast::<AnyObject>();
    let panel = *PANEL_CLASS;
    // SAFETY: `ns` is Tauri's live NSWindow; the swap is only legal between classes of equal instance size, which
    // is checked first (a different layout would corrupt the object, so the window then stays a plain window).
    unsafe {
        let Some(w) = w.as_ref() else { return };
        if w.class().instance_size() != panel.instance_size() {
            eprintln!("dock: NSPanel layout differs from the window class; staying a plain window");
            return;
        }
        AnyObject::set_class(w, panel);
        let win = &*(w as *const AnyObject).cast::<objc2_app_kit::NSWindow>();
        win.setStyleMask(win.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        win.setLevel(objc2_app_kit::NSStatusWindowLevel);
        win.setHidesOnDeactivate(false);
        win.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let _: () = objc2::msg_send![w, setFloatingPanel: true];
        let _: () = objc2::msg_send![w, setBecomesKeyOnlyIfNeeded: true];
    }
}

/// Make the (non-activating) panel the key window so the composer input receives keys, without activating the app.
/// Only the user's app calls this (`dock::dock_keys` is a no-op wall in agent mode, see the guard test).
pub fn make_key(win: &tauri::Window) {
    let Ok(ns) = win.ns_window() else { return };
    // SAFETY: Tauri's live NSWindow, main thread; `-makeKeyWindow` takes no arguments.
    unsafe {
        let _: () = objc2::msg_send![ns.cast::<AnyObject>(), makeKeyWindow];
    }
}

/// The capture overlay needs Esc and Space like the composer needs typing: while it is up, key status is allowed and
/// the given window becomes key (still without activating the app). Closing it revokes both. The dock keeps its own
/// say: the composer re-enables the flag when it opens.
pub fn overlay_keys(win: Option<&tauri::Window>) {
    // An agent instance never takes key status (it would steal the user's keyboard): the wall is here, in the one
    // place every capture window asks, not at each caller.
    if !crate::agent_mode::Mode::current().focus_allowed() {
        return;
    }
    set_key_allowed(win.is_some());
    if let Some(w) = win {
        make_key(w);
    }
}
