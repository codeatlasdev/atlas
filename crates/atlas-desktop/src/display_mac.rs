//! A private virtual display for agent instances (`agent_mode.rs`): a window placed on it is invisible on the user's
//! monitors and Spaces, never occluded by his windows (so WebKit keeps the page at full rate), and capturable by
//! ScreenCaptureKit. It uses `CGVirtualDisplay`, a private CoreGraphics API (the one DeskPad and BetterDisplay use),
//! reached through the Objective-C runtime so a macOS that drops it fails with a message instead of not linking.
//!
//! Verified on macOS 26 with the real API (see docs/desktop.md): creating one leaves the user's display bounds, main
//! display, menu bar, Dock and Spaces unchanged, does not mirror, and it disappears with the process (the display is
//! owned by the `CGVirtualDisplay` object; `Drop`, a clean exit and a crash all release it).
//!
//! A retina display needs a HiDPI *mode*: the descriptor/mode list carries the pixel size, and the mode whose
//! logical size is half of it must be selected (`CGConfigureDisplayWithDisplayMode`), otherwise the display is 1x.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSNumber, NSString};

use crate::agent_mode::{AGENT_VENDOR, ActiveDisplay, ModeSize, Rect, virtual_origin};

/// CoreGraphics registers a virtual display's modes shortly after its first configuration completes (measured: the
/// list was empty on an agent start right after `place`, populated on others). Ask again, briefly, instead of giving
/// up on the first empty answer: `poll` is called until it returns `Some` or `attempts` are used, `pause` between.
pub fn poll_until<T>(
    attempts: u32,
    mut pause: impl FnMut(u32),
    mut poll: impl FnMut() -> Option<T>,
) -> Option<T> {
    for i in 0..attempts {
        if let Some(v) = poll() {
            return Some(v);
        }
        if i + 1 < attempts {
            pause(i);
        }
    }
    None
}

/// Display configuration is global to the machine: two agent instances starting together reconfigure it at the
/// same time and CoreGraphics then lists no modes for one of them. Holding this exclusive `flock` while a display
/// is created and placed makes concurrent starts take turns. Released on drop (and by the OS if the process dies).
pub struct ConfigLock {
    /// Held for its `Drop`: closing the descriptor releases the lock.
    _file: std::fs::File,
}

impl ConfigLock {
    pub fn acquire(path: &std::path::Path) -> std::io::Result<Self> {
        use std::os::fd::AsRawFd;
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        // SAFETY: a valid open fd; LOCK_EX blocks until the other instance's display is configured.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { _file: f })
    }
}

/// What `Display` needs from the OS; faked in tests.
pub trait Backend {
    /// Create the display, `pixels` large, 2x HiDPI; returns its `CGDirectDisplayID`.
    fn create(&mut self, pixels: (u32, u32), hidpi: bool) -> Result<u32, String>;
    /// Bounds of every active display that is the user's, top-left origin.
    fn real_bounds(&self) -> Vec<Rect>;
    /// Bounds of the virtual displays other agent instances own (active, but never the user's).
    fn sibling_bounds(&self) -> Vec<Rect>;
    /// Move display `id` so its top-left is `(x, y)` in global coordinates.
    fn place(&mut self, id: u32, x: f64, y: f64) -> Result<(), String>;
    /// Select the 2x HiDPI mode (logical size = half the pixels). CoreGraphics lists no modes for a virtual display
    /// until its first configuration (`place`), so this MUST come after `place` (measured: before it the mode list
    /// is empty and there is no current mode).
    fn select_retina(&mut self, id: u32, pixels: (u32, u32)) -> Result<(), String>;
    /// Bounds the OS actually gave display `id`.
    fn bounds(&self, id: u32) -> Rect;
    fn destroy(&mut self, id: u32);
}

/// A live virtual display; dropping it destroys the display.
pub struct Display<B: Backend> {
    backend: B,
    pub id: u32,
    /// Where the OS put it (global points, top-left origin).
    pub bounds: Rect,
}

/// What `select_retina` reports when CoreGraphics never listed a mode: the one failure that a fresh display can fix.
pub const NO_MODES: &str = "the virtual display has no modes (waited 10 s after placing it)";

/// Displays tried before giving up on a start whose new display comes up without modes.
pub const CREATE_ATTEMPTS: u32 = 3;

