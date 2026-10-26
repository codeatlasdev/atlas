//! The AppKit side: a click-through `NSView` per terminal, laid over the pane in the
//! window's content view and used as the wgpu surface. Main thread only.

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSResponder, NSView, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};

define_class!(
    // SAFETY: NSView has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AtlasTermView"]
    struct TermView;

    impl TermView {
        /// Transparent to the mouse: clicks reach the webview (focus, selection later).
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _p: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

/// Top-left logical rect -> AppKit frame in the content view.
fn frame(content: &NSView, x: f64, y: f64, w: f64, h: f64) -> NSRect {
    let y = if content.isFlipped() {
        y
    } else {
        content.bounds().size.height - y - h
    };
    NSRect::new(NSPoint::new(x, y), NSSize::new(w.max(1.0), h.max(1.0)))
}

/// New view in the window's content view; returns the retained `NSView*`.
pub fn create(ns_window: usize, x: f64, y: f64, w: f64, h: f64) -> Result<usize, String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: Tauri's live main NSWindow.
    let win = unsafe { &*(ns_window as *const NSWindow) };
    let content = win.contentView().ok_or("no content view")?;
    // SAFETY: plain NSView designated initializer.
    let view: Retained<TermView> =
        unsafe { msg_send![TermView::alloc(mtm), initWithFrame: frame(&content, x, y, w, h)] };
    view.setWantsLayer(true);
    content.addSubview(&view);
    Ok(Retained::into_raw(view) as usize)
}

pub fn set_frame(view: usize, x: f64, y: f64, w: f64, h: f64) {
    // SAFETY: retained in `create`, released only in `destroy`.
    let view = unsafe { &*(view as *const NSView) };
    if let Some(sup) = unsafe { view.superview() } {
        view.setFrame(frame(&sup, x, y, w, h));
    }
}

pub fn destroy(view: usize) {
    // SAFETY: balances `Retained::into_raw` in `create`.
    if let Some(view) = unsafe { Retained::from_raw(view as *mut NSView) } {
        view.removeFromSuperview();
    }
}
