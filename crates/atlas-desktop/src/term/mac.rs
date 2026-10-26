//! The AppKit side: one `NSView` per terminal, used as the wgpu surface and laid **below** the main
//! `WKWebView` (see `layering` for the stack and why). Main thread only.
//!
//! The view never takes the mouse (`hitTest:` → nil): the transparent webview above it receives pointer
//! and key events and the slot's DOM handlers forward them (`pointer.ts`, `term_*`). That keeps scrolling,
//! selection, ⌘-click, IME and focus exactly where they were while a menu, dialog or the sidebar peek
//! can now simply be drawn over the terminal.
//!
//! Geometry has one owner per frame: [`apply`] moves every terminal of a batch inside one Core Animation
//! transaction with implicit animations off, resizes each Metal layer's drawable in the same block and, when
//! the size changed, presents with the transaction, so the surface and the frame land in the same screen
//! frame (no stretched or blank frame while a pane is dragged). [`after_page_presents`] holds that block
//! until the web content has presented its own commit, so page and terminal appear together.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSResponder, NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CATransaction;

use super::layering::Target;

define_class!(
    // SAFETY: NSView has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AtlasTermView"]
    struct TermView;

    impl TermView {
        /// Transparent to the mouse: the webview above forwards what happens over the slot.
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

/// Is `v` a web view (the page or a browser child)?
fn is_web(v: &NSView) -> bool {
    let any: &AnyObject = v;
    let name = any.class().name().to_string_lossy();
    name.contains("WKWebView") || name.contains("WryWebView")
}

/// The main page's web view: the content view's first web view (browser children are added after it).
fn page_webview(content: &NSView) -> Option<Retained<NSView>> {
    content.subviews().iter().find(|v| is_web(v))
}

/// Put `view` directly below the page's web view, above the vibrancy view; at the bottom when there
/// is no page yet. The one place that decides the order.
fn place_below_page(content: &NSView, view: &NSView) {
    let page = page_webview(content);
    content.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, page.as_deref());
}

/// New view below the window's page web view; returns the retained `NSView*`. The layer is opaque and
/// painted with `bg` (`#rrggbb` as 0..1 floats) from its first instant, so a frame in which the Metal
/// layer has nothing yet shows the terminal's colour, never the desktop through the transparent window.
pub fn create(
    ns_window: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    bg: [f64; 3],
) -> Result<usize, String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: Tauri's live main NSWindow.
    let win = unsafe { &*(ns_window as *const NSWindow) };
    let content = win.contentView().ok_or("no content view")?;
    // SAFETY: plain NSView designated initializer.
    let view: Retained<TermView> =
        unsafe { msg_send![TermView::alloc(mtm), initWithFrame: frame(&content, x, y, w, h)] };
    view.setWantsLayer(true);
    set_background(&view, bg);
    place_below_page(&content, &view);
    Ok(Retained::into_raw(view) as usize)
}

/// `CGColor` of an sRGB triple, as the object `CALayer.backgroundColor` takes.
fn cg_color(rgb: [f64; 3]) -> *mut AnyObject {
    // SAFETY: NSColor class methods; `CGColor` is autoreleased by the NSColor, which the caller's
    // autorelease pool outlives (the layer retains it on assignment).
    unsafe {
        let cls = objc2::class!(NSColor);
        let c: *mut AnyObject = msg_send![cls, colorWithSRGBRed: rgb[0], green: rgb[1], blue: rgb[2], alpha: 1.0f64];
        msg_send![c, CGColor]
    }
}

/// Paint a bare `CALayer` with an opaque sRGB colour (the page stage's terminal fills).
pub fn paint_layer(layer: &objc2_quartz_core::CALayer, rgb: [f64; 3]) {
    // SAFETY: CALayer setters on the main thread; the colour is retained by the layer.
    unsafe {
        let color = cg_color(rgb);
        let _: () = msg_send![layer, setBackgroundColor: color];
        let _: () = msg_send![layer, setOpaque: true];
    }
}

/// Opaque layer background: the colour a layer shows before its first drawable and at any gap.
pub fn set_background(view: &NSView, rgb: [f64; 3]) {
    if let Some(layer) = view.layer() {
        // SAFETY: CALayer setters on the main thread; the colour is retained by the layer.
        unsafe {
            let color = cg_color(rgb);
            let _: () = msg_send![&*layer, setBackgroundColor: color];
            let _: () = msg_send![&*layer, setOpaque: true];
        }
    }
}

/// Recolor a live terminal view's backing (a theme switch).
pub fn recolor(view: usize, rgb: [f64; 3]) {
    // SAFETY: retained in `create`, released only in `destroy`.
    let view = unsafe { &*(view as *const NSView) };
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    set_background(view, rgb);
    CATransaction::commit();
}

/// One terminal's move for [`apply`].
pub struct Move {
    pub view: usize,
    pub target: Target,
    /// Physical pixels of the surface after the move (`Over` only), to size the drawable in the same block.
    pub pixels: Option<(u32, u32)>,
}