impl<B: Backend> Display<B> {
    /// `logical` = size in points; `scale` = 1 (standard) or 2 (retina).
    pub fn create(mut backend: B, logical: (u32, u32), scale: u32) -> Result<Self, String> {
        let mut last = String::new();
        for _ in 0..CREATE_ATTEMPTS {
            match Self::try_create(&mut backend, logical, scale) {
                Ok((id, bounds)) => {
                    return Ok(Self {
                        backend,
                        id,
                        bounds,
                    });
                }
                // A display that came up without modes is destroyed by `try_create`; a new one often has them.
                Err(e) if e == NO_MODES => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// One display, end to end. On any error the display it made is already destroyed.
    fn try_create(backend: &mut B, logical: (u32, u32), scale: u32) -> Result<(u32, Rect), String> {
        let real = backend.real_bounds();
        let pixels = crate::agent_mode::backing_pixels(logical, scale);
        let id = backend.create(pixels, scale == 2)?;
        // Right of everything on screen, other agents' displays included, so instances never share a spot.
        let all: Vec<Rect> = real
            .iter()
            .chain(&backend.sibling_bounds())
            .copied()
            .collect();
        let (x, y) = virtual_origin(&all);
        // Placing configures the display (this is what makes CoreGraphics list its modes), then the retina mode
        // (a 1x display is already in its only mode).
        if let Err(e) = backend.place(id, x, y).and_then(|()| {
            if scale == 2 {
                backend.select_retina(id, pixels)
            } else {
                Ok(())
            }
        }) {
            backend.destroy(id);
            return Err(e);
        }
        let bounds = backend.bounds(id);
        if real.iter().any(|r| r.intersects(&bounds)) {
            // never leave a display that overlaps one of the user's: his layout would be rearranged
            backend.destroy(id);
            return Err(format!(
                "the virtual display landed on a real one ({bounds:?}); refusing to use it"
            ));
        }
        Ok((id, bounds))
    }
}

impl<B: Backend> Drop for Display<B> {
    fn drop(&mut self) {
        self.backend.destroy(self.id);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct Log {
        created: Vec<(u32, u32)>,
        placed: Vec<(u32, f64, f64)>,
        /// Order of backend calls, to pin that the mode is chosen only after the first placement.
        calls: Vec<&'static str>,
        destroyed: Vec<u32>,
    }

    struct Fake {
        log: Rc<RefCell<Log>>,
        real: Vec<Rect>,
        /// Other agents' virtual displays: active, but not the user's.
        siblings: Vec<Rect>,
        /// Where the OS puts the new display regardless of the request (to test the overlap guard).
        lands: Option<Rect>,
        fail_place: bool,
        fail_retina: bool,
        /// The first N `select_retina` calls fail with "no modes" (the flake), then it works.
        retina_flakes: u32,
    }

    impl Backend for Fake {
        fn create(&mut self, pixels: (u32, u32), _hidpi: bool) -> Result<u32, String> {
            self.log.borrow_mut().created.push(pixels);
            self.log.borrow_mut().calls.push("create");
            Ok(7)
        }
        fn select_retina(&mut self, _: u32, _: (u32, u32)) -> Result<(), String> {
            self.log.borrow_mut().calls.push("select_retina");
            if self.fail_retina {
                return Err("no HiDPI mode".into());
            }
            if self.retina_flakes > 0 {
                self.retina_flakes -= 1;
                return Err(NO_MODES.into());
            }
            Ok(())
        }
        fn real_bounds(&self) -> Vec<Rect> {
            self.real.clone()
        }
        fn sibling_bounds(&self) -> Vec<Rect> {
            self.siblings.clone()
        }
        fn place(&mut self, id: u32, x: f64, y: f64) -> Result<(), String> {
            if self.fail_place {
                return Err("refused".into());
            }
            self.log.borrow_mut().placed.push((id, x, y));
            self.log.borrow_mut().calls.push("place");
            Ok(())
        }
        fn bounds(&self, _: u32) -> Rect {
            let p = *self.log.borrow().placed.last().unwrap();
            self.lands.unwrap_or(Rect {
                x: p.1,
                y: p.2,
                w: 1280.0,
                h: 820.0,
            })
        }
        fn destroy(&mut self, id: u32) {
            self.log.borrow_mut().destroyed.push(id);
        }
    }

    /// Another agent's virtual display, already sitting right of the user's panel.
    const SIBLING: Rect = Rect {
        x: 1352.0,
        y: 58.0,
        w: 1280.0,
        h: 820.0,
    };

    /// Incident (two agent instances at once): the second `desktop:agent` failed with "the virtual display landed
    /// on a real one" because the first instance's own virtual display counted as one of the user's. Sibling agent
    /// displays must be stacked beside, never mistaken for the user's, and never block each other.
    /// Incident (two instances at once): both reconfigured the displays together and one got an empty mode list.
    #[test]
    fn the_config_lock_makes_two_holders_take_turns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("display.lock");
        let first = ConfigLock::acquire(&path).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let p2 = path.clone();
        let t = std::thread::spawn(move || {
            let _second = ConfigLock::acquire(&p2).unwrap(); // blocks until `first` drops
            tx.send(()).unwrap();
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(300))
                .is_err(),
            "the second holder must wait while the first holds the lock"
        );
        drop(first);
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("the second holder proceeds once the first lets go");
        t.join().unwrap();
    }

    #[test]
    fn a_second_agent_display_goes_right_of_the_first_and_is_not_refused() {
        let (mut b, log) = fake(None, false);
        b.siblings = vec![SIBLING];
        let d = Display::create(b, (1280, 820), 2)
            .expect("a sibling agent display must not block this one");
        assert_eq!(
            d.bounds.x, 2632.0,
            "right of the user's panel AND the sibling"
        );
        assert!(log.borrow().destroyed.is_empty());
    }

    #[test]
    fn a_display_that_lands_on_a_sibling_agent_display_is_still_fine_but_one_on_the_users_is_refused()
     {
        // Landing on the sibling's rect: harmless for the user (it only shares an agent-only display).
        let (mut b, _) = fake(Some(SIBLING), false);
        b.siblings = vec![SIBLING];
        assert!(Display::create(b, (1280, 820), 2).is_ok());
        // Landing on the built-in panel is what the guard exists for.
        let (mut b, log) = fake(Some(BUILTIN), false);
        b.siblings = vec![SIBLING];
        assert!(Display::create(b, (1280, 820), 2).is_err());
        assert_eq!(log.borrow().destroyed, [7]);
    }

    const BUILTIN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1352.0,
        h: 878.0,
    };

    fn fake(lands: Option<Rect>, fail_place: bool) -> (Fake, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log::default()));
        (
            Fake {
                log: log.clone(),
                real: vec![BUILTIN],
                siblings: vec![],
                lands,
                fail_place,
                fail_retina: false,
                retina_flakes: 0,
            },
            log,
        )
    }

