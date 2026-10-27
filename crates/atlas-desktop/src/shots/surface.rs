//! The windows capture draws: the crosshair overlay (one per display), the quick-access stack, the recording HUD,
//! the editor and pinned screenshots. Every one of them is shown through [`show`], the single place a capture
//! window is ordered in, because that is where the agent-instance rule lives: an agent instance (`ATLAS_AGENT=1`)
//! never shows anything on the user's displays, so there it goes through `agent_display::park_and_show` (the
//! virtual display and the watchdog) and the overlay, which is meant to cover a real screen, is not built at all.
//!
//! All of them are listed in [`Surface`] with their label, size and behaviour (pure data, tested), so "which
//! windows can capture create" has one answer and the capture filter can exclude them all by label.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::geom::Rect;
use super::prefs::Corner;

/// A window capture owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Full-display crosshair; one per display, label suffixed with the display id.
    Overlay,
    /// The stack of quick-access cards (one window, sized to the number of cards).
    Quick,
    Hud,
    Editor,
    /// A screenshot floating above every window; any number, label suffixed with the pin's number.
    Pin,
    /// The keystroke pill of a recording: it must be in the video, so it is not excluded from capture, and it is
    /// click-through (the recorder sets that), which only makes sense over a real screen: never in an agent instance.
    Keys,
}

/// Prefix of the overlay labels (`shots-overlay-<displayId>`).
pub const OVERLAY_PREFIX: &str = "shots-overlay-";
/// Prefix of the pin labels (`shots-pin-<n>`).
pub const PIN_PREFIX: &str = "shots-pin-";

impl Surface {
    /// Window label (the overlay's takes the display id, a pin's its number).
    pub fn label(self, n: u32) -> String {
        match self {
            Surface::Overlay => format!("{OVERLAY_PREFIX}{n}"),
            Surface::Quick => "shots-quick".into(),
            Surface::Hud => "shots-hud".into(),
            Surface::Editor => "shots-editor".into(),
            Surface::Pin => format!("{PIN_PREFIX}{n}"),
            Surface::Keys => "shots-keys".into(),
        }
    }

    /// The route the page lives at.
    pub fn route(self) -> &'static str {
        match self {
            Surface::Overlay => "shots/overlay",
            Surface::Quick => "shots/quick",
            Surface::Hud => "shots/hud",
            Surface::Editor => "shots/editor",
            Surface::Pin => "shots/pin",
            Surface::Keys => "shots/keys",
        }
    }

    /// Default logical size (the overlay is resized to its display, the quick stack to its cards, a pin to its image).
    pub fn size(self) -> (f64, f64) {
        match self {
            Surface::Overlay => (800.0, 600.0),
            Surface::Quick => QUICK_CARD,
            Surface::Hud => (120.0, 120.0),
            Surface::Editor => (1100.0, 760.0),
            Surface::Pin => (320.0, 240.0),
            Surface::Keys => (420.0, 64.0),
        }
    }

    /// Whether the window is a fixed part of the screen picture a capture must not contain. The editor is an
    /// ordinary document window the person may want in a screenshot, and so is a pin: it is a picture the person
    /// put on their screen on purpose. The keystroke pill is the opposite case: it exists to be in the recording.
    pub fn excluded_from_capture(self) -> bool {
        !matches!(self, Surface::Editor | Surface::Pin | Surface::Keys)
    }

    /// May this surface exist in an agent instance? The overlay covers real displays and the keystroke pill is a
    /// click-through strip over one, so never.
    pub fn allowed_in_agent_mode(self) -> bool {
        !matches!(self, Surface::Overlay | Surface::Keys)
    }
}

/// True when a window with this label must be left out of every screen capture: the overlay, the card stack, the
/// HUD and the dock pill. The editor, the pins and every other window may appear.
pub fn is_excluded_label(label: &str) -> bool {
    label == crate::dock::LABEL
        || [Surface::Quick, Surface::Hud, Surface::Editor]
            .into_iter()
            .any(|s| s.label(0) == label && s.excluded_from_capture())
        || label.starts_with(OVERLAY_PREFIX)
}

