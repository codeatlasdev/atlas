//! ScreenCaptureKit stills of the user's screen (macOS): a display, a region of a display, or one window.
//!
//! Unlike `capture/sck.rs` (Atlas's own windows) this sees every app, so it needs the Screen Recording permission,
//! and it must leave Atlas's own capture surfaces (overlay, quick card, HUD, dock pill) out of the picture: the
//! content filter excludes them by window id, and the unit tests of `windows.rs` pin which ids those are.
//!
//! Everything returned to the rest of the app is plain Rust data ([`Display`], [`super::windows::Win`]); no
//! Objective-C object leaves this file.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_media::CMTime;
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    SCWindow,
};

use super::geom::Rect;
use super::windows::Win;

const WAIT: Duration = Duration::from_secs(10);

/// A display as ScreenCaptureKit reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    pub id: u32,
    /// Global points, top-left origin of the primary display, y down.
    pub frame: Rect,
    /// Backing scale: pixels per point.
    pub scale: f64,
}

/// What is on screen now.
pub struct Content {
    pub displays: Vec<Display>,
    pub windows: Vec<Win>,
    raw: Retained<SCShareableContent>,
}

// SAFETY: the retained SCShareableContent is never mutated; its getters and the SCK calls it is handed to are
// thread-safe, so it may be shared (the overlay keeps one for the window pick that follows it) and sent.
unsafe impl Send for Content {}
unsafe impl Sync for Content {}

fn cgrect(r: &Rect) -> CGRect {
    CGRect::new(CGPoint::new(r.x, r.y), CGSize::new(r.w, r.h))
}

fn rect(r: CGRect) -> Rect {
    Rect::new(r.origin.x, r.origin.y, r.size.width, r.size.height)
}

/// The Screen Recording grant of this process.
pub fn permission_granted() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

/// Shows the system prompt (once; macOS remembers the answer) and returns the current grant.
pub fn request_permission() -> bool {
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

/// The sentence for a failed ScreenCaptureKit call: a missing permission says how to fix it.
pub fn explain(e: &str) -> String {
    if permission_granted() {
        format!("ScreenCaptureKit: {e}")
    } else {
        "Atlas needs the Screen Recording permission to capture the screen. Grant it in System Settings > Privacy & Security > Screen Recording, then restart Atlas.".into()
    }
}

impl Content {
    /// The displays and on-screen windows now (the desktop-icon windows are not in the list).
    pub fn fetch() -> Result<Self, String> {
        Self::fetch_with(false)
    }

    /// [`Content::fetch`]; `desktop_windows` also lists Finder's desktop-icon windows (layer
    /// `windows::DESKTOP_ICON_LAYER`), which "hide desktop icons" then excludes from the picture.
    pub fn fetch_with(desktop_windows: bool) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
            // SAFETY: SCK passes a valid content object or a valid error for the duration of the callback.
            let r = match unsafe { Retained::retain(c) } {
                Some(c) => Ok(c),
                None => Err(unsafe { e.as_ref() }
                    .map(|e| e.localizedDescription().to_string())
                    .unwrap_or_else(|| "unknown ScreenCaptureKit error".into())),
            };
            let _ = tx.send(r);
        });
        // SAFETY: documented SCK call; the block is copied.
        unsafe {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                !desktop_windows, true, &block,
            );
        }
        let raw = rx
            .recv_timeout(WAIT)
            .map_err(|_| "ScreenCaptureKit did not answer".to_string())?
            .map_err(|e| explain(&e))?;
        // SAFETY: plain getters on the live content object.
        let (displays, windows) = unsafe {
            let displays = raw
                .displays()
                .iter()
                .map(|d: Retained<SCDisplay>| {
                    let frame = rect(d.frame());
                    Display {
                        id: d.displayID(),
                        scale: pixel_scale(d.displayID(), frame),
                        frame,
                    }
                })
                .collect();
            let windows = raw
                .windows()
                .iter()
                .map(|w: Retained<SCWindow>| Win {
                    id: w.windowID(),
                    frame: rect(w.frame()),
                    title: w.title().map(|t| t.to_string()).unwrap_or_default(),
                    pid: w.owningApplication().map_or(-1, |a| a.processID()),
                    layer: w.windowLayer() as i64,
                    on_screen: w.isOnScreen(),
                    app: w
                        .owningApplication()
                        .map(|a| a.applicationName().to_string())
                        .unwrap_or_default(),
                })
                .collect();
            (displays, windows)
        };
        Ok(Self {
            displays,
            windows,
            raw,
        })
    }

    pub fn display(&self, id: u32) -> Option<&Display> {
        self.displays.iter().find(|d| d.id == id)
    }

    fn sc_display(&self, id: u32) -> Result<Retained<SCDisplay>, String> {
        // SAFETY: plain getters.
        unsafe {
            self.raw
                .displays()
                .iter()
                .find(|d| d.displayID() == id)
                .ok_or_else(|| format!("display {id} is not available any more"))
        }
    }

    fn sc_windows(&self, ids: &[u32]) -> Retained<NSArray<SCWindow>> {
        // SAFETY: plain getters.
        let list: Vec<Retained<SCWindow>> = unsafe {
            self.raw
                .windows()
                .iter()
                .filter(|w| ids.contains(&w.windowID()))
                .collect()
        };
        NSArray::from_retained_slice(&list)
    }
}