    #[test]
    fn creates_a_retina_display_right_of_the_users_and_destroys_it_on_drop() {
        let (b, log) = fake(None, false);
        let d = Display::create(b, (1280, 820), 2).unwrap();
        assert_eq!(d.bounds.x, 1352.0, "right of the built-in display");
        assert_eq!(log.borrow().created, [(2560, 1640)], "2x backing pixels");
        assert!(log.borrow().destroyed.is_empty(), "alive while held");
        drop(d);
        assert_eq!(
            log.borrow().destroyed,
            [7],
            "destroyed exactly once, on drop"
        );
    }

    #[test]
    fn a_standard_display_has_1x_pixels_and_never_asks_for_a_retina_mode() {
        let (b, log) = fake(None, false);
        let _d = Display::create(b, (1280, 820), 1).unwrap();
        assert_eq!(log.borrow().created, [(1280, 820)]);
        assert_eq!(log.borrow().calls, ["create", "place"]);
    }

    #[test]
    fn a_display_that_lands_on_a_real_one_is_destroyed_and_refused() {
        let (b, log) = fake(Some(BUILTIN), false);
        let e = Display::create(b, (1280, 820), 2).err().expect("refused");
        assert!(e.contains("landed on a real one"), "{e}");
        assert_eq!(
            log.borrow().destroyed,
            [7],
            "no orphan display is left behind"
        );
    }