/// True for the label of a pin window (`shots-pin-<n>`).
pub fn is_pin_label(label: &str) -> bool {
    label
        .strip_prefix(PIN_PREFIX)
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// One card of the quick stack, logical points: the page lays cards out [`QUICK_CARD`]-wide with [`QUICK_GAP`]
/// between them.
pub const QUICK_CARD: (f64, f64) = (252.0, 270.0);
/// Space between two stacked cards.
pub const QUICK_GAP: f64 = 8.0;

/// The window that holds `count` cards in `corner` of `display`'s visible area, `margin` from the edges: the stack
/// grows away from the corner, so the newest card (nearest the corner) never moves when another arrives. When the
/// cards would not fit the display's height the window is clamped to it (the page scrolls inside). Returns the
/// frame (global points, y down). `count` 0 is treated as 1 (an empty stack has no window to place).
pub fn quick_stack_frame(
    display: Rect,
    count: usize,
    corner: Corner,
    card: (f64, f64),
    margin: f64,
) -> Rect {
    let n = count.max(1) as f64;
    let wanted = n * card.1 + (n - 1.0) * QUICK_GAP;
    let room = (display.h - 2.0 * margin).max(card.1.min(display.h));
    let h = wanted.min(room);
    let w = card.0.min(display.w);
    let x = match corner {
        Corner::BottomRight | Corner::TopRight => display.x + display.w - w - margin,
        Corner::BottomLeft | Corner::TopLeft => display.x + margin,
    }
    .max(display.x);
    let y = match corner {
        Corner::BottomRight | Corner::BottomLeft => display.y + display.h - h - margin,
        Corner::TopRight | Corner::TopLeft => display.y + margin,
    }
    .max(display.y);
    Rect::new(x, y, w, h)
}

/// The pin's first window size: `image` points at 100%, scaled down (never up) to fit in 80% of `display`, keeping
/// the aspect. The page does the same math (`lib/shots/pin.ts::fitSize`); this one sizes the window before the
/// page has loaded.
pub fn pin_fit(image: (f64, f64), display: Rect) -> (f64, f64) {
    let (w, h) = (image.0.max(1.0), image.1.max(1.0));
    let k = (display.w * 0.8 / w).min(display.h * 0.8 / h).min(1.0);
    ((w * k).round().max(1.0), (h * k).round().max(1.0))
}

/// Where the `n`th pin opens: centred on `display`, then each following pin 24 points down and right so a pile of
/// pins is not one window over another, wrapping after eight steps. Clamped so the window stays on the display.
pub fn pin_origin(display: Rect, size: (f64, f64), n: u32) -> (f64, f64) {
    let step = f64::from(n % 8) * 24.0;
    let x = display.x + (display.w - size.0) / 2.0 + step;
    let y = display.y + (display.h - size.1) / 2.0 + step;
    (
        x.min(display.right() - size.0).max(display.x),
        y.min(display.bottom() - size.1).max(display.y),
    )
}

/// Top-left of the recording HUD: centred horizontally just below the top of `display`, but moved out of the
/// way of the recorded `region` when it would overlap it (the HUD is excluded from the video, but it must not hide
/// what is being recorded from the person watching). It goes below the region, else above, else stays at the top.
pub fn hud_origin(display: Rect, size: (f64, f64), region: Option<Rect>) -> (f64, f64) {
    let x = display.x + (display.w - size.0) / 2.0;
    let top = (x, display.y + 28.0);
    let Some(r) = region else { return top };
    let overlaps = |y: f64| {
        let hud = Rect::new(x, y, size.0, size.1);
        hud.intersect(&r).is_some()
    };
    if !overlaps(top.1) {
        return top;
    }
    let below = r.bottom() + 12.0;
    if below + size.1 <= display.bottom() {
        return (x, below);
    }
    let above = r.y - size.1 - 12.0;
    if above >= display.y {
        return (x, above);
    }
    top
}

/// Build (hidden) the window of `surface`. The overlay is refused in an agent instance.
pub fn build(
    app: &AppHandle,
    surface: Surface,
    display: u32,
    query: &str,
) -> Result<WebviewWindow, String> {
    if !surface.allowed_in_agent_mode() && !crate::agent_mode::Mode::current().focus_allowed() {
        return Err(
            "the capture overlay and the keystroke pill cover real displays, so agent instances never build them"
                .into(),
        );
    }
    let label = surface.label(display);
    if let Some(w) = app.get_webview_window(&label) {
        return Ok(w);
    }
    let t = std::time::Instant::now();
    let (theme, script) = crate::boot::window_boot(crate::boot::load_cache().as_ref());
    let url = if query.is_empty() {
        surface.route().to_string()
    } else {
        format!("{}?{query}", surface.route())
    };
    let (w, h) = surface.size();
    let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .theme(theme)
        .initialization_script(script)
        .title("Atlas Capture")
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;
    // A capture window is a non-activating panel before it is ever ordered in: an ordinary NSWindow ordered in with
    // `makeKeyAndOrderFront` brings the whole app forward. The conversion is AppKit window work, so it runs on the
    // main thread (`build` is called from capture workers) and is waited for.
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let w = win.clone();
        let queued = app.run_on_main_thread(move || {
            if let Ok(ns) = w.ns_window() {
                crate::dock_mac::make_panel(ns);
            }
            let _ = tx.send(());
        });
        if queued.is_ok() {
            let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
        }
    }
    super::perf::mark("surface.build", t);
    Ok(win)
}

