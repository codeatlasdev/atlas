//! Small AppKit pieces of capture: the pasteboard, Finder, the shutter sound, thread priority.

use std::path::Path;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSSound};
use objc2_foundation::{NSData, NSString, NSURL};

/// The pasteboard name an agent instance writes to instead of the general one. An agent shares the user's login
/// session with the user: a capture it takes must never replace what the person copied.
const AGENT_BOARD: &str = "br.com.codeatlas.atlas.agent-capture";

/// `None` = the general pasteboard (the user's app); `Some(name)` = a private named one (an agent instance).
pub fn board_name(focus_allowed: bool) -> Option<&'static str> {
    (!focus_allowed).then_some(AGENT_BOARD)
}

/// The pasteboard captures of this process copy to.
pub fn pasteboard(focus_allowed: bool) -> Retained<NSPasteboard> {
    match board_name(focus_allowed) {
        None => NSPasteboard::generalPasteboard(),
        Some(n) => NSPasteboard::pasteboardWithName(&NSString::from_str(n)),
    }
}

fn board() -> Retained<NSPasteboard> {
    pasteboard(crate::agent_mode::Mode::current().focus_allowed())
}

/// The scheduler class of a capture thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qos {
    /// The person is waiting on this thread (hotkey to overlay, release to file, clipboard and card): a loaded Mac (a
    /// build, a test run) would otherwise park it behind them.
    Interactive,
    /// Everything that can take its time (history, sound, recordings, GIF encoding, text recognition).
    Default,
}

impl Qos {
    fn class(self) -> libc::qos_class_t {
        match self {
            Qos::Interactive => libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
            Qos::Default => libc::qos_class_t::QOS_CLASS_DEFAULT,
        }
    }
}

/// Set the calling thread's scheduler class. It is per thread and NOT inherited: a thread spawned from an
/// interactive one starts at the default class (measured, test below), so each helper of the instant path (the freeze
/// of a display, the clipboard write) asks for it itself, and a long-lived worker must never be left on
/// [`Qos::Interactive`].
pub fn set_qos(qos: Qos) {
    // SAFETY: plain libc call on the calling thread; the relative priority 0 is the class's own.
    unsafe {
        libc::pthread_set_qos_class_self_np(qos.class(), 0);
    }
}

/// The QoS class of the calling thread (`qos_class_t` raw value).
#[cfg(test)]
pub(super) fn current_qos() -> u32 {
    let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
    let mut rel = 0;
    // SAFETY: libc call writing two integers owned by this frame.
    unsafe { libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut rel) };
    class as u32
}

/// Put `png` on the general pasteboard as an image **and**, when it is a saved file, as that file's URL: pasting
/// into an editor, Preview or Slack's composer gets the picture, pasting into Finder or Mail gets the file.
pub fn copy_image(path: &Path) -> Result<(), String> {
    copy_image_to(&board(), path)
}

/// [`copy_image`] onto `pb`.
pub fn copy_image_to(pb: &NSPasteboard, path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    copy_png_bytes_to(pb, &bytes, path)
}