    /// Incident (first agent run): the display was created but "had no modes", so the window never showed.
    /// CoreGraphics lists a virtual display's modes only after its first configuration, i.e. after `place`.
    #[test]
    fn polling_returns_the_first_answer_and_pauses_only_between_empty_ones() {
        let mut pauses = 0;
        let mut calls = 0;
        let got = poll_until(
            10,
            |_| pauses += 1,
            || {
                calls += 1;
                (calls == 3).then_some("modes")
            },
        );
        assert_eq!((got, calls, pauses), (Some("modes"), 3, 2));
        // never answers: bounded, no pause after the last attempt
        let (mut calls, mut pauses) = (0, 0);
        assert_eq!(
            poll_until(
                4,
                |_| pauses += 1,
                || {
                    calls += 1;
                    None::<u8>
                }
            ),
            None
        );
        assert_eq!((calls, pauses), (4, 3));
        // first try wins without any pause
        let mut pauses = 0;
        assert_eq!(poll_until(4, |_| pauses += 1, || Some(1)), Some(1));
        assert_eq!(pauses, 0);
    }

    #[test]
    fn the_retina_mode_is_chosen_after_the_display_is_placed_never_before() {
        let (b, log) = fake(None, false);
        let _d = Display::create(b, (1280, 820), 2).unwrap();
        assert_eq!(log.borrow().calls, ["create", "place", "select_retina"]);
    }

    #[test]
    fn a_display_without_a_retina_mode_is_destroyed_not_used_at_1x() {
        let (mut b, log) = fake(None, false);
        b.fail_retina = true;
        let e = Display::create(b, (1280, 820), 2).err().expect("refused");
        assert!(e.contains("HiDPI"), "{e}");
        assert_eq!(log.borrow().destroyed, [7]);
    }

    /// Incident (a plain sequential start, no concurrency): CoreGraphics listed no modes for the new display and the
    /// instance refused to show ("no modes, waited 10 s"). A display with no modes is unusable but not a reason to
    /// give up: it is destroyed and a fresh one created, a bounded number of times.
    #[test]
    fn a_display_that_comes_up_without_modes_is_destroyed_and_recreated() {
        let (mut b, log) = fake(None, false);
        b.retina_flakes = 2;
        let d = Display::create(b, (1280, 820), 2).expect("the third display works");
        assert_eq!(d.bounds.x, 1352.0);
        assert_eq!(log.borrow().created.len(), 3, "two failures, one success");
        assert_eq!(
            log.borrow().destroyed,
            [7, 7],
            "each modeless display is destroyed before the next is made"
        );
    }

    #[test]
    fn recreating_is_bounded_and_the_last_error_is_reported() {
        let (mut b, log) = fake(None, false);
        b.retina_flakes = 100;
        let e = Display::create(b, (1280, 820), 2).err().expect("gives up");
        assert!(e.contains("no modes"), "{e}");
        assert_eq!(log.borrow().created.len() as u32, CREATE_ATTEMPTS);
        assert_eq!(
            log.borrow().destroyed.len() as u32,
            CREATE_ATTEMPTS,
            "no orphan"
        );
    }

    /// Only the mode flake is retried: a refused placement or a 1x-only display is a decision, not a hiccup.
    #[test]
    fn other_failures_are_not_retried() {
        let (b, log) = fake(None, true);
        assert!(Display::create(b, (1280, 820), 2).is_err());
        assert_eq!(log.borrow().created.len(), 1);
    }