/// Pixels per point of display `id`: the pixel width of its current mode over its width in points. A display
/// without a readable mode is 1× rather than a failed capture.
fn pixel_scale(id: u32, frame: Rect) -> f64 {
    crate::display_mac::display_mode(id)
        .filter(|m| frame.w > 0.0 && m.pixels.0 > 0.0)
        .map_or(1.0, |m| (m.pixels.0 / frame.w).max(1.0))
}

/// What a still is of.
pub enum Target<'a> {
    /// A display (`region`: points relative to that display's origin, or all of it).
    Display {
        content: &'a Content,
        id: u32,
        region: Option<Rect>,
    },
    /// One window.
    Window { content: &'a Content, id: u32 },
}

/// What differs between stills: the retina choice and whether a window keeps SCK's own drop shadow.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StillOpts {
    /// One pixel per point instead of the display's own scale.
    pub one_x: bool,
    /// Leave the window's drop shadow out of a window capture (the framing is done in pixels afterwards).
    pub ignore_shadow: bool,
}

/// Take a still and write it as a PNG at `out`. `exclude` are window ids left out of a display capture (Atlas's own
/// capture surfaces); `opts` the retina and shadow choices.
pub fn still_with(
    target: &Target,
    exclude: &[u32],
    out: &Path,
    opts: StillOpts,
) -> Result<(u32, u32), String> {
    let (filter, cfg, px) = still_config(target, exclude, opts)?;
    let (tx, rx) = mpsc::channel();
    let out = out.to_path_buf();
    let block = RcBlock::new(
        move |img: *mut objc2_core_graphics::CGImage, e: *mut NSError| {
            // SAFETY: SCK passes a valid image or a valid error, both live for this callback.
            let r = match unsafe { img.as_ref() } {
                Some(i) => crate::capture::png_of(i, &out),
                None => Err(sck_error(e)),
            };
            let _ = tx.send(r);
        },
    );
    capture_image(&filter, &cfg, &block);
    rx.recv_timeout(WAIT)
        .map_err(|_| "ScreenCaptureKit did not return the screenshot".to_string())??;
    Ok(px)
}

/// A display's pixels straight into memory: tight RGBA, opaque, no PNG encode and no file. This is what the
/// overlay freezes (`runner::show_overlay`); a PNG round trip of a retina frame is most of the hotkey-to-overlay time.
pub fn still_rgba(
    target: &Target,
    exclude: &[u32],
    opts: StillOpts,
) -> Result<super::pixels::Rgba, String> {
    still_pixels(target, exclude, opts, false)
}

/// [`still_rgba`] keeping the alpha channel, straight: a window's rounded corners and the margin of its shadow are
/// transparent, and the shadow and padding drawn around it (`pixels::frame_window`) are made from that alpha.
pub fn still_rgba_alpha(
    target: &Target,
    exclude: &[u32],
    opts: StillOpts,
) -> Result<super::pixels::Rgba, String> {
    still_pixels(target, exclude, opts, true)
}

