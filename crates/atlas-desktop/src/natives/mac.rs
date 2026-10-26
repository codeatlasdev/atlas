//! The AppKit side of the native-view engine: stacking, the opaque underlay, and the input monitor. Main thread only
//! unless said otherwise.
//!
//! The window's content view holds, bottom to top: the vibrancy material (`window_vibrancy`), the **underlay** (an
//! opaque theme-coloured view: what shows through a slot hole for the frame before a native view has landed),
//! every native view (GPU terminals, browser pages: `lower_below_page`), and the main webview on top. The page is
//! transparent exactly over each native view's slot, and draws overlays, scrims and shadows over it like over any
//! other pixels.

use super::registry::Rect;
use super::router::{Decision, Kind, Owner, Router, Target};
use super::with_registry;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSEvent, NSEventMask, NSEventType, NSView, NSWindowOrderingMode,
};
use objc2_core_graphics::CGColor;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{Emitter, Manager};

/// `NSView*` of the main webview (the page); set once by `install`. Compared and used as a reference for stacking.
static PAGE: AtomicUsize = AtomicUsize::new(0);
/// `NSView*` of the underlay; set once by `install`.
static UNDERLAY: AtomicUsize = AtomicUsize::new(0);

/// Put `view` directly below the page, above the underlay and every other native view: the page paints over it.
/// Call on the main thread right after the view joined the window's content view (the terminal's own `NSView`, a
/// browser's child `WKWebView`). Re-calling is harmless.
pub fn lower_below_page(view: usize) {
    let page = PAGE.load(Ordering::Relaxed);
    if page == 0 || view == 0 {
        return;
    }
    // SAFETY: both are live NSViews of this process (the page for the whole app, `view` retained by its owner).
    let (view, page) = unsafe { (&*(view as *const NSView), &*(page as *const NSView)) };
    // SAFETY: plain getter, main thread.
    let Some(sup) = (unsafe { view.superview() }) else {
        return;
    };
    sup.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, Some(page));
}

/// Set the underlay's colour (`[r, g, b, a]`, 0..1, sRGB; clamped). Main thread.
pub fn set_ground(rgba: [f64; 4]) {
    let u = UNDERLAY.load(Ordering::Relaxed);
    if u == 0 {
        return;
    }
    // SAFETY: the underlay lives for the whole app.
    let view = unsafe { &*(u as *const NSView) };
    let Some(layer) = view.layer() else {
        return;
    };
    let [r, g, b, a] = rgba.map(|c| {
        if c.is_finite() {
            c.clamp(0.0, 1.0)
        } else {
            0.0
        }
    });
    layer.setBackgroundColor(Some(&CGColor::new_srgb(r, g, b, a)));
}

/// Record the main webview, add the underlay below it, and install the input monitor. Setup, main thread, after the
/// main window and its webview exist and before any native view does (so every native view lands above the underlay).
pub fn install(app: &tauri::AppHandle) {
    if let Some(wv) = app.get_webview("main") {
        // Queued onto the main thread: the marker is taken there (it cannot cross threads).
        let _ = wv.with_webview(move |pw| {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let page = pw.inner() as usize;
            PAGE.store(page, Ordering::Relaxed);
            // SAFETY: the main webview lives for the whole app.
            let page_view = unsafe { &*(page as *const NSView) };
            // SAFETY: plain getter, main thread.
            let Some(sup) = (unsafe { page_view.superview() }) else {
                return;
            };
            // SAFETY: plain NSView designated initializer.
            let u: Retained<NSView> =
                unsafe { msg_send![NSView::alloc(mtm), initWithFrame: sup.bounds()] };
            u.setWantsLayer(true);
            u.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            sup.addSubview_positioned_relativeTo(&u, NSWindowOrderingMode::Below, Some(page_view));
            UNDERLAY.store(Retained::into_raw(u) as usize, Ordering::Relaxed);
        });
    }
    if let Some(Ok(ns)) = app.get_window("main").map(|w| w.ns_window()) {
        // SAFETY: Tauri's live NSWindow; moves must reach the monitor even with no tracking area over the point.
        unsafe {
            let _: () = msg_send![ns as *mut AnyObject, setAcceptsMouseMovedEvents: true];
        }
    }
    install_input_monitor(app);
}

/// The frame of `view` in window points, top-left origin (the page's coordinates), or none when it is hidden or has
/// no superview.
fn frame_in_window(view: &NSView, content: &NSView) -> Option<Rect> {
    if view.isHidden() {
        return None;
    }
    // SAFETY: plain getter, main thread.
    let sup = unsafe { view.superview() }?;
    let f = content.convertRect_fromView(view.frame(), Some(&sup));
    let y = if content.isFlipped() {
        f.origin.y
    } else {
        content.bounds().size.height - f.origin.y - f.size.height
    };
    Some(Rect::new(f.origin.x, y, f.size.width, f.size.height))
}