    #[test]
    fn a_failed_placement_does_not_leak_the_display() {
        let (b, log) = fake(None, true);
        assert!(Display::create(b, (1280, 820), 2).is_err());
        assert_eq!(log.borrow().destroyed, [7]);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The real backend: CGVirtualDisplay through the Objective-C runtime + CoreGraphics display configuration.

#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, out: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(id: u32) -> CGRect;
    fn CGDisplayVendorNumber(id: u32) -> u32;
    fn CGBeginDisplayConfiguration(cfg: *mut *mut c_void) -> i32;
    fn CGConfigureDisplayOrigin(cfg: *mut c_void, id: u32, x: i32, y: i32) -> i32;
    fn CGConfigureDisplayWithDisplayMode(
        cfg: *mut c_void,
        id: u32,
        mode: *const c_void,
        options: *const c_void,
    ) -> i32;
    fn CGCompleteDisplayConfiguration(cfg: *mut c_void, option: u32) -> i32;
    fn CGDisplayCopyAllDisplayModes(id: u32, options: *const c_void) -> *const c_void;
    fn CGDisplayModeGetWidth(mode: *const c_void) -> usize;
    fn CGDisplayModeGetHeight(mode: *const c_void) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: *const c_void) -> usize;
    fn CGDisplayCopyDisplayMode(id: u32) -> *const c_void;
    fn CGDisplayModeRelease(mode: *const c_void);
    fn CGDisplayModeGetPixelWidth(mode: *const c_void) -> usize;
    static kCGDisplayShowDuplicateLowResolutionModes: *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(a: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(a: *const c_void, i: isize) -> *const c_void;
    fn CFRelease(o: *const c_void);
    fn CFDictionaryCreate(
        alloc: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        n: isize,
        key_cb: *const c_void,
        value_cb: *const c_void,
    ) -> *const c_void;
    static kCFBooleanTrue: *const c_void;
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source_handled: u8) -> i32;
    static kCFRunLoopDefaultMode: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

/// `kCGConfigureForSession`: the change dies with the login session and is never written to the user's preferences.
const FOR_SESSION: u32 = 1;

pub struct Cg {
    /// The retained `CGVirtualDisplay`; releasing it removes the display.
    display: Option<Retained<AnyObject>>,
}

// SAFETY: the `CGVirtualDisplay` is created, queried and released through `&mut Cg` / `&Cg` behind the single mutex
// in `agent_display.rs`; it is never messaged concurrently (and the object itself is thread-agnostic).
unsafe impl Send for Cg {}

impl Cg {
    pub fn new() -> Self {
        Self { display: None }
    }
}

/// Every active display as the OS reports it right now: id, bounds and vendor. Nothing is filtered out, so a
/// display can never be dropped from a safety check by being misclassified.
pub fn active_displays() -> Vec<ActiveDisplay> {
    let mut n = 0u32;
    // SAFETY: CoreGraphics C API with correctly sized buffers.
    unsafe {
        CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut n);
        let mut ids = vec![0u32; n as usize];
        CGGetActiveDisplayList(n, ids.as_mut_ptr(), &mut n);
        ids.truncate(n as usize);
        ids.into_iter()
            .map(|id| ActiveDisplay {
                id,
                bounds: rect_of(CGDisplayBounds(id)),
                vendor: CGDisplayVendorNumber(id),
            })
            .collect()
    }
}

/// The current mode of display `id` in points and pixels; `None` when it has none (or `id` is not a display).
pub fn display_mode(id: u32) -> Option<ModeSize> {
    // SAFETY: CoreGraphics C API; the copied mode is released.
    unsafe {
        let m = CGDisplayCopyDisplayMode(id);
        if m.is_null() {
            return None;
        }
        let out = ModeSize {
            points: (
                CGDisplayModeGetWidth(m) as f64,
                CGDisplayModeGetHeight(m) as f64,
            ),
            pixels: (
                CGDisplayModeGetPixelWidth(m) as f64,
                CGDisplayModeGetPixelHeight(m) as f64,
            ),
        };
        CGDisplayModeRelease(m);
        Some(out)
    }
}

/// Height in points of the primary display: the origin of Cocoa's bottom-left coordinate system.
pub fn primary_height() -> f64 {
    // SAFETY: plain getters; display 0 is not valid, CGMainDisplayID is.
    unsafe {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGMainDisplayID() -> u32;
        }
        CGDisplayBounds(CGMainDisplayID()).size.height
    }
}

fn class(name: &str) -> Result<&'static AnyClass, String> {
    AnyClass::get(&std::ffi::CString::new(name).unwrap()).ok_or_else(|| {
        format!("{name} does not exist on this macOS: the virtual display needs a different approach here")
    })
}

fn kvc(obj: &AnyObject, key: &str, value: &AnyObject) {
    let key = NSString::from_str(key);
    // SAFETY: setValue:forKey: on an NSObject subclass; the keys are the properties listed by the runtime.
    unsafe {
        let _: () = msg_send![obj, setValue: value, forKey: &*key];
    }
}

fn number(n: u32) -> Retained<NSNumber> {
    NSNumber::new_u32(n)
}

