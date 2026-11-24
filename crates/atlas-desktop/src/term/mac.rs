//! The AppKit side: a click-through `NSView` per terminal, laid in the window's content view BELOW the main
//! webview (the page is transparent over the pane's slot and paints overlays above it: `natives`) and used as the
//! wgpu surface. Main thread only.

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
    crate::natives::lower_below_page(Retained::as_ptr(&view) as usize);
    Ok(Retained::into_raw(view) as usize)
}

pub fn set_frame(view: usize, x: f64, y: f64, w: f64, h: f64) {
    // SAFETY: retained in `create`, released only in `destroy`.
    let view = unsafe { &*(view as *const NSView) };
    if let Some(sup) = unsafe { view.superview() } {
        view.setFrame(frame(&sup, x, y, w, h));
    }
}

/// Slide the view into its (already set) frame from `dx` points to the left of it, over `ms`,
/// on the compositor: the model is at rest at once, only the presentation travels. Matches the
/// page's CSS slide so the terminal moves with its pane instead of being hidden for it.
pub fn slide_from(view: usize, dx: f64, ms: f64) {
    // SAFETY: retained in `create`, released only in `destroy`.
    let view = unsafe { &*(view as *const NSView) };
    let Some(layer) = view.layer() else {
        return;
    };
    // SAFETY: CoreAnimation class methods and setters with matching argument types; the
    // animation is retained by the layer once added.
    unsafe {
        let key = objc2_foundation::NSString::from_str("transform.translation.x");
        let anim: *mut NSObject =
            msg_send![objc2::class!(CABasicAnimation), animationWithKeyPath: &*key];
        let from = objc2_foundation::NSNumber::new_f64(-dx);
        let to = objc2_foundation::NSNumber::new_f64(0.0);
        let _: () = msg_send![anim, setFromValue: &*from];
        let _: () = msg_send![anim, setToValue: &*to];
        let _: () = msg_send![anim, setDuration: ms / 1000.0];
        // CoreAnimation's "easeOut" = cubic-bezier(0, 0, 0.58, 1), the page slide's CSS `ease-out`.
        let name = objc2_foundation::NSString::from_str("easeOut");
        let timing: *mut NSObject =
            msg_send![objc2::class!(CAMediaTimingFunction), functionWithName: &*name];
        let _: () = msg_send![anim, setTimingFunction: timing];
        let _: () = msg_send![&*layer, addAnimation: anim, forKey: &*key];
    }
}

pub fn destroy(view: usize) {
    // SAFETY: balances `Retained::into_raw` in `create`.
    if let Some(view) = unsafe { Retained::from_raw(view as *mut NSView) } {
        view.removeFromSuperview();
    }
}

// ---- what `term` asks of its platform (the Linux twin is `term/linux`) ----

/// The main `NSWindow*` the views are added to.
pub fn window(app: &tauri::AppHandle) -> Result<usize, String> {
    use tauri::Manager;
    let win = app.get_window("main").ok_or("no main window")?;
    Ok(win.ns_window().map_err(|e| e.to_string())? as usize)
}

/// Backing scale factor of the main window.
pub fn scale(app: &tauri::AppHandle) -> Result<f64, String> {
    use tauri::Manager;
    let win = app.get_window("main").ok_or("no main window")?;
    win.scale_factor().map_err(|e| e.to_string())
}

/// A terminal keeps the scale it was opened at (a display change rebuilds the surface, not the font).
pub fn scale_changed(_old: f64, _new: f64) -> bool {
    false
}

/// The swapchain size, in physical pixels, of a slot of `w` x `h` points.
pub fn pixel_size(_x: f64, _y: f64, w: f64, h: f64, scale: f64) -> (u32, u32) {
    ((w * scale) as u32, (h * scale) as u32)
}

/// A renderer drawing into `view`.
///
/// # Safety
/// `view` is a live `NSView*` that outlives the renderer. Call on the main thread.
pub unsafe fn renderer(
    view: usize,
    pixels: (u32, u32),
    font_pt: f32,
    scale: f32,
    spec: atlas_term::FontSpec,
) -> Result<(atlas_term::Renderer, atlas_term::FontResolution), String> {
    let ptr = std::ptr::NonNull::new(view as *mut std::ffi::c_void).ok_or("null view")?;
    // SAFETY: the caller's contract.
    unsafe { atlas_term::Renderer::new(ptr, pixels.0, pixels.1, font_pt, scale, spec) }
}

/// The general pasteboard as a paste decision (⌘V). Read natively because the webview's paste
/// event only exposes text: a screenshot or a Finder file copy arrives as nothing.
pub fn read_clipboard() -> atlas_term::input::Clipboard {
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
        NSPasteboardTypeTIFF,
    };
    use objc2_foundation::NSURL;
    // SAFETY: the general pasteboard is safe to read off the main thread; the type constants
    // are static NSStrings.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let mut files = Vec::new();
        for item in pb.pasteboardItems().iter().flatten() {
            let Some(s) = item.stringForType(NSPasteboardTypeFileURL) else {
                continue;
            };
            if let Some(p) = NSURL::URLWithString(&s).and_then(|u| u.path()) {
                files.push(p.to_string());
            }
        }
        let text = pb
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string());
        let has_image = pb.types().is_some_and(|t| {
            t.iter().any(|t| {
                t.isEqualToString(NSPasteboardTypePNG) || t.isEqualToString(NSPasteboardTypeTIFF)
            })
        });
        atlas_term::input::Clipboard::classify(files, text, has_image)
    }
}

/// Text → the system pasteboard (⌘C, OSC 52).
pub fn write_clipboard(text: &str) {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    // SAFETY: NSPasteboard general pasteboard is safe off the main thread for plain writes;
    // `NSPasteboardTypeString` is a static NSString constant.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
    }
}

/// macOS has no PRIMARY selection: a selection is copied only when `copy-on-select` says so, and a middle click is
/// the program's. The page never asks (`pointer.ts`); these keep the shared commands compiling.
pub fn read_primary() -> Option<String> {
    None
}

pub fn write_primary(_text: &str) {}

/// Wake and display-change observers (`system.rs`).
pub use super::system::install as install_system_observers;
