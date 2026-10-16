//! AppKit side of `browser_keys`: a local `keyDown` monitor that runs before the key reaches a
//! browser page.
//!
//! The monitor only acts while the window's first responder sits inside one of the browser child
//! webviews. With focus anywhere else (the app's own webview, the GPU terminal) the frontend's
//! keymap handler already runs the command, so forwarding there would run it twice.
//!
//! A forwarded key is swallowed (the page never sees it) and sent to the app as `browser://key`;
//! any other key is returned untouched.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, Message};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSResponder, NSView};
use parking_lot::Mutex;
use std::ptr::NonNull;
use tauri::{AppHandle, Emitter, Manager};

use crate::browser_keys::{EVENT, Mods, forward};

/// `WKWebView*` of every live browser child, recorded by `track` once the view exists (the callback
/// runs on the main thread; a key cannot arrive before it). Pointers are only compared, never dereferenced.
static VIEWS: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());

/// Remember the native view of tab webview `label`.
pub fn track(v: &tauri::Webview, label: String) {
    let _ = v.with_webview(move |pw| {
        let mut views = VIEWS.lock();
        views.retain(|(l, _)| *l != label);
        views.push((label, pw.inner() as usize));
    });
}

/// Forget a closed tab's view.
pub fn untrack(label: &str) {
    VIEWS.lock().retain(|(l, _)| l != label);
}

/// Is `responder` a browser webview or one of its subviews?
fn inside(responder: &NSResponder) -> bool {
    let views = VIEWS.lock();
    let mut cur: Option<Retained<NSView>> = <NSResponder as AsRef<AnyObject>>::as_ref(responder)
        .downcast_ref::<NSView>()
        .map(|v| v.retain());
    while let Some(v) = cur {
        if views
            .iter()
            .any(|(_, p)| *p == Retained::as_ptr(&v) as usize)
        {
            return true;
        }
        cur = unsafe { v.superview() };
    }
    false
}

fn mods_of(f: NSEventModifierFlags) -> Mods {
    Mods {
        ctrl: f.contains(NSEventModifierFlags::Control),
        alt: f.contains(NSEventModifierFlags::Option),
        shift: f.contains(NSEventModifierFlags::Shift),
        meta: f.contains(NSEventModifierFlags::Command),
    }
}

/// Returns true to swallow the event.
fn on_key(app: &AppHandle, ev: &NSEvent) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Some(forwarded) = forward(ev.keyCode(), mods_of(ev.modifierFlags())) else {
        return false;
    };
    let Some(win) = ev.window(mtm) else {
        return false;
    };
    let Some(main) = app.get_window("main").and_then(|w| w.ns_window().ok()) else {
        return false;
    };
    if !std::ptr::eq(
        Retained::as_ptr(&win) as *const AnyObject,
        main as *const AnyObject,
    ) {
        return false;
    }
    let Some(first) = win.firstResponder() else {
        return false;
    };
    if !inside(&first) {
        return false;
    }
    // A held chord repeats: swallow every repeat (the page must not see it) but switch once.
    if !ev.isARepeat() {
        let _ = app.emit(EVENT, forwarded);
    }
    true
}

/// Install the monitor. Main thread (Tauri `setup`).
pub fn install(app: &AppHandle) {
    let app = app.clone();
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the duration of the call.
        let ev = unsafe { ev.as_ref() };
        if on_key(&app, ev) {
            std::ptr::null_mut()
        } else {
            ev as *const NSEvent as *mut NSEvent
        }
    });
    // SAFETY: the block returns null or the event it was given. The monitor lives for the whole process, so
    // the returned token is intentionally leaked.
    let token = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
    };
    std::mem::forget(token);
}
