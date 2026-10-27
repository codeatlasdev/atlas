//! Small AppKit pieces of capture: the pasteboard, Finder, the shutter sound.

use std::path::Path;

use objc2::AnyThread;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSSound};
use objc2_foundation::{NSData, NSString, NSURL};

/// Put `png` on the general pasteboard as an image **and**, when it is a saved file, as that file's URL: pasting
/// into an editor, Preview or Slack's composer gets the picture, pasting into Finder or Mail gets the file.
pub fn copy_image(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    // SAFETY: NSPasteboard's general pasteboard accepts plain writes off the main thread; the type constants are
    // static NSStrings, the data and URL objects are owned for the call.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let ok = pb.setData_forType(Some(&NSData::with_bytes(&bytes)), NSPasteboardTypePNG);
        if !ok {
            return Err("the clipboard did not accept the image".into());
        }
        if let Some(url) =
            NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())).absoluteString()
        {
            pb.setString_forType(&url, NSPasteboardTypeFileURL);
        }
    }
    Ok(())
}

/// Put the file itself (a video or a GIF) on the pasteboard: pasting into Finder, Mail or Slack attaches it.
pub fn copy_file(path: &Path) -> Result<(), String> {
    // SAFETY: as in `copy_image`.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
            .absoluteString()
            .ok_or("the file has no URL")?;
        if !pb.setString_forType(&url, NSPasteboardTypeFileURL) {
            return Err("the clipboard did not accept the file".into());
        }
    }
    Ok(())
}

/// Put plain text on the general pasteboard (the text read by "capture text").
pub fn copy_text(text: &str) -> Result<(), String> {
    // SAFETY: as in `copy_image`.
    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        if !pb.setString_forType(
            &NSString::from_str(text),
            objc2_app_kit::NSPasteboardTypeString,
        ) {
            return Err("the clipboard did not accept the text".into());
        }
    }
    Ok(())
}

/// Reveal `path` in Finder (selected), without bringing Atlas forward.
pub fn reveal(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("open: {e}"))
}

/// Open `path` with its default app.
pub fn open(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("open: {e}"))
}

/// Open a URL (a System Settings pane) with its handler.
pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("open: {e}"))
}

/// Keeps the playing sound alive (held, never read).
struct SendSound(#[allow(dead_code)] objc2::rc::Retained<NSSound>);
// SAFETY: only touched under the mutex, and only to be dropped.
unsafe impl Send for SendSound {}
static LAST_SOUND: parking_lot::Mutex<Option<SendSound>> = parking_lot::Mutex::new(None);

/// The system screenshot sound, if the file exists on this macOS; silently nothing otherwise (a missing sound is
/// not an error worth a message).
pub fn shutter() {
    const SOUND: &str = "/System/Library/Components/CoreAudio.component/Contents/SharedSupport/SystemSounds/system/Screen Capture.aif";
    if !Path::new(SOUND).exists() {
        return;
    }
    // An NSSound stops when released, so the latest one is kept alive in a slot until the next capture replaces it.
    if let Some(s) = NSSound::initWithContentsOfFile_byReference(
        NSSound::alloc(),
        &NSString::from_str(SOUND),
        true,
    ) {
        s.play();
        *LAST_SOUND.lock() = Some(SendSound(s));
    }
}

/// The pointer in global points, top-left origin of the primary display, y down (AppKit's `mouseLocation` is
/// bottom-left).
pub fn cursor_point() -> (f64, f64) {
    // SAFETY: plain class method on NSEvent, callable from any thread.
    let p: objc2_foundation::NSPoint =
        unsafe { objc2::msg_send![objc2::class!(NSEvent), mouseLocation] };
    (p.x, crate::display_mac::primary_height() - p.y)
}

/// The drag source of a file drag-out: copies only (the original stays in the capture folder).
mod drag_source {
    use std::sync::LazyLock;

    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
    use objc2::sel;

    extern "C" fn mask(
        _this: &AnyObject,
        _sel: Sel,
        _session: *mut AnyObject,
        _ctx: isize,
    ) -> usize {
        // NSDragOperationCopy
        1
    }

    pub static CLASS: LazyLock<&'static AnyClass> = LazyLock::new(|| {
        let mut b = ClassBuilder::new(c"AtlasShotsDragSource", objc2::class!(NSObject))
            .expect("AtlasShotsDragSource is registered once");
        // SAFETY: matches `-draggingSession:sourceOperationMaskForDraggingContext:` (`NSDragOperation (id, SEL, id, NSInteger)`).
        unsafe {
            b.add_method(
                sel!(draggingSession:sourceOperationMaskForDraggingContext:),
                mask as extern "C" fn(_, _, _, _) -> _,
            );
        }
        b.register()
    });
}

/// Start a native file drag of `path` from `view` (the webview's `NSView*` as an address, main thread only), using
/// the mouse-down that is current: dropping into Finder, Mail, Slack or a browser upload attaches the file. The
/// page calls this from its `dragstart`, so the event is the press that began the drag.
///
/// # Safety
/// `view` must be a live `NSView` on this (the main) thread.
pub unsafe fn drag_file(view: *mut AnyObjectRaw, path: &Path) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize};

    let view = view.cast::<AnyObject>();
    // SAFETY: caller contract. Objects are created with `alloc`/`new` (+1) and released at the end; AppKit retains
    // what the drag session needs for its lifetime.
    unsafe {
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        let event: *mut AnyObject = msg_send![app, currentEvent];
        if event.is_null() {
            return Err("There is no mouse press to start a drag from.".into());
        }
        let nspath = objc2_foundation::NSString::from_str(&path.to_string_lossy());
        let url = objc2_foundation::NSURL::fileURLWithPath(&nspath);
        let item: *mut AnyObject = msg_send![objc2::class!(NSDraggingItem), alloc];
        let item: *mut AnyObject = msg_send![item, initWithPasteboardWriter: &*url];
        // The drag image: the file's icon, 64 pt, centred under the pointer.
        let ws: *mut AnyObject = msg_send![objc2::class!(NSWorkspace), sharedWorkspace];
        let icon: *mut AnyObject = msg_send![ws, iconForFile: &*nspath];
        let at: NSPoint = msg_send![event, locationInWindow];
        let at: NSPoint =
            msg_send![view, convertPoint: at, fromView: std::ptr::null::<AnyObject>()];
        let frame = NSRect::new(
            NSPoint::new(at.x - 32.0, at.y - 32.0),
            NSSize::new(64.0, 64.0),
        );
        let _: () = msg_send![item, setDraggingFrame: frame, contents: icon];
        let source: *mut AnyObject = msg_send![*drag_source::CLASS, new];
        let items =
            NSArray::from_retained_slice(&[Retained::from_raw(item).ok_or("no drag item")?]);
        let _: *mut AnyObject = msg_send![
            view,
            beginDraggingSessionWithItems: &*items,
            event: event,
            source: source
        ];
        // The session retains the source; this is our +1.
        let _: () = msg_send![source, release];
    }
    Ok(())
}

/// `NSView` as the opaque pointer the Tauri webview handle gives.
pub type AnyObjectRaw = std::ffi::c_void;