/// One local monitor of the main window: the router's decision, carried out.
///
/// A native view owns the event: the matching responder method of that view gets it (a press also makes it first
/// responder and is reported as `native://pointerdown`, so the page's outside-press logic closes menus and popovers:
/// `nativeClicks.ts`) and AppKit's own dispatch to the page does not happen (consumed). The page owns it: AppKit
/// delivers it untouched. Moves always go to the page; the native view under the pointer is told too.
fn install_input_monitor(app: &tauri::AppHandle) {
    use block2::RcBlock;
    let app = app.clone();
    let router = std::sync::Mutex::new(Router::default());
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        let pass = ev.as_ptr();
        // SAFETY: AppKit passes a live event for the duration of the call.
        let e = unsafe { ev.as_ref() };
        let Some(mtm) = MainThreadMarker::new() else {
            return pass;
        };
        let Some(main) = app.get_window("main").and_then(|w| w.ns_window().ok()) else {
            return pass;
        };
        let Some(win) = e.window(mtm) else {
            return pass;
        };
        if !std::ptr::eq(
            Retained::as_ptr(&win) as *const AnyObject,
            main as *const AnyObject,
        ) {
            return pass;
        }
        let Some(kind) = kind_of(e.r#type()) else {
            return pass;
        };
        let Some(content) = win.contentView() else {
            return pass;
        };
        let at = e.locationInWindow();
        let p = content.convertPoint_fromView(at, None);
        let y = if content.isFlipped() {
            p.y
        } else {
            content.bounds().size.height - p.y
        };
        let (targets, overlays) = with_registry(|r| {
            let targets: Vec<Target> = r
                .interactive()
                .into_iter()
                // SAFETY: registered views are live until `unregister`, which precedes their destruction.
                .filter_map(|v| {
                    let view = unsafe { &*(v.ptr as *const NSView) };
                    frame_in_window(view, &content).map(|frame| Target { view: v.ptr, frame })
                })
                .collect();
            (targets, r.overlays().to_vec())
        });
        let Decision { owner, consume } = router
            .lock()
            .map(|mut r| r.decide(kind, &targets, &overlays, p.x, y))
            .unwrap_or(Decision {
                owner: Owner::Page,
                consume: false,
            });
        let Owner::Native(ptr) = owner else {
            return pass;
        };
        // SAFETY: `ptr` came from `targets` just now, so it is a live registered view.
        let view = unsafe { &*(ptr as *const NSView) };
        let Some(sel) = selector_for(e.r#type()) else {
            return pass;
        };
        if matches!(
            e.r#type(),
            NSEventType::LeftMouseDown | NSEventType::RightMouseDown | NSEventType::OtherMouseDown
        ) {
            if !matches!(e.r#type(), NSEventType::OtherMouseDown) {
                let _ = win.makeFirstResponder(Some(view));
            }
            let _ = app.emit(
                "native://pointerdown",
                serde_json::json!({ "x": p.x, "y": y }),
            );
        }
        // SAFETY: every selector below is an NSResponder mouse method taking the event.
        unsafe {
            let _: () = msg_send![view, performSelector: sel, withObject: e];
        }
        if consume { std::ptr::null_mut() } else { pass }
    });
    let mask = NSEventMask::LeftMouseDown
        | NSEventMask::LeftMouseUp
        | NSEventMask::LeftMouseDragged
        | NSEventMask::RightMouseDown
        | NSEventMask::RightMouseUp
        | NSEventMask::RightMouseDragged
        | NSEventMask::OtherMouseDown
        | NSEventMask::OtherMouseUp
        | NSEventMask::OtherMouseDragged
        | NSEventMask::ScrollWheel
        | NSEventMask::MouseMoved;
    // SAFETY: the block returns null (consumed) or the event it was given. Lives for the whole process.
    let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    std::mem::forget(token);
}

fn kind_of(t: NSEventType) -> Option<Kind> {
    Some(match t {
        NSEventType::LeftMouseDown | NSEventType::RightMouseDown | NSEventType::OtherMouseDown => {
            Kind::Down
        }
        NSEventType::LeftMouseDragged
        | NSEventType::RightMouseDragged
        | NSEventType::OtherMouseDragged => Kind::Drag,
        NSEventType::LeftMouseUp | NSEventType::RightMouseUp | NSEventType::OtherMouseUp => {
            Kind::Up
        }
        NSEventType::MouseMoved => Kind::Move,
        NSEventType::ScrollWheel => Kind::Scroll,
        _ => return None,
    })
}

fn selector_for(t: NSEventType) -> Option<Sel> {
    Some(match t {
        NSEventType::LeftMouseDown => sel!(mouseDown:),
        NSEventType::LeftMouseUp => sel!(mouseUp:),
        NSEventType::LeftMouseDragged => sel!(mouseDragged:),
        NSEventType::RightMouseDown => sel!(rightMouseDown:),
        NSEventType::RightMouseUp => sel!(rightMouseUp:),
        NSEventType::RightMouseDragged => sel!(rightMouseDragged:),
        NSEventType::OtherMouseDown => sel!(otherMouseDown:),
        NSEventType::OtherMouseUp => sel!(otherMouseUp:),
        NSEventType::OtherMouseDragged => sel!(otherMouseDragged:),
        NSEventType::ScrollWheel => sel!(scrollWheel:),
        NSEventType::MouseMoved => sel!(mouseMoved:),
        _ => return None,
    })
}
