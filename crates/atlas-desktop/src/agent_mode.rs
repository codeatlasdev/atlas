//! Agent instances: an Atlas Desktop started by an agent (to debug, screenshot or record it) must never appear on the
//! user's desktop: no window on his monitors or Spaces, no Dock icon or bounce, no focus theft, no mouse/keyboard.
//!
//! It runs with `ATLAS_AGENT=1` (debug builds only: a release app ignores it) and then
//!   * its activation policy is `Prohibited` (never frontmost, no Dock icon, no menu bar takeover),
//!   * its window is placed on a private virtual display (`display_mac.rs`) that is not any of the user's displays,
//!     is never occluded (full-rate `requestAnimationFrame`, no visibility throttling) and is capturable,
//!   * nothing in the app activates it or steals focus (`show_main` consults [`Mode::focus_allowed`]).
//!
//! This module is the pure part: the decision, and where a window goes on the virtual display.

/// Environment variable that switches the mode on.
pub const ENV: &str = "ATLAS_AGENT";

/// How this process was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The user's own app.
    User,
    /// Started by an agent: invisible to the user.
    Agent,
}

impl Mode {
    /// `value` = the `ATLAS_AGENT` variable, `debug_build` = `cfg!(debug_assertions)`. A release app is always the user's.
    pub fn resolve(value: Option<&str>, debug_build: bool) -> Self {
        match value {
            Some("1" | "true") if debug_build => Mode::Agent,
            _ => Mode::User,
        }
    }

    pub fn current() -> Self {
        Self::resolve(std::env::var(ENV).ok().as_deref(), cfg!(debug_assertions))
    }

    /// Whether the app may raise itself, take focus, or switch Spaces.
    pub fn focus_allowed(self) -> bool {
        self == Mode::User
    }

    /// macOS activation policy: the user's app is a normal app (Dock icon, can be frontmost); an agent instance is
    /// `Prohibited` (no Dock icon or bounce, can never become frontmost, no menu bar takeover).
    pub fn policy(self) -> Policy {
        match self {
            Mode::User => Policy::Regular,
            Mode::Agent => Policy::Prohibited,
        }
    }

    /// The menu-bar tray icon is the user's; an agent instance has none (it would appear in HIS menu bar).
    pub fn tray_allowed(self) -> bool {
        self == Mode::User
    }
}

/// The macOS activation policies this app uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Regular,
    Prohibited,
}

/// A rectangle in global display coordinates (points, origin top-left like `CGDisplayBounds`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// Where the virtual display goes: to the right of every real display, with a gap, so that the mouse can never reach
/// it by accident and no real display's coordinates change. `real` = bounds of the user's displays.
pub fn virtual_origin(real: &[Rect]) -> (f64, f64) {
    const GAP: f64 = 0.0; // macOS clamps a display to touch the arrangement; the right edge is the quietest side
    let right = real.iter().map(|r| r.x + r.w).fold(0.0, f64::max);
    (right + GAP, 0.0)
}

/// Top-left origin for a window of `size` on a display of `display` bounds, centered. Never negative offsets inside
/// the display: a window larger than the display sits at its top-left corner.
pub fn window_origin(display: Rect, size: (f64, f64)) -> (f64, f64) {
    (
        display.x + ((display.w - size.0) / 2.0).max(SAFE_MARGIN),
        display.y + ((display.h - size.1) / 2.0).max(SAFE_MARGIN),
    )
}

/// Where a window of `size` goes on `display` so AppKit leaves it alone: centred, then moved into `allowed` (the
/// usable frame of the screen, which excludes the reserved menu-bar band) inset by the margin.
pub fn target_frame(display: Rect, allowed: Rect, size: (f64, f64)) -> Rect {
    let band = Rect {
        x: allowed.x.max(display.x),
        y: allowed.y.max(display.y),
        w: allowed.w.min(display.w),
        h: allowed.h.min(display.h),
    }
    .inset_by(SAFE_MARGIN);
    let (w, h) = (size.0.min(band.w), size.1.min(band.h));
    let (x, y) = window_origin(band.outset(SAFE_MARGIN), (w, h));
    Rect {
        x: x.clamp(band.x, band.x + band.w - w),
        y: y.clamp(band.y, band.y + band.h - h),
        w,
        h,
    }
}