/// What moving a capture window to a new frame means in this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The user's app: size and position are set as asked.
    SizeAndPosition,
    /// An agent instance: the position is never set (a global point is a real display, and a visible agent window
    /// there ends the instance), only the size; a visible window is parked again so it stays inside the virtual one.
    SizeThenPark,
}

/// Which [`Placement`] applies. Pure, so the agent rule is tested without a window.
pub fn placement(focus_allowed: bool) -> Placement {
    if focus_allowed {
        Placement::SizeAndPosition
    } else {
        Placement::SizeThenPark
    }
}

/// What a relayout asks of a capture window. A trait so the rule in [`relayout`] is tested against a recording fake:
/// the real one ([`Live`]) is tauri's window.
trait Mover {
    fn size(&mut self, w: f64, h: f64);
    fn position(&mut self, x: f64, y: f64);
    fn visible(&self) -> bool;
    /// Put an already visible window back where an agent instance may show it (`show`).
    fn park(&mut self);
}

/// The whole relayout rule: size always, the position only where `placement` allows it.
fn relayout(placement: Placement, win: &mut impl Mover, frame: Rect) {
    win.size(frame.w, frame.h);
    match placement {
        Placement::SizeAndPosition => win.position(frame.x, frame.y),
        Placement::SizeThenPark => {
            if win.visible() {
                win.park();
            }
        }
    }
}

struct Live<'a> {
    app: &'a AppHandle,
    win: &'a WebviewWindow,
}

impl Mover for Live<'_> {
    fn size(&mut self, w: f64, h: f64) {
        let _ = self.win.set_size(tauri::LogicalSize::new(w, h));
    }
    fn position(&mut self, x: f64, y: f64) {
        let _ = self.win.set_position(tauri::LogicalPosition::new(x, y));
    }
    fn visible(&self) -> bool {
        self.win.is_visible().unwrap_or(false)
    }
    fn park(&mut self) {
        show(self.app, self.win);
    }
}

/// Move/resize `win` to `frame` (global logical points). The one entry for every relayout of a capture window after
/// it has been built (the quick stack growing, a pin zooming), so the agent-instance rule lives in one place.
/// A window that is not visible yet is placed the same way: `show` then parks it in an agent instance.
pub fn place(app: &AppHandle, win: &WebviewWindow, frame: Rect) {
    let focus_allowed = crate::agent_mode::Mode::current().focus_allowed();
    relayout(placement(focus_allowed), &mut Live { app, win }, frame);
}