/// Move every terminal of a batch in ONE Core Animation transaction, implicit animations off.
///
/// A terminal that changes size gets its Metal drawable resized in this block and presents with the
/// transaction until the next call, so the new surface and the new frame are one commit (Zed and Ghostty
/// do the same on live resize). Parked and same-size moves keep the cheap async present.
pub fn apply(moves: &[Move]) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    for m in moves {
        // SAFETY: retained in `create`, released only in `destroy`.
        let view = unsafe { &*(m.view as *const NSView) };
        let Some(sup) = (unsafe { view.superview() }) else {
            continue;
        };
        let (x, y, w, h) = match m.target {
            Target::Over((x, y, w, h)) => (x, y, w, h),
            Target::Park(w, h) => (super::layering::OFFSCREEN, super::layering::OFFSCREEN, w, h),
        };
        let new = frame(&sup, x, y, w, h);
        let resized = view.frame().size != new.size;
        if resized {
            sync_with_transaction(view, true);
        }
        view.setFrame(new);
        if resized && let Some((pw, ph)) = m.pixels {
            set_drawable_size(view, pw, ph);
        }
    }
    CATransaction::commit();
    // The transaction has committed: the resized presents above landed with it. Back to async presents.
    for m in moves {
        // SAFETY: as above.
        let view = unsafe { &*(m.view as *const NSView) };
        sync_with_transaction(view, false);
    }
}

fn sync_with_transaction(view: &NSView, on: bool) {
    if let Some(layer) = view.layer() {
        // SAFETY: `CAMetalLayer.presentsWithTransaction`; the view's layer is the CAMetalLayer wgpu installed,
        // and the selector is guarded so a plain layer (before the surface exists) is left alone.
        unsafe {
            let sel: Sel = sel!(setPresentsWithTransaction:);
            let ok: bool = msg_send![&*layer, respondsToSelector: sel];
            if ok {
                let _: () = msg_send![&*layer, setPresentsWithTransaction: on];
            }
        }
    }
}

fn set_drawable_size(view: &NSView, w: u32, h: u32) {
    if let Some(layer) = view.layer() {
        // SAFETY: as above; `drawableSize` is a CGSize in pixels.
        unsafe {
            let ok: bool = msg_send![&*layer, respondsToSelector: sel!(setDrawableSize:)];
            if ok {
                let _: () = msg_send![&*layer, setDrawableSize: NSSize::new(w as f64, h as f64)];
            }
        }
    }
}

/// Run `f` on the main thread once the main web view has presented its next commit
/// (`-[WKWebView _doAfterNextPresentationUpdate:]`), so a native frame lands in the same screen frame as the
/// DOM change that asked for it. Falls back to running `f` now when the SPI or the page is missing.
pub fn after_page_presents(ns_window: usize, f: impl FnOnce() + 'static) {
    // SAFETY: Tauri's live main NSWindow, main thread.
    let win = unsafe { &*(ns_window as *const NSWindow) };
    let page = win.contentView().and_then(|c| page_webview(&c));
    let Some(page) = page else { return f() };
    // SAFETY: selector existence checked; the block is copied by WebKit and invoked once on the main thread.
    unsafe {
        let ok: bool = msg_send![&*page, respondsToSelector: sel!(_doAfterNextPresentationUpdate:)];
        if !ok {
            return f();
        }
        let cell = std::cell::Cell::new(Some(f));
        let block = RcBlock::new(move || {
            if let Some(f) = cell.take() {
                f();
            }
        });
        let _: () = msg_send![&*page, _doAfterNextPresentationUpdate: &*block];
    }
}

/// Is the view still in the window, below the page's web view? False after anything that rebuilt
/// the content view's children (a window/screen change, a webview re-added): the page would then
/// paint over a terminal that is "there" but never visible again.
pub fn is_layered(view: usize) -> bool {
    // SAFETY: retained in `create`, released only in `destroy`.
    let view = unsafe { &*(view as *const NSView) };
    let Some(sup) = (unsafe { view.superview() }) else {
        return false;
    };
    let subs = sup.subviews();
    let pos = |t: &NSView| subs.iter().position(|v| std::ptr::eq(&*v, t));
    match (pos(view), subs.iter().position(|v| is_web(&v))) {
        (Some(me), Some(page)) => me < page,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Put the view back below the page (it may have been re-parented or the page re-added over it).
pub fn relayer(ns_window: usize, view: usize) {
    // SAFETY: Tauri's live main NSWindow; the view is retained in `create`.
    let win = unsafe { &*(ns_window as *const NSWindow) };
    let view = unsafe { &*(view as *const NSView) };
    let Some(content) = win.contentView() else {
        return;
    };
    // Re-adding moves an existing subview; no hide, no frame change.
    place_below_page(&content, view);
}

/// The window itself paints the app's colour (`#rrggbb`) behind everything, so any one-frame mismatch
/// between the page and a native view shows the app, never the desktop. The vibrancy view, below the
/// terminals, still draws the sidebar material; this colour only backs the transparent window.
pub fn set_window_background(ns_window: usize, rgb: [f64; 3]) {
    // SAFETY: Tauri's live main NSWindow, main thread.
    unsafe {
        let win = ns_window as *mut AnyObject;
        let cls = objc2::class!(NSColor);
        let c: *mut AnyObject = msg_send![cls, colorWithSRGBRed: rgb[0], green: rgb[1], blue: rgb[2], alpha: 1.0f64];
        let _: () = msg_send![win, setBackgroundColor: c];
    }
}

pub fn destroy(view: usize) {
    // SAFETY: balances `Retained::into_raw` in `create`.
    if let Some(view) = unsafe { Retained::from_raw(view as *mut NSView) } {
        view.removeFromSuperview();
    }
}