fn still_pixels(
    target: &Target,
    exclude: &[u32],
    opts: StillOpts,
    keep_alpha: bool,
) -> Result<super::pixels::Rgba, String> {
    let (filter, cfg, _) = still_config(target, exclude, opts)?;
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(
        move |img: *mut objc2_core_graphics::CGImage, e: *mut NSError| {
            // SAFETY: SCK passes a valid image or a valid error, both live for this callback.
            let r = match unsafe { img.as_ref() } {
                Some(i) => read_pixels(i, keep_alpha),
                None => Err(sck_error(e)),
            };
            let _ = tx.send(r);
        },
    );
    capture_image(&filter, &cfg, &block);
    let data = rx
        .recv_timeout(WAIT)
        .map_err(|_| "ScreenCaptureKit did not return the screenshot".to_string())??;
    Ok(super::pixels::Rgba {
        data: data.0,
        size: data.1,
    })
}

/// Tight RGBA (opaque) of a 32-bit `CGImage`, read from its own pixel buffer: no PNG, no file. The channel order
/// comes from the image's alpha info and byte order (`pixels::Order::of`); a format that is not 8-bit-per-channel
/// 32-bit is an error, not a guess.
pub(super) fn rgba_of(img: &objc2_core_graphics::CGImage) -> Result<(Vec<u8>, (u32, u32)), String> {
    read_pixels(img, false)
}

fn read_pixels(
    img: &objc2_core_graphics::CGImage,
    keep_alpha: bool,
) -> Result<(Vec<u8>, (u32, u32)), String> {
    use objc2_core_graphics::{CGDataProvider, CGImage as Img, CGImageByteOrderInfo};
    let i = Some(img);
    let (w, h) = (Img::width(i), Img::height(i));
    let stride = Img::bytes_per_row(i);
    if Img::bits_per_pixel(i) != 32 || Img::bits_per_component(i) != 8 {
        return Err("the image is not 8-bit 32-bit pixels".into());
    }
    let little = Img::byte_order_info(i) == CGImageByteOrderInfo::Order32Little;
    let alpha = Img::alpha_info(i).0;
    let order = super::pixels::Order::of(alpha, little)
        .ok_or("the image has a pixel layout that is not read")?;
    let provider = Img::data_provider(i).ok_or("the image has no pixel data")?;
    let data = CGDataProvider::data(Some(&provider)).ok_or("the image has no pixel data")?;
    // SAFETY: `data` is a live CFData; its pointer and length describe one readable buffer until it is dropped.
    let bytes = unsafe { std::slice::from_raw_parts(data.byte_ptr(), data.length() as usize) };
    let rgba = if keep_alpha {
        // CGImageAlphaInfo 1 and 2 are the premultiplied layouts
        super::pixels::straight_rgba_from_rows(bytes, stride, w, h, order, matches!(alpha, 1 | 2))?
    } else {
        super::pixels::rgba_from_rows(bytes, stride, w, h, order)?
    };
    Ok((rgba, (w as u32, h as u32)))
}

fn sck_error(e: *mut NSError) -> String {
    // SAFETY: SCK passes a valid error (or null) for the duration of the callback.
    unsafe { e.as_ref() }
        .map(|e| explain(&e.localizedDescription().to_string()))
        .unwrap_or_else(|| "ScreenCaptureKit returned no image".into())
}

fn capture_image(
    filter: &SCContentFilter,
    cfg: &SCStreamConfiguration,
    block: &RcBlock<dyn Fn(*mut objc2_core_graphics::CGImage, *mut NSError)>,
) {
    // SAFETY: documented SCK call; the block is copied.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            filter,
            cfg,
            Some(block),
        );
    }
}

/// The content filter, stream configuration and pixel size of a still.
type StillConfig = (
    Retained<SCContentFilter>,
    Retained<SCStreamConfiguration>,
    (u32, u32),
);