/// Put already-encoded PNG `bytes` (the file at `path`) on `pb`: the screenshot is encoded once and the same bytes
/// go to the file and the clipboard, so the clipboard does not wait for a file to be written and read back.
pub fn copy_png_bytes_to(pb: &NSPasteboard, bytes: &[u8], path: &Path) -> Result<(), String> {
    // SAFETY: NSPasteboard accepts plain writes off the main thread; the type constants are static NSStrings, the
    // data and URL objects are owned for the call.
    unsafe {
        pb.clearContents();
        let ok = pb.setData_forType(Some(&NSData::with_bytes(bytes)), NSPasteboardTypePNG);
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

/// [`copy_png_bytes_to`] on this process's pasteboard.
pub fn copy_png_bytes(bytes: &[u8], path: &Path) -> Result<(), String> {
    copy_png_bytes_to(&board(), bytes, path)
}

/// Put the file itself (a video or a GIF) on the pasteboard: pasting into Finder, Mail or Slack attaches it.
pub fn copy_file(path: &Path) -> Result<(), String> {
    copy_file_to(&board(), path)
}

/// [`copy_file`] onto `pb`.
pub fn copy_file_to(pb: &NSPasteboard, path: &Path) -> Result<(), String> {
    // SAFETY: as in `copy_png_bytes_to`.
    unsafe {
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

/// Put plain text on this process's pasteboard (the text read by "capture text").
pub fn copy_text(text: &str) -> Result<(), String> {
    let pb = board();
    // SAFETY: as in `copy_png_bytes_to`.
    unsafe {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Read back from the thread, not from our own bookkeeping: each class sticks, and it can go back down (a worker
    /// is Interactive for the instant path only and must not stay there for history, sound or a recording).
    #[test]
    fn a_thread_reads_back_the_qos_it_asked_for_and_can_come_down_again() {
        let (up, down) = std::thread::spawn(|| {
            set_qos(Qos::Interactive);
            let up = current_qos();
            set_qos(Qos::Default);
            (up, current_qos())
        })
        .join()
        .unwrap();
        assert_eq!(up, libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE as u32);
        assert_eq!(down, libc::qos_class_t::QOS_CLASS_DEFAULT as u32);
    }

    /// The helpers of the instant path (a freeze per display, the clipboard write) are threads of their own: they do
    /// not get their spawner's class, so they set it themselves. A helper that forgot would silently run at default.
    #[test]
    fn a_spawned_helper_does_not_inherit_the_interactive_class() {
        let child = std::thread::spawn(|| {
            set_qos(Qos::Interactive);
            std::thread::spawn(current_qos).join().unwrap()
        })
        .join()
        .unwrap();
        assert_ne!(child, libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE as u32);
    }

    fn unique_board() -> objc2::rc::Retained<NSPasteboard> {
        NSPasteboard::pasteboardWithUniqueName()
    }

    fn png_fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas-mac-test-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("shot.png");
        let png = super::super::pixels::encode_png(&[10, 20, 30, 255], 1, 1).unwrap();
        std::fs::write(&p, png).unwrap();
        p
    }

    /// A screenshot lands on the pasteboard as a PNG and as its file URL, read back from the pasteboard itself.
    #[test]
    fn a_copied_screenshot_is_a_png_and_a_file_url_on_the_pasteboard() {
        let p = png_fixture("img");
        let board = unique_board();
        copy_image_to(&board, &p).unwrap();
        // SAFETY: reading the types of a pasteboard owned by this test.
        let (png, url) = unsafe {
            (
                board.dataForType(NSPasteboardTypePNG),
                board.stringForType(NSPasteboardTypeFileURL),
            )
        };
        let png = png.expect("a PNG is on the pasteboard");
        assert_eq!(png.to_vec(), std::fs::read(&p).unwrap());
        assert!(url.expect("a file URL").to_string().ends_with("shot.png"));
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A recording or a GIF is the file URL only.
    #[test]
    fn a_copied_file_is_only_its_url_on_the_pasteboard() {
        let p = png_fixture("file");
        let board = unique_board();
        copy_file_to(&board, &p).unwrap();
        // SAFETY: as above.
        let (png, url) = unsafe {
            (
                board.dataForType(NSPasteboardTypePNG),
                board.stringForType(NSPasteboardTypeFileURL),
            )
        };
        assert!(png.is_none());
        assert!(url.is_some());
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// An agent instance shares the user's pasteboard with the user: it writes to a private one of its own, so a
    /// capture it takes never replaces what the person copied.
    #[test]
    fn an_agent_instance_never_writes_the_general_pasteboard() {
        assert_eq!(
            board_name(true),
            None,
            "the user's app copies to the general pasteboard"
        );
        let n = board_name(false).expect("an agent instance has its own pasteboard");
        assert!(!n.is_empty());
        // the private board really is another pasteboard than the general one
        let own = pasteboard(false);
        let general = NSPasteboard::generalPasteboard();
        assert_ne!(own.name().to_string(), general.name().to_string());
    }
}
