//! AppKit side of the dock: the Tauri window becomes a non-activating floating panel.
//!
//! Tauri's window is a `TaoWindow` (an `NSWindow`). A non-activating panel is an `NSPanel` with
//! `NSWindowStyleMaskNonactivatingPanel`: clicking it or typing in it never activates Atlas, so the app you are
//! working in keeps the keyboard focus. Only `NSPanel` honours that style bit, so the window's class is swapped for
//! a runtime subclass of `NSPanel` (`object_setClass`, the technique tauri-nspanel uses; `NSPanel` adds no instance
//! variables to `NSWindow`, which [`layout_allows_swap`] checks before it swaps). The subclass answers
//! `canBecomeKeyWindow` from a flag: false while the dock is the idle pill (it can never take the key from another app), true while the composer is
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

/// Whether an object allocated as `object` bytes (its class, e.g. Tao's window) may take `subclass` (our `NSPanel`
/// subclass): `NSPanel` must add no instance variables to `NSWindow` (`panel == window_base`) and neither may the
/// subclass, so every variable the object already has, the base class's and the extra ones of its own class (Tao keeps
/// its Rust state after `NSWindow`'s), stays where its code looks for it, and the object is at least as big as the
/// class it becomes. Pure, so the rule is tested with Tao-like sizes.
pub fn layout_allows_swap(
    object: usize,
    window_base: usize,
    panel: usize,
    subclass: usize,
) -> bool {
    panel == window_base && subclass == panel && object >= subclass
}

/// Turn the Tauri window `ns` (an `NSWindow*`) into the dock panel. Main thread (called from `build`).
pub fn make_panel(ns: *mut std::ffi::c_void) {
    let w = ns.cast::<AnyObject>();
    let panel = *PANEL_CLASS;
    let base = |name: &std::ffi::CStr| AnyClass::get(name).map(AnyClass::instance_size);
    // SAFETY: `ns` is Tauri's live NSWindow; the swap is only legal when the layout rule holds, which is checked
    // first (a different layout would corrupt the object, so the window then stays a plain window and the caller
    // refuses to order it in, see `ensure_nonactivating`).
    unsafe {
        let Some(w) = w.as_ref() else { return };
        let (Some(window_base), Some(panel_base)) = (base(c"NSWindow"), base(c"NSPanel")) else {
            return;
        };
        if !layout_allows_swap(
            w.class().instance_size(),
            window_base,
            panel_base,
            panel.instance_size(),
        ) {
            eprintln!("dock: NSPanel layout differs from the window class; staying a plain window");
            return;
        }
        // The runtime call itself, not `AnyObject::set_class`: that one asserts (debug) that the two classes are the
        // same size, which is the rule for objects whose own class has variables the new class does not know. Here
        // the allocation is Tao's (bigger), the new class adds no variable, and Tao's state is a plain `AtomicBool`
        // with nothing to drop, so nothing is read through the wrong class and nothing leaks (`layout_allows_swap`).
        objc2::ffi::object_setClass((w as *const AnyObject).cast_mut(), panel);
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
        prevent_activation(w);
    }
}

/// Tell the window server that a click on, or ordering in, this window never activates the app.
///
/// `NSPanel` sets that window-server tag (`kCGSPreventsActivationTagBit`) only while it initialises, when the style
/// mask already has `NonactivatingPanel`. [`make_panel`] changes the class and the style of a window that was
/// created as an ordinary `NSWindow`, so AppKit's view of it says "non-activating" while the window server's tag was
/// never set: the first mouse-down on it (the drag of a region on the capture overlay, a click on a card) activated
/// Atlas and raised its main window over the app being captured. `-[NSWindow _setPreventsActivation:]` is the call
/// `NSPanel` itself makes; it is private, so it is only sent when the window answers to it (the window then stays as
/// it was, and [`ensure_nonactivating`] says so).
unsafe fn prevent_activation(w: &AnyObject) {
    let set = sel!(_setPreventsActivation:);
    // SAFETY: `w` is a live NSWindow; the selector is only sent when it responds to it, and takes one BOOL.
    unsafe {
        if w.class().responds_to(set) {
            let _: () = objc2::msg_send![w, _setPreventsActivation: true];
        }
    }
}

/// Make sure the capture window `ns` cannot activate the app, right before it is ordered in (main thread): an
/// `NSPanel` with the non-activating style and the window-server tag. A window that is not (a panel that lost its
/// style, one whose tag was never set) is converted again; `false` when it still is not safe, and the caller must not
/// order it in (a missing capture window is a bug the person can report, an app that jumps in front of what they
/// were looking at is not acceptable).
pub fn ensure_nonactivating(ns: *mut std::ffi::c_void) -> bool {
    if activation_of(ns).safe() {
        return true;
    }
    make_panel(ns);
    let a = activation_of(ns);
    if !a.safe() {
        eprintln!("capture: window cannot be made non-activating, not shown: {a:?}");
    }
    a.safe()
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

/// What decides whether ordering a window in can bring the app forward, read from AppKit and the window server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Activation {
    /// The window's class is an `NSPanel` (only a panel honours the non-activating style).
    pub panel: bool,
    /// Its style mask has `NSWindowStyleMaskNonactivatingPanel`.
    pub nonactivating_style: bool,
    /// The window server's `kCGSPreventsActivationTagBit` is set (a click or an ordering-in never activates the
    /// owner). `None` while the window has no window-server number yet, so the tag cannot be read.
    pub prevents_tag: Option<bool>,
}