impl Cg {
    /// Active displays other than ours: the agent-owned virtual ones (vendor `AGENT_VENDOR`) when `agents`, else
    /// the user's.
    fn active(&self, agents: bool) -> Vec<Rect> {
        let own = self.display.as_ref().map(|d| {
            // SAFETY: plain getter.
            unsafe { msg_send![&**d, displayID] }
        });
        let mut n = 0u32;
        // SAFETY: CoreGraphics C API with correctly sized buffers.
        unsafe {
            CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut n);
            let mut ids = vec![0u32; n as usize];
            CGGetActiveDisplayList(n, ids.as_mut_ptr(), &mut n);
            ids.truncate(n as usize);
            ids.into_iter()
                .filter(|id| Some(*id) != own)
                .filter(|id| (CGDisplayVendorNumber(*id) == AGENT_VENDOR) == agents)
                .map(|id| rect_of(CGDisplayBounds(id)))
                .collect()
        }
    }
}

impl Backend for Cg {
    fn create(&mut self, pixels: (u32, u32), hidpi: bool) -> Result<u32, String> {
        let (w, h) = pixels;
        // SAFETY: the Objective-C protocol of CGVirtualDisplay as used by DeskPad/BetterDisplay; every selector is
        // checked with `responds_to` before it is sent, and a missing class is an Err.
        unsafe {
            let desc_cls = class("CGVirtualDisplayDescriptor")?;
            let disp_cls = class("CGVirtualDisplay")?;
            let set_cls = class("CGVirtualDisplaySettings")?;
            let mode_cls = class("CGVirtualDisplayMode")?;
            // Every selector we send must exist; the first missing one is named in the error.
            for (cls, sel) in [
                (disp_cls, sel!(initWithDescriptor:)),
                (disp_cls, sel!(applySettings:)),
                (disp_cls, sel!(displayID)),
                (mode_cls, sel!(initWithWidth:height:refreshRate:)),
            ] {
                if cls.instance_method(sel).is_none() {
                    return Err(format!(
                        "CGVirtualDisplay has a different shape on this macOS: {} has no {sel}",
                        cls.name().to_string_lossy()
                    ));
                }
            }
            let desc: Retained<AnyObject> = msg_send![msg_send![desc_cls, alloc], init];
            kvc(
                &desc,
                "name",
                &NSString::from_str("Atlas Agent Display") as &AnyObject,
            );
            kvc(&desc, "vendorID", &number(AGENT_VENDOR));
            kvc(&desc, "productID", &number(0x0001));
            kvc(&desc, "serialNum", &number(std::process::id()));
            kvc(&desc, "maxPixelsWide", &number(w));
            kvc(&desc, "maxPixelsHigh", &number(h));
            let q: *mut AnyObject = msg_send![objc2::class!(NSObject), new];
            let _ = q; // the descriptor's `queue` defaults to the main queue when unset
            let disp: Option<Retained<AnyObject>> =
                msg_send![msg_send![disp_cls, alloc], initWithDescriptor: &*desc];
            let disp = disp.ok_or("CGVirtualDisplay refused the descriptor")?;

            let settings: Retained<AnyObject> = msg_send![msg_send![set_cls, alloc], init];
            kvc(&settings, "hiDPI", &number(u32::from(hidpi)));
            let full: Retained<AnyObject> = msg_send![msg_send![mode_cls, alloc],
                initWithWidth: w, height: h, refreshRate: 60.0f64];
            let half: Retained<AnyObject> = msg_send![msg_send![mode_cls, alloc],
                initWithWidth: w / 2, height: h / 2, refreshRate: 60.0f64];
            let modes = if hidpi {
                objc2_foundation::NSArray::from_retained_slice(&[full, half])
            } else {
                objc2_foundation::NSArray::from_retained_slice(&[full])
            };
            kvc(&settings, "modes", &modes as &AnyObject);
            let ok: bool = msg_send![&*disp, applySettings: &*settings];
            if !ok {
                return Err("CGVirtualDisplay refused its settings".into());
            }
            let id: u32 = msg_send![&*disp, displayID];
            self.display = Some(disp);
            Ok(id)
        }
    }

    fn real_bounds(&self) -> Vec<Rect> {
        self.active(false)
    }

    fn sibling_bounds(&self) -> Vec<Rect> {
        self.active(true)
    }