/// A top-left global frame as AppKit's bottom-left one: Cocoa's origin is the bottom-left of the PRIMARY display, and
/// `primary_h` is that display's height. `y` flips about it.
pub fn cocoa_frame(frame: Rect, primary_h: f64) -> Rect {
    Rect {
        y: primary_h - (frame.y + frame.h),
        ..frame
    }
}

/// The window size that fits the virtual display: a restored size (window-state, `--scratch` settings from a bigger
/// monitor) larger than the display is clamped to it, so the window is shown rather than refused.
pub fn fit_size(size: (f64, f64), display: Rect) -> (f64, f64) {
    (
        size.0.min(display.w - 2.0 * SAFE_MARGIN),
        size.1.min(display.h - 2.0 * SAFE_MARGIN),
    )
}

/// Clear space kept between an agent window and every edge of its display (and so every other display).
pub const SAFE_MARGIN: f64 = 4.0;

/// Vendor id every Atlas agent display is created with: how a sibling agent display is told from the user's panel.
pub const AGENT_VENDOR: u32 = 0x4147;

/// One active display as the OS reports it right now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveDisplay {
    /// `CGDirectDisplayID`.
    pub id: u32,
    pub bounds: Rect,
    /// `CGDisplayVendorNumber`.
    pub vendor: u32,
}

/// The current display mode of a display: its size in points and in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModeSize {
    pub points: (f64, f64),
    pub pixels: (f64, f64),
}

/// Everything the show decision looks at, all read from the OS after the window was placed.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The `CGDirectDisplayID` this instance created and still holds.
    pub own_id: u32,
    /// The size in points and the pixel ratio this instance asked for.
    pub logical: (f64, f64),
    pub scale: u32,
    /// Every active display (the user's, sibling agents', ours).
    pub displays: &'a [ActiveDisplay],
    /// The current mode of our display; `None` when it has none.
    pub own_mode: Option<ModeSize>,
    /// The frame we told AppKit to use (global top-left points)...
    pub target: Rect,
    /// ...and the frame AppKit reports now.
    pub frame: Rect,
    /// The frame AppKit's `constrainFrameRect:toScreen:` gives that frame: where the window really ends up once ordered in.
    pub constrained: Rect,
}

/// Proof that [`show_decision`] said yes. The only way to get one; `agent_display` needs it to order the window in,
/// so no other path can show an agent window.
#[derive(Debug)]
pub struct ShowGranted(());

impl Rect {
    fn inside(&self, o: &Rect) -> bool {
        self.x >= o.x
            && self.y >= o.y
            && self.x + self.w <= o.x + o.w
            && self.y + self.h <= o.y + o.h
    }

    fn inset_by(&self, m: f64) -> Rect {
        self.outset(-m)
    }

    /// This rectangle grown by `m` on every side.
    fn outset(&self, m: f64) -> Rect {
        Rect {
            x: self.x - m,
            y: self.y - m,
            w: self.w + 2.0 * m,
            h: self.h + 2.0 * m,
        }
    }

    /// Same rectangle to within a point (AppKit may round a frame).
    fn same_as(&self, o: &Rect) -> bool {
        (self.x - o.x).abs() <= 1.0
            && (self.y - o.y).abs() <= 1.0
            && (self.w - o.w).abs() <= 1.0
            && (self.h - o.h).abs() <= 1.0
    }
}

/// A display's bounds shrunk by [`SAFE_MARGIN`]: where a window may be.
fn inset(r: Rect) -> Rect {
    Rect {
        x: r.x + SAFE_MARGIN,
        y: r.y + SAFE_MARGIN,
        w: r.w - 2.0 * SAFE_MARGIN,
        h: r.h - 2.0 * SAFE_MARGIN,
    }
}