/// The content filter, stream configuration and pixel size of a still of `target`.
fn still_config(target: &Target, exclude: &[u32], opts: StillOpts) -> Result<StillConfig, String> {
    // SAFETY: documented SCK setup; every object lives until the call returns.
    let r = unsafe {
        let cfg = SCStreamConfiguration::new();
        cfg.setShowsCursor(false);
        cfg.setMinimumFrameInterval(CMTime::new(1, 60));
        match target {
            Target::Display {
                content,
                id,
                region,
            } => {
                let d = content.display(*id).ok_or("that display is gone")?;
                let sc = content.sc_display(*id)?;
                let filter = SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &sc,
                    &content.sc_windows(exclude),
                );
                let (w, h) = match region {
                    Some(r) => {
                        cfg.setSourceRect(cgrect(r));
                        (r.w, r.h)
                    }
                    None => (d.frame.w, d.frame.h),
                };
                let scale = super::geom::effective_scale(d.scale, opts.one_x);
                let (pw, ph) = ((w * scale).round() as usize, (h * scale).round() as usize);
                cfg.setWidth(pw);
                cfg.setHeight(ph);
                (filter, cfg, (pw as u32, ph as u32))
            }
            Target::Window { content, id } => {
                let win = content
                    .raw
                    .windows()
                    .iter()
                    .find(|w| w.windowID() == *id)
                    .ok_or("that window is gone")?;
                let f = win.frame();
                let scale = super::geom::effective_scale(
                    content
                        .displays
                        .iter()
                        .find(|d| d.frame.intersect(&rect(f)).is_some())
                        .map_or(2.0, |d| d.scale),
                    opts.one_x,
                );
                let filter = SCContentFilter::initWithDesktopIndependentWindow(
                    SCContentFilter::alloc(),
                    &win,
                );
                cfg.setIgnoreShadowsSingleWindow(opts.ignore_shadow);
                let (pw, ph) = (
                    (f.size.width * scale).round() as usize,
                    (f.size.height * scale).round() as usize,
                );
                cfg.setWidth(pw);
                cfg.setHeight(ph);
                (filter, cfg, (pw as u32, ph as u32))
            }
        }
    };
    Ok(r)
}

/// What the stream of a recording captures besides the picture, and how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamOpts {
    /// Draw the pointer.
    pub cursor: bool,
    /// Draw a ring around each click (ScreenCaptureKit draws it itself, in the picture; macOS 15+).
    pub clicks: bool,
    /// Record what the Mac plays (macOS 15+ for the recording output). Atlas's own sounds are left out.
    pub system_audio: bool,
    /// Record the default microphone (macOS 15+; the caller has checked the permission).
    pub microphone: bool,
}

/// The content filter and stream configuration of a screen recording: display `id` minus the windows in `exclude`
/// (Atlas's capture surfaces), cropped to `region` (points relative to the display's origin) when given, `pixels`
/// output size, `fps`, and `opts` (pointer, click ring, audio sources).
pub fn recording_filter(
    content: &Content,
    id: u32,
    exclude: &[u32],
    region: Option<Rect>,
    pixels: (usize, usize),
    fps: i32,
    opts: StreamOpts,
) -> Result<(Retained<SCContentFilter>, Retained<SCStreamConfiguration>), String> {
    let sc = content.sc_display(id)?;
    // SAFETY: documented SCK setup; the objects are retained by the caller's stream.
    unsafe {
        let filter = SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            &sc,
            &content.sc_windows(exclude),
        );
        let cfg = SCStreamConfiguration::new();
        if let Some(r) = region {
            cfg.setSourceRect(cgrect(&r));
        }
        cfg.setWidth(pixels.0);
        cfg.setHeight(pixels.1);
        cfg.setMinimumFrameInterval(CMTime::new(1, fps));
        cfg.setShowsCursor(opts.cursor);
        cfg.setQueueDepth(6);
        // macOS 15 properties; recording (which needs 15 anyway) is the only caller.
        cfg.setShowMouseClicks(opts.clicks);
        if opts.system_audio {
            cfg.setCapturesAudio(true);
            cfg.setExcludesCurrentProcessAudio(true);
        }
        if opts.microphone {
            cfg.setCaptureMicrophone(true);
        }
        Ok((filter, cfg))
    }
}