    fn place(&mut self, id: u32, x: f64, y: f64) -> Result<(), String> {
        let mut cfg: *mut c_void = std::ptr::null_mut();
        // SAFETY: begin/complete configuration pair; the session option keeps it out of the user's preferences.
        unsafe {
            if CGBeginDisplayConfiguration(&mut cfg) != 0 {
                return Err("could not begin a display configuration".into());
            }
            CGConfigureDisplayOrigin(cfg, id, x as i32, y as i32);
            if CGCompleteDisplayConfiguration(cfg, FOR_SESSION) != 0 {
                return Err("macOS refused to place the virtual display".into());
            }
        }
        Ok(())
    }

    fn select_retina(&mut self, id: u32, pixels: (u32, u32)) -> Result<(), String> {
        select_hidpi_mode(id, pixels.0)
    }

    fn bounds(&self, id: u32) -> Rect {
        // SAFETY: plain getter.
        rect_of(unsafe { CGDisplayBounds(id) })
    }

    fn destroy(&mut self, _id: u32) {
        // Releasing the CGVirtualDisplay removes the display.
        self.display = None;
    }
}

/// Wait `seconds` the way a UI thread must: by servicing its run loop. CoreGraphics refreshes a process's view of a
/// display it just created (its mode list, its current mode) from run loop sources, so a thread that sleeps instead
/// sees `CGDisplayCopyAllDisplayModes` return NULL for as long as it sleeps (measured on macOS 27 inside the app's
/// main thread: NULL for 10 s while WindowServer listed 24 modes; non-NULL after one 50 ms run loop turn).
fn service_run_loop(seconds: f64) {
    // SAFETY: plain CoreFoundation call on the current thread's run loop with the framework's own default mode.
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, 0);
    }
}

fn rect_of(r: CGRect) -> Rect {
    Rect {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.width,
        h: r.size.height,
    }
}

/// Select the mode whose logical width is half of `pixel_w` and whose backing is `pixel_w` (the HiDPI one).
fn select_hidpi_mode(id: u32, pixel_w: u32) -> Result<(), String> {
    // SAFETY: CoreGraphics C API; the mode array is released after use.
    unsafe {
        // Without `kCGDisplayShowDuplicateLowResolutionModes` the list has no HiDPI mode (measured: 13 modes, none
        // with pixels = 2 x points): it shares its logical size with the 1x one and is hidden as a duplicate. The
        // options are a real CFDictionary built from the framework's own constant and kCFBooleanTrue.
        let keys = [kCGDisplayShowDuplicateLowResolutionModes];
        let values = [kCFBooleanTrue];
        let opts = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        );
        let modes = poll_until(
            200,
            |_| service_run_loop(0.05),
            || {
                let m = CGDisplayCopyAllDisplayModes(id, opts);
                (!m.is_null()).then_some(m)
            },
        );
        if !opts.is_null() {
            CFRelease(opts);
        }
        let Some(modes) = modes else {
            return Err(NO_MODES.into());
        };
        let mut chosen = None;
        let mut seen = Vec::new();
        for i in 0..CFArrayGetCount(modes) {
            let m = CFArrayGetValueAtIndex(modes, i);
            seen.push(format!(
                "{}/{}",
                CGDisplayModeGetWidth(m),
                CGDisplayModeGetPixelWidth(m)
            ));
            if CGDisplayModeGetPixelWidth(m) == pixel_w as usize
                && CGDisplayModeGetWidth(m) == pixel_w as usize / 2
            {
                chosen = Some(m);
                break;
            }
        }
        let result = match chosen {
            None => Err(format!(
                "no HiDPI mode ({pixel_w} px wide at half the points) on the virtual display; it lists {} modes (points/pixels): {}",
                seen.len(),
                seen.join(" ")
            )),
            Some(m) => {
                let mut cfg: *mut c_void = std::ptr::null_mut();
                if CGBeginDisplayConfiguration(&mut cfg) != 0 {
                    Err("could not begin a display configuration".into())
                } else {
                    CGConfigureDisplayWithDisplayMode(cfg, id, m, std::ptr::null());
                    if CGCompleteDisplayConfiguration(cfg, FOR_SESSION) != 0 {
                        Err("macOS refused the HiDPI mode".into())
                    } else {
                        Ok(())
                    }
                }
            }
        };
        CFRelease(modes);
        result
    }
}