/// The one decision before an agent window is shown. It fails CLOSED: a window is shown only when its frame, read
/// back from AppKit after the placement, is wholly inside the one active display whose id this instance created and
/// still owns, that display has the mode we asked for, and no other display (the user's, a sibling's) touches either
/// the window or our display. Any doubt is an `Err` naming it: unknown or duplicated display, mode missing or
/// different, placement not applied yet, nothing of the user's on the list to compare with.
pub fn show_decision(f: &Facts) -> Result<ShowGranted, String> {
    let mut ours = f.displays.iter().filter(|d| d.id == f.own_id);
    let own = ours.next().ok_or(
        "our virtual display is not among the active displays (gone, or never became active)",
    )?;
    if ours.next().is_some() {
        return Err("our display id appears more than once in the active display list".into());
    }
    if own.vendor != AGENT_VENDOR {
        return Err(format!(
            "display {} is not one this app created (vendor {:#x})",
            own.id, own.vendor
        ));
    }
    let mode = f
        .own_mode
        .ok_or("the virtual display has no current mode (CoreGraphics lists none)")?;
    let scale = f64::from(f.scale);
    if mode.points != f.logical
        || mode.pixels != (f.logical.0 * scale, f.logical.1 * scale)
        || (own.bounds.w, own.bounds.h) != f.logical
    {
        return Err(format!(
            "the virtual display is {:?} at mode {mode:?}, not the {:?} points at {}x that was asked for",
            own.bounds, f.logical, f.scale
        ));
    }
    let others = || f.displays.iter().filter(|d| d.id != f.own_id);
    if !others().any(|d| d.vendor != AGENT_VENDOR) {
        return Err(
            "no user display is known, so the window cannot be proven clear of them".into(),
        );
    }
    if let Some(d) = others().find(|d| d.bounds.intersects(&own.bounds)) {
        return Err(format!(
            "the virtual display {:?} overlaps display {} {:?}",
            own.bounds, d.id, d.bounds
        ));
    }
    if !f.constrained.same_as(&f.frame) {
        return Err(format!(
            "AppKit would constrain the frame {:?} to {:?} when it is ordered in",
            f.frame, f.constrained
        ));
    }
    if !f.frame.same_as(&f.target) {
        return Err(format!(
            "the placement was not applied: AppKit reports {:?}, the target is {:?}",
            f.frame, f.target
        ));
    }
    if !f.frame.inside(&inset(own.bounds)) {
        return Err(format!(
            "window frame {:?} is not inside the virtual display {:?} inset by {SAFE_MARGIN} (the window server outsets windows)",
            f.frame, own.bounds
        ));
    }
    if let Some(d) = others().find(|d| d.bounds.intersects(&f.frame.outset(SAFE_MARGIN))) {
        return Err(format!(
            "window frame {:?} overlaps display {} {:?}",
            f.frame, d.id, d.bounds
        ));
    }
    Ok(ShowGranted(()))
}

/// Why the watchdog must hide a visible agent window, or `None` when it is where it belongs: wholly inside the
/// display `own_id`, touching no other display.
pub fn watchdog_verdict(frame: Rect, own_id: u32, displays: &[ActiveDisplay]) -> Option<String> {
    let Some(own) = displays.iter().find(|d| d.id == own_id) else {
        return Some("our virtual display is gone".into());
    };
    let others: Vec<Rect> = displays
        .iter()
        .filter(|d| d.id != own_id)
        .map(|d| d.bounds)
        .collect();
    if !windows_on_user_displays(&[frame.outset(SAFE_MARGIN)], &others).is_empty() {
        return Some(format!("the window {frame:?} is on another display"));
    }
    (!frame.inside(&inset(own.bounds))).then(|| {
        format!(
            "the window {frame:?} is not inside the virtual display {:?}",
            own.bounds
        )
    })
}

/// Indices of the `windows` (global top-left rects, as the window server reports them) that intersect any of the
/// user's `displays`. An agent instance's windows must never be among them.
pub fn windows_on_user_displays(windows: &[Rect], displays: &[Rect]) -> Vec<usize> {
    windows
        .iter()
        .enumerate()
        .filter(|(_, w)| displays.iter().any(|d| d.intersects(w)))
        .map(|(i, _)| i)
        .collect()
}

/// Pixel ratio of the agent display from `ATLAS_AGENT_SCALE`: `1` (a standard monitor) or `2` (retina, the default).
/// Anything else is the default: a typo must not silently give an unexpected ratio such as 3.
pub fn scale_from(value: Option<&str>) -> u32 {
    match value {
        Some("1") => 1,
        _ => 2,
    }
}

/// Pixels of the virtual display for a logical `size` at `scale` (2.0 = retina).
pub fn backing_pixels(size: (u32, u32), scale: u32) -> (u32, u32) {
    (size.0 * scale, size.1 * scale)
}

/// Which WebKit data store a webview gets. WKWebView ignores `HOME`: its default store is
/// `~/Library/WebKit/<bundle id>` of the real user, so an agent instance on the default store reads and writes the
/// user's own localStorage (incident 2026-10-22: an instance rewrote the installed app's store).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    /// The bundle's default store: the user's app only.
    Default,
    /// A persistent store of its own (`WKWebsiteDataStore(forIdentifier:)`, macOS 14+), stable per instance.
    Isolated([u8; 16]),
    /// In memory, gone at exit: an agent instance on a macOS without custom stores.
    Ephemeral,
}