/// Order `win` in. The user's app shows it; an agent instance only through the guarded virtual display path.
///
/// Capture runs on worker threads (ScreenCaptureKit blocks), and AppKit window calls trap off the main thread
/// (`park_and_show` moves the frame; incident of the first agent run: SIGTRAP in `setFrame:display:`), so the whole
/// ordering is done on the main thread and waited for.
pub fn show(app: &AppHandle, win: &WebviewWindow) {
    let t = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    let (a, w) = (app.clone(), win.clone());
    let queued = app.run_on_main_thread(move || {
        if crate::agent_mode::Mode::current().focus_allowed() {
            #[cfg(target_os = "macos")]
            if !w
                .ns_window()
                .is_ok_and(crate::dock_mac::ensure_nonactivating)
            {
                let _ = tx.send(());
                return;
            }
            let _ = w.show();
        } else {
            #[cfg(target_os = "macos")]
            if let Some(win) = a.get_window(w.label()) {
                crate::agent_display::guard_user_displays(&a);
                crate::agent_display::park_and_show(&a, &win);
            }
        }
        let _ = tx.send(());
    });
    if queued.is_ok() {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
    }
    super::perf::mark("surface.show", t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_unique_and_the_overlay_carries_its_display() {
        let a = Surface::Overlay.label(1);
        let b = Surface::Overlay.label(2);
        assert_ne!(a, b);
        assert!(a.starts_with(OVERLAY_PREFIX));
        let all = [
            Surface::Quick,
            Surface::Hud,
            Surface::Editor,
            Surface::Keys,
            Surface::Pin,
        ]
        .map(|s| s.label(0));
        let set: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(set.len(), 5);
        for l in [&a, &b] {
            assert!(is_excluded_label(l), "{l}");
        }
    }

    /// Any number of pins can exist, each with its own label, and the label never collides with another surface's.
    #[test]
    fn every_pin_has_its_own_label_in_the_pin_namespace() {
        let a = Surface::Pin.label(1);
        let b = Surface::Pin.label(2);
        assert_eq!((a.as_str(), b.as_str()), ("shots-pin-1", "shots-pin-2"));
        assert!(is_pin_label(&a) && is_pin_label(&b));
        for other in [
            Surface::Overlay.label(1),
            Surface::Quick.label(0),
            Surface::Hud.label(0),
            Surface::Editor.label(0),
            "main".to_string(),
            "shots-pin-".to_string(),
            "shots-pin-x".to_string(),
            "shots-pin-1-2".to_string(),
        ] {
            assert!(!is_pin_label(&other), "{other}");
        }
    }

    /// The capture filter excludes every surface but the editor and the pins (documents and pictures the person put
    /// on screen on purpose, which they may want in a screenshot). The overlay and the HUD must never be in a picture.
    #[test]
    fn only_the_editor_the_pins_and_the_keystroke_pill_may_appear_in_a_capture() {
        assert!(Surface::Overlay.excluded_from_capture());
        assert!(Surface::Quick.excluded_from_capture());
        assert!(Surface::Hud.excluded_from_capture());
        assert!(!Surface::Keys.excluded_from_capture());
        assert!(!Surface::Editor.excluded_from_capture());
        assert!(!Surface::Pin.excluded_from_capture());
    }

    /// The card and the HUD appear while the user works in another app: taking focus would send their next keystroke
    /// into Atlas.
    /// Which of Atlas's own windows a capture leaves out, by label. The first agent run excluded the editor too
    /// (every surface label was filtered), contradicting the rule above.
    #[test]
    fn the_capture_filter_excludes_the_overlay_card_hud_and_dock_but_not_the_editor_pins_or_main() {
        for l in [
            Surface::Overlay.label(7),
            Surface::Quick.label(0),
            Surface::Hud.label(0),
            crate::dock::LABEL.to_string(),
        ] {
            assert!(is_excluded_label(&l), "{l}");
        }
        for l in [
            Surface::Editor.label(0),
            Surface::Pin.label(0),
            Surface::Pin.label(3),
            "main".to_string(),
            "tray".to_string(),
        ] {
            assert!(!is_excluded_label(&l), "{l}");
        }
    }

    #[test]
    fn the_overlay_and_the_keystroke_pill_are_the_surfaces_an_agent_instance_may_not_have() {
        assert!(!Surface::Overlay.allowed_in_agent_mode());
        assert!(!Surface::Keys.allowed_in_agent_mode());
        for s in [Surface::Quick, Surface::Hud, Surface::Editor, Surface::Pin] {
            assert!(s.allowed_in_agent_mode(), "{s:?}");
        }
    }

    /// The HUD page is laid out for a 120 × 120 window (a 112 px pill, a 104 px countdown disc). A shorter window
    /// clips its buttons: the first agent run showed only the timer and the word "Recording" (2026-10-26).
    #[test]
    fn the_hud_window_is_as_big_as_the_page_it_shows() {
        let page = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../apps/desktop/src/routes/shots/hud/+page.svelte"),
        )
        .unwrap();
        assert!(
            page.contains("120x120"),
            "the page documents its window size"
        );
        assert_eq!(Surface::Hud.size(), (120.0, 120.0));
    }

    #[test]
    fn routes_are_the_pages_that_exist() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/routes");
        for s in [
            Surface::Overlay,
            Surface::Quick,
            Surface::Hud,
            Surface::Editor,
            Surface::Pin,
        ] {
            let page = root.join(s.route()).join("+page.svelte");
            // the editor page may land after this file; the other pages must exist
            if s != Surface::Editor {
                assert!(
                    page.exists(),
                    "{} has no page at {}",
                    s.label(1),
                    page.display()
                );
            }
        }
    }

    const D: Rect = Rect::new(0.0, 0.0, 1512.0, 982.0);
    const CARD: (f64, f64) = QUICK_CARD;

    /// One card in each corner: `margin` from the two edges of the corner, and the window is exactly one card.
    #[test]
    fn a_single_card_sits_margin_away_from_its_corner() {
        let m = 16.0;
        let f = |c| quick_stack_frame(D, 1, c, CARD, m);
        assert_eq!(
            f(Corner::BottomRight),
            Rect::new(1244.0, 696.0, 252.0, 270.0)
        );
        assert_eq!(f(Corner::BottomLeft), Rect::new(16.0, 696.0, 252.0, 270.0));
        assert_eq!(f(Corner::TopRight), Rect::new(1244.0, 16.0, 252.0, 270.0));
        assert_eq!(f(Corner::TopLeft), Rect::new(16.0, 16.0, 252.0, 270.0));
    }

    /// A second display with a negative origin (a monitor above the primary): the corner is of THAT display.
    #[test]
    fn the_stack_corner_follows_the_display_origin() {
        let second = Rect::new(1512.0, -200.0, 2560.0, 1440.0);
        let f = quick_stack_frame(second, 1, Corner::BottomRight, CARD, 16.0);
        assert_eq!(
            (f.x, f.y),
            (1512.0 + 2560.0 - 268.0, -200.0 + 1440.0 - 286.0)
        );
        let f = quick_stack_frame(second, 1, Corner::TopLeft, CARD, 16.0);
        assert_eq!((f.x, f.y), (1528.0, -184.0));
    }

    /// The stack grows away from the corner and the corner edge stays put: a bottom stack keeps its bottom edge, a top
    /// stack its top edge, so the newest card does not jump when another arrives. Height is n cards plus n-1 gaps.
    #[test]
    fn the_stack_grows_away_from_the_corner_and_keeps_that_edge_fixed() {
        let mut last = 0.0;
        for n in 1..=3usize {
            let b = quick_stack_frame(D, n, Corner::BottomRight, CARD, 16.0);
            let nf = n as f64;
            assert_eq!(b.h, nf * 270.0 + (nf - 1.0) * QUICK_GAP, "n={n}");
            assert_eq!(b.bottom(), 982.0 - 16.0, "bottom edge fixed, n={n}");
            assert!(b.h > last, "taller with every card, n={n}");
            last = b.h;
            let t = quick_stack_frame(D, n, Corner::TopLeft, CARD, 16.0);
            assert_eq!(t.y, 16.0, "top edge fixed, n={n}");
            assert_eq!(t.h, b.h);
        }
    }

    /// 1..=8 cards never leave the display's visible area, for any corner and any display: past what fits, the window
    /// is clamped to the display (minus margins) and the page scrolls inside it.
    #[test]
    fn one_to_eight_cards_never_leave_the_display_in_any_corner() {
        let displays = [
            D,
            Rect::new(-1440.0, 100.0, 1440.0, 900.0),
            Rect::new(0.0, 0.0, 800.0, 400.0),
        ];
        for d in displays {
            for c in [
                Corner::BottomRight,
                Corner::BottomLeft,
                Corner::TopRight,
                Corner::TopLeft,
            ] {
                for n in 1..=8 {
                    let f = quick_stack_frame(d, n, c, CARD, 16.0);
                    assert!(
                        f.x >= d.x && f.right() <= d.right(),
                        "{d:?} {c:?} n={n}: {f:?}"
                    );
                    assert!(
                        f.y >= d.y && f.bottom() <= d.bottom(),
                        "{d:?} {c:?} n={n}: {f:?}"
                    );
                    assert!(f.w > 0.0 && f.h > 0.0);
                }
            }
        }
        // 8 cards (2200 pt) on 982: clamped to the display height less both margins
        let f = quick_stack_frame(D, 8, Corner::BottomRight, CARD, 16.0);
        assert_eq!((f.y, f.h), (16.0, 982.0 - 32.0));
        // 3 cards (826 pt) still fit, 4 (1104 pt) do not
        assert_eq!(
            quick_stack_frame(D, 3, Corner::BottomRight, CARD, 16.0).h,
            826.0
        );
        assert_eq!(
            quick_stack_frame(D, 4, Corner::BottomRight, CARD, 16.0).h,
            950.0
        );
    }

    /// The recorder builds the keystroke pill by these values (label, route, size) and shows it in the video: it
    /// must never be filtered out of a capture by label, and must not exist in an agent instance.
    #[test]
    fn the_keystroke_pill_is_a_fixed_surface_that_appears_in_the_recording() {
        assert_eq!(Surface::Keys.label(0), "shots-keys");
        assert_eq!(
            Surface::Keys.label(9),
            "shots-keys",
            "the pill is one window, n is ignored"
        );
        assert_eq!(Surface::Keys.route(), "shots/keys");
        assert_eq!(Surface::Keys.size(), (420.0, 64.0));
        assert!(!is_excluded_label("shots-keys"));
        assert!(!is_pin_label("shots-keys"));
    }

    /// An empty stack has no window; asking for its frame must not panic or produce a negative height.
    #[test]
    fn zero_cards_is_treated_as_one() {
        assert_eq!(
            quick_stack_frame(D, 0, Corner::BottomRight, CARD, 16.0),
            quick_stack_frame(D, 1, Corner::BottomRight, CARD, 16.0)
        );
    }

    /// A display smaller than one card: the window shrinks to the display instead of hanging off it.
    #[test]
    fn a_display_smaller_than_a_card_clamps_the_window_to_the_display() {
        let tiny = Rect::new(10.0, 20.0, 200.0, 300.0);
        let f = quick_stack_frame(tiny, 3, Corner::BottomRight, CARD, 16.0);
        assert!(f.w <= 200.0 && f.h <= 300.0, "{f:?}");
        assert!(
            f.x >= 10.0 && f.y >= 20.0 && f.right() <= 210.0 && f.bottom() <= 320.0,
            "{f:?}"
        );
    }

    #[test]
    fn the_hud_stays_at_the_top_unless_it_would_cover_the_recorded_region() {
        let (x, y) = hud_origin(D, Surface::Hud.size(), None);
        assert_eq!((x, y), ((1512.0 - 120.0) / 2.0, 28.0));
        // a region far from the top: the HUD stays at the top
        assert_eq!(
            hud_origin(
                D,
                Surface::Hud.size(),
                Some(Rect::new(100.0, 400.0, 500.0, 300.0))
            ),
            (x, 28.0)
        );
        // a region under the top centre: the HUD goes below it
        let r = Rect::new(500.0, 0.0, 600.0, 300.0);
        assert_eq!(hud_origin(D, Surface::Hud.size(), Some(r)), (x, 312.0));
        // a region that runs to the bottom: the HUD goes above... there is no room above at y=0, so back to the top
        let tall = Rect::new(500.0, 0.0, 600.0, 982.0);
        assert_eq!(hud_origin(D, Surface::Hud.size(), Some(tall)), (x, 28.0));
        // a region that touches the bottom but starts lower: above it
        let low = Rect::new(500.0, 200.0, 600.0, 782.0);
        let (_, y) = hud_origin(D, Surface::Hud.size(), Some(low));
        assert_eq!(y, 28.0, "28..80 does not overlap a region starting at 200");
    }

    #[test]
    fn the_hud_goes_above_a_region_that_fills_the_bottom_when_there_is_room() {
        let r = Rect::new(500.0, 20.0, 600.0, 962.0);
        // top overlaps (28..80 inside 20..), below does not fit (982+12+52 > 982): above needs y >= 0: 20-52-12 < 0
        let (_, y) = hud_origin(D, Surface::Hud.size(), Some(r));
        assert_eq!(y, 28.0, "no room above or below: stays at the top");
        let r = Rect::new(500.0, 300.0, 600.0, 682.0);
        // here the top does not overlap at all
        assert_eq!(hud_origin(D, Surface::Hud.size(), Some(r)).1, 28.0);
    }

    /// A pin opens at 100% logical size; a screenshot bigger than 80% of the display is scaled down to fit, keeping
    /// its aspect, and a small one is never scaled up.
    #[test]
    fn a_pin_opens_at_one_to_one_unless_it_is_bigger_than_80_percent_of_the_display() {
        assert_eq!(pin_fit((400.0, 300.0), D), (400.0, 300.0));
        // a full-display screenshot (1512 x 982 points) is scaled to 80%
        assert_eq!(pin_fit((1512.0, 982.0), D), (1210.0, 786.0));
        // a wide image is limited by width, keeping its aspect
        assert_eq!(pin_fit((3000.0, 300.0), D), (1210.0, 121.0));
        // never zero, even for a degenerate image
        assert_eq!(pin_fit((0.0, 0.0), D), (1.0, 1.0));
    }

    /// Pins opened one after another are staggered, never exactly on top of each other, and always on the display.
    #[test]
    fn pins_are_staggered_and_stay_on_the_display() {
        let size = (400.0, 300.0);
        let first = pin_origin(D, size, 0);
        assert_eq!(first, ((1512.0 - 400.0) / 2.0, (982.0 - 300.0) / 2.0));
        let second = pin_origin(D, size, 1);
        assert_eq!((second.0 - first.0, second.1 - first.1), (24.0, 24.0));
        // wraps after eight steps
        assert_eq!(pin_origin(D, size, 8), first);
        // a window as big as the display cannot move off it
        assert_eq!(pin_origin(D, (1512.0, 982.0), 5), (0.0, 0.0));
        for n in 0..20 {
            let (x, y) = pin_origin(D, (1200.0, 800.0), n);
            assert!(
                x >= 0.0 && x + 1200.0 <= 1512.0 && y >= 0.0 && y + 800.0 <= 982.0,
                "n={n}"
            );
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    /// A window that records what a relayout asked of it.
    #[derive(Default)]
    struct Recorder {
        visible: bool,
        calls: Vec<String>,
    }

    impl Mover for Recorder {
        fn size(&mut self, w: f64, h: f64) {
            self.calls.push(format!("size {w}x{h}"));
        }
        fn position(&mut self, x: f64, y: f64) {
            self.calls.push(format!("position {x},{y}"));
        }
        fn visible(&self) -> bool {
            self.visible
        }
        fn park(&mut self) {
            self.calls.push("park".into());
        }
    }

    const FRAME: Rect = Rect::new(1500.0, 40.0, 252.0, 540.0);

    fn relaid(focus_allowed: bool, visible: bool) -> Vec<String> {
        let mut w = Recorder {
            visible,
            ..Recorder::default()
        };
        relayout(placement(focus_allowed), &mut w, FRAME);
        w.calls
    }

    /// Incident class (an agent window on the user's display): relayout of a capture window in an agent instance must
    /// never set a position (a global point is a real display, and a visible agent window there ends the instance);
    /// only the user's app may. Visible or not, whatever the frame.
    #[test]
    fn an_agent_instance_never_sets_a_position() {
        for visible in [false, true] {
            let calls = relaid(false, visible);
            assert!(
                !calls.iter().any(|c| c.starts_with("position")),
                "visible={visible}: {calls:?}"
            );
            assert_eq!(calls[0], "size 252x540");
        }
    }

    /// A visible agent window is parked again after a resize so it stays inside the virtual display; one that is not
    /// shown yet is left for `show` to park.
    #[test]
    fn an_agent_window_is_parked_again_only_when_it_is_visible() {
        assert_eq!(relaid(false, true), ["size 252x540", "park"]);
        assert_eq!(relaid(false, false), ["size 252x540"]);
    }

    /// The user's app is placed exactly where it was asked to be, size first.
    #[test]
    fn the_users_app_gets_its_size_and_its_position() {
        for visible in [false, true] {
            assert_eq!(relaid(true, visible), ["size 252x540", "position 1500,40"]);
        }
    }

    #[test]
    fn the_agent_rule_follows_the_focus_gate() {
        assert_eq!(placement(true), Placement::SizeAndPosition);
        assert_eq!(placement(false), Placement::SizeThenPark);
    }
}