impl Activation {
    /// Ordering the window in or clicking it can never activate the app. Pure.
    pub fn safe(self) -> bool {
        self.panel && self.nonactivating_style && self.prevents_tag != Some(false)
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> u32;
    fn CGSGetWindowTags(cid: u32, wid: u32, tags: *mut [i32; 2], max: usize) -> i32;
}

/// `kCGSPreventsActivationTagBit`.
const PREVENTS_ACTIVATION: i32 = 1 << 16;

/// Read [`Activation`] of the `NSWindow*` `ns`. Main thread.
pub fn activation_of(ns: *mut std::ffi::c_void) -> Activation {
    let w = ns.cast::<AnyObject>();
    // SAFETY: `ns` is Tauri's live NSWindow, main thread; these are plain getters.
    unsafe {
        let Some(w) = w.as_ref() else {
            return Activation {
                panel: false,
                nonactivating_style: false,
                prevents_tag: None,
            };
        };
        let panel_class = AnyClass::get(c"NSPanel").expect("AppKit has NSPanel");
        let is_panel: Bool = objc2::msg_send![w, isKindOfClass: panel_class];
        let win = &*(w as *const AnyObject).cast::<objc2_app_kit::NSWindow>();
        let number = win.windowNumber();
        let prevents_tag = u32::try_from(number).ok().filter(|n| *n > 0).and_then(|n| {
            let mut tags = [0i32; 2];
            // SAFETY: the window server fills the two words owned by this frame (its size argument is in bits).
            (CGSGetWindowTags(CGSMainConnectionID(), n, &mut tags, 32) == 0)
                .then_some(tags[0] & PREVENTS_ACTIVATION != 0)
        });
        Activation {
            panel: is_panel.as_bool(),
            nonactivating_style: win
                .styleMask()
                .contains(NSWindowStyleMask::NonactivatingPanel),
            prevents_tag,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Activation, PANEL_CLASS, layout_allows_swap};
    use objc2::runtime::AnyClass;

    fn size_of_class(name: &std::ffi::CStr) -> usize {
        AnyClass::get(name).expect("AppKit class").instance_size()
    }

    /// The first version of the conversion required the window's class to be as big as `NSPanel`, and Tao's window
    /// class carries its own Rust state after `NSWindow`'s variables, so it never matched: every capture window (and
    /// the dock) stayed an ordinary `NSWindow` and brought the app forward. What is true is that a panel adds no
    /// variables to `NSWindow`; a window class that is bigger (Tao's) can take the panel class, a smaller or foreign
    /// allocation cannot.
    #[test]
    fn a_window_class_bigger_than_nswindow_may_become_a_panel_and_a_smaller_one_may_not() {
        let (window, panel, ours) = (
            size_of_class(c"NSWindow"),
            size_of_class(c"NSPanel"),
            PANEL_CLASS.instance_size(),
        );
        // the facts the conversion rests on, read from this macOS
        assert_eq!(
            panel, window,
            "NSPanel adds no instance variables to NSWindow"
        );
        assert_eq!(ours, panel, "the dock panel class adds none either");
        // Tao's window: NSWindow's variables plus its own state
        assert!(layout_allows_swap(window + 8, window, panel, ours));
        // exactly NSWindow's size
        assert!(layout_allows_swap(window, window, panel, ours));
        // an allocation smaller than the panel class would be written past its end
        assert!(!layout_allows_swap(window - 8, window, panel, ours));
        // a macOS where NSPanel did grow, or a subclass that did: the swap would overlap the window's own variables
        assert!(!layout_allows_swap(window + 8, window, panel + 8, ours));
        assert!(!layout_allows_swap(window + 64, window, panel, ours + 8));
    }

    fn act(panel: bool, nonactivating_style: bool, prevents_tag: Option<bool>) -> Activation {
        Activation {
            panel,
            nonactivating_style,
            prevents_tag,
        }
    }

    /// The decision that keeps Atlas from coming forward when a capture window is ordered in or clicked: it is safe
    /// only as a panel, with the non-activating style, and without a window-server tag that says otherwise. Each
    /// row is one way the app used to come forward (or could).
    #[test]
    fn only_a_nonactivating_panel_whose_tag_is_set_or_not_readable_yet_is_safe() {
        assert!(act(true, true, Some(true)).safe());
        // no window number yet: the tag cannot be read, the class and the style decide
        assert!(act(true, true, None).safe());
        // the original bug: converted after creation, style set, window server never told
        assert!(!act(true, true, Some(false)).safe());
        // a plain NSWindow (`make_panel` skipped or refused): `makeKeyAndOrderFront` activates the app
        assert!(!act(false, false, Some(false)).safe());
        assert!(!act(false, true, Some(true)).safe());
        // a panel whose style bit was dropped
        assert!(!act(true, false, Some(true)).safe());
        assert!(!act(true, false, None).safe());
    }
}