/// `instance` = something unique and stable per agent instance (its debug socket path); `macos_major` = 0 off macOS.
pub fn webview_store(mode: Mode, instance: &str, macos_major: u32) -> Store {
    match mode {
        Mode::User => Store::Default,
        Mode::Agent if macos_major >= 14 || macos_major == 0 => {
            Store::Isolated(atlas_core::agent_store::id(instance))
        }
        Mode::Agent => Store::Ephemeral,
    }
}

/// The store for this process's webviews.
pub fn current_store() -> Store {
    #[cfg(target_os = "macos")]
    let major = objc2_foundation::NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion as u32;
    #[cfg(not(target_os = "macos"))]
    let major = 0; // WebKitGTK and WebView2 keep data under HOME / the instance's dirs already
    webview_store(
        Mode::current(),
        &std::env::var("ATLAS_DEBUG_SOCK").unwrap_or_default(),
        major,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_instance_never_gets_the_users_default_webkit_store() {
        // Regression: 2026-10-22, the `lab` instance wrote into ~/Library/WebKit/<the installed app's id>.
        for major in [0, 13, 14, 15, 26] {
            assert_ne!(
                webview_store(Mode::Agent, "/tmp/atlas-agent-a/debug.sock", major),
                Store::Default,
                "{major}"
            );
        }
        assert_eq!(webview_store(Mode::Agent, "x", 13), Store::Ephemeral);
        assert_eq!(webview_store(Mode::User, "x", 26), Store::Default);
    }

    #[test]
    fn each_instance_has_its_own_store_and_keeps_it_across_restarts() {
        let a = webview_store(Mode::Agent, "/tmp/atlas-agent-a/debug.sock", 26);
        assert_eq!(
            a,
            webview_store(Mode::Agent, "/tmp/atlas-agent-a/debug.sock", 26)
        );
        assert_ne!(
            a,
            webview_store(Mode::Agent, "/tmp/atlas-agent-b/debug.sock", 26)
        );
    }

    const PANEL: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1352.0,
        h: 878.0,
    };
    const OURS_ID: u32 = 77;
    const LOGICAL: (f64, f64) = (1280.0, 820.0);

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    fn panel() -> ActiveDisplay {
        ActiveDisplay {
            id: 1,
            bounds: PANEL,
            vendor: 0x610,
        }
    }

    fn ours(bounds: Rect) -> ActiveDisplay {
        ActiveDisplay {
            id: OURS_ID,
            bounds,
            vendor: AGENT_VENDOR,
        }
    }

    fn sibling(bounds: Rect) -> ActiveDisplay {
        ActiveDisplay {
            id: 9,
            bounds,
            vendor: AGENT_VENDOR,
        }
    }

    fn mode2x() -> Option<ModeSize> {
        Some(ModeSize {
            points: LOGICAL,
            pixels: (2560.0, 1640.0),
        })
    }

    /// A healthy start: our retina display right of the panel, the window centred on it exactly as placed.
    fn healthy(displays: &[ActiveDisplay]) -> Facts<'_> {
        let win = r(
            1352.0 + SAFE_MARGIN,
            SAFE_MARGIN,
            1280.0 - 2.0 * SAFE_MARGIN,
            820.0 - 2.0 * SAFE_MARGIN,
        );
        Facts {
            own_id: OURS_ID,
            logical: LOGICAL,
            scale: 2,
            displays,
            own_mode: mode2x(),
            target: win,
            frame: win,
            constrained: win,
        }
    }

    #[test]
    fn a_healthy_start_is_shown() {
        let d = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        assert!(show_decision(&healthy(&d)).is_ok());
    }

    /// Incident (three agent instances started at once, pid 60923): a window at (36, 59) 1280x790 was on-screen on
    /// the built-in display for ~25 s. The virtual display had come up as 2560x1640 points at 1x, at the origin of
    /// the layout: it CONTAINS that frame, so a decision that only asks "is the frame inside our display?" allowed
    /// the show. Reproduced here with the observed display set.
    #[test]
    fn incident_a_window_on_the_panel_is_refused_even_when_our_oversized_display_contains_it() {
        let window = r(36.0, 59.0, 1280.0, 790.0);
        let wrong_mode = Some(ModeSize {
            points: (2560.0, 1640.0),
            pixels: (2560.0, 1640.0),
        });
        // as observed: 2560x1640 at 1x, at the origin, over the panel
        let d = [panel(), ours(r(0.0, 0.0, 2560.0, 1640.0))];
        let f = Facts {
            own_mode: wrong_mode,
            target: window,
            frame: window,
            ..healthy(&d)
        };
        let e = show_decision(&f).unwrap_err();
        assert!(e.contains("not the"), "the wrong size/ratio is named: {e}");
        // the same, with the display size and mode right but the layout put it on the panel
        let d = [panel(), ours(r(0.0, 0.0, 1280.0, 820.0))];
        let f = Facts {
            target: window,
            frame: window,
            ..healthy(&d)
        };
        assert!(show_decision(&f).unwrap_err().contains("overlaps"));
        // "no modes (waited 10 s)": the display exists with bounds but has no mode
        let d = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        let f = Facts {
            own_mode: None,
            ..healthy(&d)
        };
        assert!(show_decision(&f).unwrap_err().contains("no current mode"));
    }

    #[test]
    fn every_doubt_keeps_the_window_hidden() {
        let good = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        // our display id is not on the active list (gone / never active)
        let d = [panel()];
        assert!(
            show_decision(&healthy(&d))
                .unwrap_err()
                .contains("not among")
        );
        // our id twice
        let d = [
            panel(),
            ours(r(1352.0, 0.0, 1280.0, 820.0)),
            ours(r(0.0, 0.0, 1.0, 1.0)),
        ];
        assert!(
            show_decision(&healthy(&d))
                .unwrap_err()
                .contains("more than once")
        );
        // the id belongs to a display this app did not create
        let d = [
            ActiveDisplay {
                vendor: 0x610,
                ..ours(r(1352.0, 0.0, 1280.0, 820.0))
            },
            panel(),
        ];
        assert!(
            show_decision(&healthy(&d))
                .unwrap_err()
                .contains("not one this app created")
        );
        // the mode is the 1x one while 2x was asked for, the points differ, or there are no pixels
        for bad in [
            ModeSize {
                points: LOGICAL,
                pixels: LOGICAL,
            },
            ModeSize {
                points: (1024.0, 768.0),
                pixels: (2048.0, 1536.0),
            },
            ModeSize {
                points: LOGICAL,
                pixels: (0.0, 0.0),
            },
        ] {
            let f = Facts {
                own_mode: Some(bad),
                ..healthy(&good)
            };
            assert!(show_decision(&f).is_err(), "{bad:?}");
        }
        // bounds of the display differ from the logical size
        let d = [panel(), ours(r(1352.0, 0.0, 1281.0, 820.0))];
        assert!(show_decision(&healthy(&d)).is_err());
        // only our display is known, or only siblings': nothing to prove the window clear of
        let d = [ours(r(1352.0, 0.0, 1280.0, 820.0))];
        assert!(
            show_decision(&healthy(&d))
                .unwrap_err()
                .contains("no user display")
        );
        let d = [
            ours(r(1352.0, 0.0, 1280.0, 820.0)),
            sibling(r(2632.0, 0.0, 1280.0, 820.0)),
        ];
        assert!(
            show_decision(&healthy(&d))
                .unwrap_err()
                .contains("no user display")
        );
        // placement not applied yet: AppKit still reports a frame other than the one we asked for
        let f = Facts {
            frame: r(1400.0, 10.0, 1280.0, 790.0),
            constrained: r(1400.0, 10.0, 1280.0, 790.0),
            ..healthy(&good)
        };
        assert!(show_decision(&f).unwrap_err().contains("not applied"));
        // applied, but not inside our display (one point too wide)
        let t = r(1352.0, 0.0, 1281.0, 820.0);
        let f = Facts {
            target: t,
            frame: t,
            constrained: t,
            ..healthy(&good)
        };
        assert!(show_decision(&f).unwrap_err().contains("not inside"));
        // our display overlaps a sibling agent display: a layout we do not trust
        let d = [
            panel(),
            ours(r(1352.0, 0.0, 1280.0, 820.0)),
            sibling(r(2000.0, 0.0, 1280.0, 820.0)),
        ];
        assert!(
            show_decision(&healthy(&d))
                .unwrap_err()
                .contains("overlaps")
        );
    }

    /// Incident (real single-instance run, sampled every 50 ms): the window server reported our window outset by one
    /// point on each side (1282x792 at 1351,29 for a 1280x790 frame at 1352,30), i.e. one column of the user's
    /// panel, whose right edge is 1352. A frame that merely touches the display edge is not safe: the window must keep
    /// SAFE_MARGIN clear of every other display, and sit inside our display inset by it.
    #[test]
    fn a_window_touching_the_seam_is_refused_because_the_window_server_outsets_it() {
        let d = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        // flush with the seam: exactly the frame placed in the real run
        let flush = r(1352.0, 0.0, 1280.0, 820.0);
        let f = Facts {
            target: flush,
            frame: flush,
            ..healthy(&d)
        };
        assert!(show_decision(&f).is_err(), "flush window must be refused");
        // the margin-respecting placement is accepted
        let t = r(
            1352.0 + SAFE_MARGIN,
            SAFE_MARGIN,
            1280.0 - 2.0 * SAFE_MARGIN,
            820.0 - 2.0 * SAFE_MARGIN,
        );
        let f = Facts {
            target: t,
            frame: t,
            ..healthy(&d)
        };
        assert!(show_decision(&f).is_ok());
        // the outset the window server reported (1 pt each side) of that frame still clears the panel
        let outset = r(t.x - 1.0, t.y - 1.0, t.w + 2.0, t.h + 2.0);
        assert!(windows_on_user_displays(&[outset], &[PANEL]).is_empty());
        // the watchdog agrees with the decision on both
        assert!(watchdog_verdict(flush, OURS_ID, &d).is_some());
        assert!(watchdog_verdict(t, OURS_ID, &d).is_none());
        // the window server's real report from the incident run is hidden
        assert!(watchdog_verdict(r(1351.0, 29.0, 1282.0, 792.0), OURS_ID, &d).is_some());
    }

    /// The size a window gets on our display leaves the margin on every side, and the origin centres it.
    #[test]
    fn the_window_is_sized_and_placed_inside_the_display_inset_by_the_margin() {
        let d = r(1352.0, 0.0, 1280.0, 820.0);
        let size = fit_size((1280.0, 820.0), d);
        assert_eq!(
            size,
            (1280.0 - 2.0 * SAFE_MARGIN, 820.0 - 2.0 * SAFE_MARGIN)
        );
        let (x, y) = window_origin(d, size);
        assert_eq!((x, y), (1352.0 + SAFE_MARGIN, SAFE_MARGIN));
        // a smaller window is untouched and stays centred
        assert_eq!(fit_size((1000.0, 600.0), d), (1000.0, 600.0));
    }

    /// Incident (real run after the margin fix): AppKit moved the shown window from y=4 to y=30 and cut it to 790
    /// high (the menu-bar band is reserved on any screen), so the frame read back BEFORE ordering in was not the one
    /// on screen and the watchdog had to hide a window that had passed the decision. A target is only trusted when
    /// AppKit's own constrained frame for it is the same rectangle.
    #[test]
    fn a_target_appkit_would_move_is_refused_before_it_is_shown() {
        let d = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        let t = healthy(&d).target;
        let moved = r(t.x, 30.0, t.w, 790.0);
        let f = Facts {
            constrained: moved,
            ..healthy(&d)
        };
        assert!(show_decision(&f).unwrap_err().contains("constrain"));
        // the same target, unchanged by AppKit: shown
        assert!(show_decision(&healthy(&d)).is_ok());
    }

    /// The target is derived from the band AppKit allows (`allowed`: the display's usable frame in global top-left
    /// points) so that constraining leaves it alone.
    #[test]
    fn the_target_is_the_display_inset_by_the_margin_and_cut_to_what_appkit_allows() {
        let display = r(1352.0, 0.0, 1280.0, 820.0);
        // AppKit reserves the top 26 points of the screen
        let allowed = r(1352.0, 26.0, 1280.0, 794.0);
        let size = fit_size((1280.0, 820.0), display);
        let t = target_frame(display, allowed, size);
        assert!(t.inside(&allowed.inset_by(SAFE_MARGIN)), "{t:?}");
        assert!(t.inside(&inset(display)));
        // nothing reserved: centred as before
        let t = target_frame(display, display, (1000.0, 600.0));
        assert_eq!((t.x, t.y, t.w, t.h), (1492.0, 110.0, 1000.0, 600.0));
    }

    #[test]
    fn a_1x_display_is_accepted_only_when_1x_was_asked_for() {
        let d = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        let one = Some(ModeSize {
            points: LOGICAL,
            pixels: LOGICAL,
        });
        let f = Facts {
            scale: 1,
            own_mode: one,
            ..healthy(&d)
        };
        assert!(show_decision(&f).is_ok());
        let f = Facts {
            scale: 2,
            own_mode: one,
            ..healthy(&d)
        };
        assert!(show_decision(&f).is_err());
    }

    /// Second line (watchdog) with the incident data: the window on the built-in panel is hidden; one on our
    /// display is left alone; our display disappearing hides it too.
    #[test]
    fn the_watchdog_hides_a_window_on_any_other_display_or_off_our_own() {
        let d = [panel(), ours(r(1352.0, 0.0, 1280.0, 820.0))];
        let on_panel = r(36.0, 59.0, 1280.0, 790.0);
        assert!(
            watchdog_verdict(on_panel, OURS_ID, &d)
                .unwrap()
                .contains("another display")
        );
        assert!(watchdog_verdict(healthy(&d).frame, OURS_ID, &d).is_none());
        // straddling the seam
        assert!(watchdog_verdict(r(1300.0, 0.0, 600.0, 400.0), OURS_ID, &d).is_some());
        // off every display: not ours either, so hidden
        assert!(
            watchdog_verdict(r(9000.0, 0.0, 100.0, 100.0), OURS_ID, &d)
                .unwrap()
                .contains("not inside")
        );
        // our display is gone
        assert!(
            watchdog_verdict(r(1352.0, 0.0, 1280.0, 820.0), OURS_ID, &[panel()])
                .unwrap()
                .contains("gone")
        );
        // the verdict goes by display id, not vendor: a panel wrongly labelled as an agent display still counts
        let mislabelled = [
            ActiveDisplay {
                vendor: AGENT_VENDOR,
                ..panel()
            },
            ours(r(1352.0, 0.0, 1280.0, 820.0)),
        ];
        assert!(watchdog_verdict(on_panel, OURS_ID, &mislabelled).is_some());
    }

    /// Incident (three agent instances started at once): one window was on-screen on the user's built-in display for
    /// ~25 s although the placement code checks the frame before showing. The placement check cannot be the only
    /// guard: this one looks at what the window server says afterwards, for the life of the process.
    #[test]
    fn a_window_that_intersects_a_users_display_is_reported_with_the_display_it_is_on() {
        let user = [
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1352.0,
                h: 878.0,
            },
            Rect {
                x: -1920.0,
                y: 0.0,
                w: 1920.0,
                h: 1080.0,
            },
        ];
        let on_builtin = Rect {
            x: 36.0,
            y: 59.0,
            w: 1280.0,
            h: 790.0,
        };
        let on_virtual = Rect {
            x: 1352.0,
            y: 58.0,
            w: 1280.0,
            h: 820.0,
        };
        let straddling = Rect {
            x: 1300.0,
            y: 58.0,
            w: 600.0,
            h: 400.0,
        };
        assert_eq!(windows_on_user_displays(&[on_builtin], &user), vec![0]);
        assert!(windows_on_user_displays(&[on_virtual], &user).is_empty());
        // even a sliver of one pixel row counts: it would show on his screen
        assert_eq!(
            windows_on_user_displays(&[on_virtual, straddling], &user),
            vec![1]
        );
        // tiny helper windows (menu extras, shadows) are ignored only by the caller, never here
        assert_eq!(
            windows_on_user_displays(
                &[Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0
                }],
                &user
            ),
            vec![0]
        );
        assert!(windows_on_user_displays(&[], &user).is_empty());
        assert!(
            windows_on_user_displays(&[on_builtin], &[]).is_empty(),
            "no user display known: nothing to be on"
        );
    }

    #[test]
    fn only_a_debug_build_with_the_variable_set_is_an_agent_instance() {
        assert_eq!(Mode::resolve(Some("1"), true), Mode::Agent);
        assert_eq!(Mode::resolve(Some("true"), true), Mode::Agent);
        assert_eq!(Mode::resolve(None, true), Mode::User);
        assert_eq!(Mode::resolve(Some("0"), true), Mode::User);
        assert_eq!(Mode::resolve(Some(""), true), Mode::User);
        // a release app must never be silenced by a leftover variable
        assert_eq!(Mode::resolve(Some("1"), false), Mode::User);
    }

    #[test]
    fn an_agent_instance_has_no_dock_icon_no_tray_and_cannot_be_frontmost() {
        assert_eq!(Mode::Agent.policy(), Policy::Prohibited);
        assert!(!Mode::Agent.tray_allowed());
        assert_eq!(Mode::User.policy(), Policy::Regular);
        assert!(Mode::User.tray_allowed());
    }

    #[test]
    fn an_agent_instance_may_never_take_focus_the_users_app_may() {
        assert!(!Mode::Agent.focus_allowed());
        assert!(Mode::User.focus_allowed());
    }

    const BUILTIN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1352.0,
        h: 878.0,
    };

    #[test]
    fn the_virtual_display_goes_right_of_every_real_display_and_overlaps_none() {
        let external = Rect {
            x: 1352.0,
            y: -200.0,
            w: 2560.0,
            h: 1440.0,
        };
        for real in [vec![BUILTIN], vec![BUILTIN, external]] {
            let (x, y) = virtual_origin(&real);
            let v = Rect {
                x,
                y,
                w: 1280.0,
                h: 820.0,
            };
            assert!(real.iter().all(|r| !r.intersects(&v)), "{real:?} vs {v:?}");
            assert!(real.iter().all(|r| x >= r.x + r.w));
        }
        assert_eq!(virtual_origin(&[BUILTIN]), (1352.0, 0.0));
    }

    #[test]
    fn with_no_displays_the_origin_is_zero() {
        assert_eq!(virtual_origin(&[]), (0.0, 0.0));
    }

    #[test]
    fn a_window_is_centered_on_the_virtual_display_and_stays_inside_it() {
        let d = Rect {
            x: 1352.0,
            y: 0.0,
            w: 1280.0,
            h: 820.0,
        };
        // a window as big as the display would sit flush; the origin never goes closer than the margin
        assert_eq!(
            window_origin(d, (1280.0, 820.0)),
            (1352.0 + SAFE_MARGIN, SAFE_MARGIN)
        );
        assert_eq!(window_origin(d, (1000.0, 600.0)), (1492.0, 110.0));
        // bigger than the display: top-left corner inside the margin, never off to the left/top of it
        assert_eq!(
            window_origin(d, (3000.0, 2000.0)),
            (1352.0 + SAFE_MARGIN, SAFE_MARGIN)
        );
    }

    /// Incident (first agent run): the window was placed with a top-left position but read back in AppKit's
    /// bottom-left space, so frames never matched; the conversion is its own inverse about the primary display.
    #[test]
    fn cocoa_and_global_frames_convert_both_ways_about_the_primary_display() {
        let primary_h = 878.0;
        let top_left = Rect {
            x: 1352.0,
            y: 0.0,
            w: 1280.0,
            h: 820.0,
        };
        let cocoa = cocoa_frame(top_left, primary_h);
        assert_eq!(
            cocoa,
            Rect {
                x: 1352.0,
                y: 58.0,
                w: 1280.0,
                h: 820.0
            }
        );
        assert_eq!(cocoa_frame(cocoa, primary_h), top_left);
        // a display above the primary has a negative top-left y
        let above = Rect {
            x: 0.0,
            y: -820.0,
            w: 1280.0,
            h: 820.0,
        };
        assert_eq!(cocoa_frame(above, primary_h).y, 878.0);
    }

    #[test]
    fn a_restored_window_bigger_than_the_display_is_clamped_to_it() {
        let d = Rect {
            x: 1352.0,
            y: 0.0,
            w: 1280.0,
            h: 820.0,
        };
        assert_eq!(fit_size((1000.0, 600.0), d), (1000.0, 600.0));
        assert_eq!(
            fit_size((2400.0, 1400.0), d),
            (1280.0 - 2.0 * SAFE_MARGIN, 820.0 - 2.0 * SAFE_MARGIN)
        );
        assert_eq!(
            fit_size((2400.0, 600.0), d),
            (1280.0 - 2.0 * SAFE_MARGIN, 600.0)
        );
    }

    #[test]
    fn the_agent_display_is_retina_unless_one_is_asked_for_and_nothing_else_is_accepted() {
        assert_eq!(scale_from(None), 2);
        assert_eq!(scale_from(Some("2")), 2);
        assert_eq!(scale_from(Some("1")), 1);
        for bad in ["", "3", "0", "1.5", "retina", "-1"] {
            assert_eq!(scale_from(Some(bad)), 2, "{bad:?}");
        }
    }

    #[test]
    fn retina_means_twice_the_pixels() {
        assert_eq!(backing_pixels((1280, 820), 2), (2560, 1640));
        assert_eq!(backing_pixels((1280, 820), 1), (1280, 820));
    }
}
