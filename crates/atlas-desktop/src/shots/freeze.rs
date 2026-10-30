//! The frozen screen a capture session works from.
//!
//! When the overlay opens, every display is frozen once (ScreenCaptureKit straight to RGBA). That frame is what the
//! person looks at, what the magnifier reads, and what a region or a whole-screen pick saves: the pixels they saw
//! when they let go of the mouse, cropped in memory, never a new capture taken after the release (which would show
//! whatever the screen turned into while Atlas was hiding the overlay, listing windows and starting ScreenCaptureKit).
//!
//! Everything here is pure (no OS): the crop, the slot a commit waits on when it arrives before the freeze is
//! ready, and the plan for which overlay windows exist. Tested on every platform.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

use super::geom::{self, Rect};
use super::pixels::{self, Rgba};
use super::session::{Effect, Still};

/// One display's frozen pixels, tight RGBA (opaque), on the display's own pixel grid.
pub struct Frozen {
    pub rgba: Vec<u8>,
    pub size: (u32, u32),
}

/// The frozen frames of one session by display id. Cheap to clone: the pixels are shared.
pub type Frames = HashMap<u32, Arc<Frozen>>;

/// What one session froze: the pixels and the displays they were taken from (frame in global points and backing
/// scale), so a crop is always read against the geometry the frame had, whatever the displays are doing now.
#[derive(Clone)]
pub struct Session {
    pub frames: Frames,
    pub displays: Vec<Disp>,
}

/// Where a freeze is published and where a commit waits for it. The freeze runs in parallel with the overlay coming
/// on screen, so a very fast release can reach Rust first: it waits here (on the capture worker, never the main
/// thread) instead of taking a second capture. A capture with no overlay behind it (previous area, the full-screen
/// key) has no session: it is told so at once instead of waiting.
#[derive(Default)]
pub struct Slot {
    state: Mutex<Freeze>,
    ready: Condvar,
}

#[derive(Default)]
enum Freeze {
    /// No overlay session is open.
    #[default]
    Closed,
    /// An overlay session is open and its freeze is being taken.
    Pending,
    Ready(Session),
}

impl Slot {
    /// A new overlay session starts: whatever the last one froze is gone and commits wait for the next freeze.
    pub fn begin(&self) {
        *self.state.lock() = Freeze::Pending;
    }

    /// The freeze is done: wake every commit waiting for it. Dropped when its session has already closed.
    pub fn publish(&self, session: Session) {
        let mut g = self.state.lock();
        if matches!(*g, Freeze::Pending) {
            *g = Freeze::Ready(session);
            self.ready.notify_all();
        }
    }

    /// The session now, or `None` when it is not frozen yet (or there is none).
    pub fn peek(&self) -> Option<Session> {
        match &*self.state.lock() {
            Freeze::Ready(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// The session, waiting up to `limit` while its freeze is being taken. `None` at once when no session is open,
    /// and on a timeout (the freeze failed or never started): the caller then captures live or says so.
    pub fn wait(&self, limit: Duration) -> Option<Session> {
        let mut g = self.state.lock();
        if matches!(*g, Freeze::Pending) {
            let _ = self.ready.wait_for(&mut g, limit);
        }
        match &*g {
            Freeze::Ready(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// The session ended (the capture took what it needs, or it was cancelled): drop the frames and wake any waiter
    /// so it does not sit out its limit.
    pub fn clear(&self) {
        *self.state.lock() = Freeze::Closed;
        self.ready.notify_all();
    }
}

/// The pixels of a whole display, as the freeze holds them.
pub fn whole(f: &Frozen) -> Rgba {
    Rgba {
        data: f.rgba.clone(),
        size: f.size,
    }
}

/// The pixels of `region` (global points) out of the frozen frame of the display at `display` with backing `scale`.
/// A region that runs off the display is clipped to it; one entirely outside it is an error. The pixel rectangle is
/// the one a live capture of the same region would have (`geom::to_pixels`), so the two agree.
pub fn crop(f: &Frozen, region: Rect, display: Rect, scale: f64) -> Result<Rgba, String> {
    let (x, y, w, h) =
        geom::to_pixels(region, display, scale).ok_or("That region is outside the display.")?;
    // The freeze is the display's pixel grid. If the mode changed since it was taken it is smaller: clip, don't fail.
    let w = w.min(f.size.0.saturating_sub(x));
    let h = h.min(f.size.1.saturating_sub(y));
    let data = pixels::block(&f.rgba, f.size, x, y, w, h)?;
    Ok(Rgba { data, size: (w, h) })
}
/// What a display is, as the freeze needs it: its id, frame in global points and backing scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Disp {
    pub id: u32,
    pub frame: Rect,
    pub scale: f64,
}

/// The pixels of a pick, cut from the frozen frames, or `Ok(None)` when the freeze cannot stand in for it: a window
/// (its own pixels and alpha, for the shadow and the rounded corners) and the fullscreen key (no overlay was up, so
/// nothing was frozen; the instant of the key press is the instant to capture). `one_x` asks for one pixel per
/// point: the retina pixels are box-averaged down, as a live 1× capture would have them.
pub fn still_of(s: &Session, still: &Still, one_x: bool) -> Result<Option<Rgba>, String> {
    let (display, region) = match still {
        Still::Region { rect, display } => (*display, Some(*rect)),
        Still::Display { display } => (*display, None),
        Still::Window { .. } | Still::Screen => return Ok(None),
    };
    let d = s
        .displays
        .iter()
        .find(|d| d.id == display)
        .ok_or("that display is gone")?;
    let f = s
        .frames
        .get(&display)
        .ok_or("that display has no frozen frame")?;
    let img = match region {
        Some(r) => crop(f, r, d.frame, d.scale)?,
        None => whole(f),
    };
    if !one_x || d.scale <= 1.0 {
        return Ok(Some(img));
    }
    let dst = (
        ((f64::from(img.size.0) / d.scale).round() as u32).max(1),
        ((f64::from(img.size.1) / d.scale).round() as u32).max(1),
    );
    Ok(Some(Rgba {
        data: super::gif::downscale(&img.data, img.size, dst),
        size: dst,
    }))
}

/// Which overlay windows a session needs: the displays now present, and which of those have no window yet.
#[derive(Debug, PartialEq, Eq)]
pub struct OverlayPlan {
    /// Displays that need a window built (none on a warm session).
    pub build: Vec<u32>,
    /// Displays whose window is ordered in now, in the order given.
    pub show: Vec<u32>,
}

/// Overlay windows are built once and reused: `existing` are the displays that already have a (hidden) window,
/// `displays` the ones on screen now. A window of a display that is gone stays hidden, ready for when it returns.
pub fn overlay_plan(existing: &[u32], displays: &[u32]) -> OverlayPlan {
    OverlayPlan {
        build: displays
            .iter()
            .copied()
            .filter(|d| !existing.contains(d))
            .collect(),
        show: displays.to_vec(),
    }
}

/// Split the effects of one step: hiding the overlay runs at once on the thread that got the event (the release
/// must clear the screen in the same event-loop turn, not after a worker has been scheduled and ScreenCaptureKit
/// answered), everything else on the capture worker, in order.
pub fn split_effects(effects: Vec<Effect>) -> (Vec<Effect>, Vec<Effect>) {
    effects
        .into_iter()
        .partition(|e| matches!(e, Effect::HideOverlay))
}

/// True for the effects a person is waiting on the instant of: the overlay coming up (hotkey) and a still being cut,
/// encoded and written (release). Only these run at [`super::mac::Qos::Interactive`]; recording, GIF encoding, text
/// recognition and the settings write run at the default class.
pub fn is_instant(effect: &Effect) -> bool {
    matches!(effect, Effect::ShowOverlay(_) | Effect::Still(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A display of `w`×`h` pixels whose pixel `(x, y)` is `[x, y, 0, 255]` (mod 256): every pixel names itself.
    fn frame(w: u32, h: u32) -> Frozen {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        Frozen { rgba, size: (w, h) }
    }

    const D: Rect = Rect::new(0.0, 0.0, 100.0, 50.0);

    fn px(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.size.0 + x) * 4) as usize;
        [
            img.data[i],
            img.data[i + 1],
            img.data[i + 2],
            img.data[i + 3],
        ]
    }

    #[test]
    fn a_region_is_cut_out_of_the_frozen_pixels_it_names() {
        let f = frame(100, 50);
        let img = crop(&f, Rect::new(10.0, 20.0, 30.0, 10.0), D, 1.0).unwrap();
        assert_eq!(img.size, (30, 10));
        assert_eq!(px(&img, 0, 0), [10, 20, 0, 255]);
        assert_eq!(px(&img, 29, 9), [39, 29, 0, 255]);
    }

    /// A retina display freezes at twice its points: the same region is twice as many pixels, from the same place.
    #[test]
    fn a_region_on_a_retina_display_is_cut_at_the_backing_scale() {
        let f = frame(200, 100);
        let img = crop(&f, Rect::new(10.0, 20.0, 30.0, 10.0), D, 2.0).unwrap();
        assert_eq!(img.size, (60, 20));
        assert_eq!(px(&img, 0, 0), [20, 40, 0, 255]);
    }

    /// A second display sits at an offset in global points; its frozen frame starts at its own origin.
    #[test]
    fn a_region_on_a_display_that_is_not_at_the_origin_is_cut_relative_to_it() {
        let f = frame(100, 50);
        let d2 = Rect::new(1000.0, -200.0, 100.0, 50.0);
        let img = crop(&f, Rect::new(1010.0, -190.0, 20.0, 10.0), d2, 1.0).unwrap();
        assert_eq!(px(&img, 0, 0), [10, 10, 0, 255]);
    }

    #[test]
    fn a_region_that_runs_off_the_display_is_clipped_and_one_outside_it_is_refused() {
        let f = frame(100, 50);
        let img = crop(&f, Rect::new(90.0, 40.0, 40.0, 40.0), D, 1.0).unwrap();
        assert_eq!(img.size, (10, 10));
        assert_eq!(px(&img, 9, 9), [99, 49, 0, 255]);
        assert!(crop(&f, Rect::new(500.0, 500.0, 10.0, 10.0), D, 1.0).is_err());
    }

    /// The display's mode changed after the freeze: a smaller frame is cropped to what it has, not a failed capture.
    #[test]
    fn a_freeze_smaller_than_the_display_is_cropped_to_what_it_has() {
        let f = frame(50, 25);
        let img = crop(&f, Rect::new(40.0, 20.0, 40.0, 20.0), D, 1.0).unwrap();
        assert_eq!(img.size, (10, 5));
    }

    /// The point of the freeze: what is saved does not change when the screen does. The crop owns its pixels.
    #[test]
    fn the_saved_pixels_are_the_frozen_ones_whatever_happens_to_the_frame_afterwards() {
        let mut f = frame(100, 50);
        let img = crop(&f, Rect::new(0.0, 0.0, 10.0, 10.0), D, 1.0).unwrap();
        f.rgba.fill(7);
        assert_eq!(px(&img, 3, 4), [3, 4, 0, 255]);
        let all = whole(&f);
        f.rgba.fill(9);
        assert_eq!(all.data[0], 7);
    }

    fn frames_of(id: u32) -> Session {
        let mut m = Frames::new();
        m.insert(id, Arc::new(frame(4, 4)));
        Session {
            frames: m,
            displays: vec![Disp {
                id,
                frame: Rect::new(0.0, 0.0, 4.0, 4.0),
                scale: 1.0,
            }],
        }
    }

    /// A release that beats the freeze waits for it and gets those frames: no second capture is ever taken.
    #[test]
    fn a_commit_that_arrives_before_the_freeze_waits_for_it() {
        let slot = Arc::new(Slot::default());
        slot.begin();
        let s = slot.clone();
        let waiter = std::thread::spawn(move || s.wait(Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        slot.publish(frames_of(7));
        let got = waiter.join().unwrap().expect("woken by the publish");
        assert!(got.frames.contains_key(&7));
    }

    #[test]
    fn a_wait_for_a_freeze_that_never_comes_gives_up_after_its_limit() {
        let slot = Slot::default();
        slot.begin();
        let t = std::time::Instant::now();
        assert!(slot.wait(Duration::from_millis(60)).is_none());
        assert!(t.elapsed() >= Duration::from_millis(50));
    }

    /// "Capture previous area" has no overlay and so no session: asking the slot answers at once, it must never sit
    /// out the wait limit (it did: the capture then failed after five seconds).
    #[test]
    fn with_no_session_open_a_wait_answers_none_at_once() {
        let slot = Slot::default();
        let t = std::time::Instant::now();
        assert!(slot.wait(Duration::from_secs(5)).is_none());
        slot.begin();
        slot.publish(frames_of(1));
        slot.clear();
        assert!(slot.wait(Duration::from_secs(5)).is_none());
        assert!(t.elapsed() < Duration::from_millis(500));
    }

    /// A freeze that finishes after its session was closed (Esc, the capture already taken) is dropped: it must not
    /// bring 24 MB per display back to life, nor feed the next capture.
    #[test]
    fn a_freeze_that_lands_after_its_session_closed_is_dropped() {
        let slot = Slot::default();
        slot.begin();
        slot.clear();
        slot.publish(frames_of(1));
        assert!(slot.peek().is_none());
        assert!(slot.wait(Duration::from_millis(10)).is_none());
    }

    #[test]
    fn a_published_freeze_answers_at_once_and_a_new_session_forgets_it() {
        let slot = Slot::default();
        slot.begin();
        slot.publish(frames_of(1));
        let t = std::time::Instant::now();
        assert!(slot.wait(Duration::from_secs(5)).is_some());
        assert!(t.elapsed() < Duration::from_millis(500));
        slot.begin();
        assert!(slot.peek().is_none());
        assert!(slot.wait(Duration::from_millis(10)).is_none());
    }

    /// The frames a capture took stay valid when the next session replaces the slot's: they are shared, not borrowed.
    #[test]
    fn frames_taken_by_a_capture_survive_the_next_session() {
        let slot = Slot::default();
        slot.begin();
        slot.publish(frames_of(1));
        let mine = slot.peek().unwrap();
        slot.begin();
        slot.publish(frames_of(2));
        assert!(mine.frames.contains_key(&1) && !mine.frames.contains_key(&2));
        assert_eq!(
            mine.displays[0].id, 1,
            "the geometry travels with the pixels"
        );
    }
}

#[cfg(test)]
mod plan_tests {
    use super::super::session::{Effect, Pick, Still};
    use super::*;

    /// Overlay windows are built once and reused: a capture builds only the displays that have none yet.
    #[test]
    fn only_displays_without_an_overlay_window_get_one_built() {
        let p = overlay_plan(&[1, 2], &[1, 2, 3]);
        assert_eq!(p.build, [3]);
        assert_eq!(p.show, [1, 2, 3]);
        let p = overlay_plan(&[1, 2, 3], &[1, 2, 3]);
        assert!(p.build.is_empty(), "a warm session builds nothing");
        assert_eq!(p.show, [1, 2, 3]);
    }

    /// A display that was unplugged keeps its (hidden) window for the next time but is not shown now.
    #[test]
    fn a_display_that_is_gone_is_not_shown_and_not_rebuilt() {
        let p = overlay_plan(&[1, 2], &[2]);
        assert!(p.build.is_empty());
        assert_eq!(p.show, [2]);
    }

    /// The overlay must be gone the instant the person releases: hiding runs on the caller's thread, ahead of
    /// everything the worker does (ScreenCaptureKit, encoding, the clipboard).
    #[test]
    fn hiding_the_overlay_runs_inline_and_the_rest_on_the_worker_in_order() {
        let still = Effect::Still(Still::Screen);
        let (inline, worker) = split_effects(vec![
            Effect::HideOverlay,
            still.clone(),
            Effect::StartCountdown,
        ]);
        assert_eq!(inline, [Effect::HideOverlay]);
        assert_eq!(worker, [still, Effect::StartCountdown]);
        let (inline, worker) = split_effects(vec![Effect::ShowOverlay(Pick::Region)]);
        assert!(inline.is_empty());
        assert_eq!(worker, [Effect::ShowOverlay(Pick::Region)]);
    }

    /// Showing the overlay is NOT inline: it starts a freeze that must not run on the main thread or a webview's.
    #[test]
    fn only_hiding_is_inline() {
        for e in [
            Effect::ShowOverlay(Pick::All),
            Effect::SetOverlayMode(Pick::Window),
            Effect::Still(Still::Screen),
            Effect::Text(Still::Screen),
            Effect::StopRecording,
            Effect::HideHud,
        ] {
            assert!(split_effects(vec![e.clone()]).0.is_empty(), "{e:?}");
        }
    }

    /// Which effects get the interactive scheduler class: what the person waits on the instant of (the overlay at the
    /// hotkey, the still at the release), and nothing that runs long or can wait (recording, GIF, text, the HUD).
    #[test]
    fn only_the_overlay_and_the_still_are_instant() {
        for e in [
            Effect::ShowOverlay(Pick::Region),
            Effect::Still(Still::Screen),
            Effect::Still(Still::Window { id: 3 }),
        ] {
            assert!(is_instant(&e), "{e:?}");
        }
        for e in [
            Effect::Text(Still::Screen),
            Effect::SetOverlayMode(Pick::Window),
            Effect::HideOverlay,
            Effect::StartCountdown,
            Effect::StartRecording {
                display: 1,
                rect: None,
                gif: true,
            },
            Effect::PauseRecording,
            Effect::ResumeRecording,
            Effect::StopRecording,
            Effect::DiscardRecording,
            Effect::HideHud,
        ] {
            assert!(!is_instant(&e), "{e:?}");
        }
    }
}

#[cfg(test)]
mod still_tests {
    use super::*;
    use crate::shots::session::Still;

    fn frame(w: u32, h: u32) -> Frozen {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        Frozen { rgba, size: (w, h) }
    }

    /// A retina display 1 (100×50 points, 200×100 pixels) and a 1× display 2 to its right.
    fn world() -> Session {
        let mut frames = Frames::new();
        frames.insert(1, Arc::new(frame(200, 100)));
        frames.insert(2, Arc::new(frame(100, 50)));
        Session {
            frames,
            displays: vec![
                Disp {
                    id: 1,
                    frame: Rect::new(0.0, 0.0, 100.0, 50.0),
                    scale: 2.0,
                },
                Disp {
                    id: 2,
                    frame: Rect::new(100.0, 0.0, 100.0, 50.0),
                    scale: 1.0,
                },
            ],
        }
    }

    fn region(display: u32, x: f64, y: f64, w: f64, h: f64) -> Still {
        Still::Region {
            rect: Rect::new(x, y, w, h),
            display,
        }
    }

    #[test]
    fn a_region_pick_is_cut_from_its_displays_frozen_frame() {
        let img = still_of(&world(), &region(1, 10.0, 10.0, 20.0, 10.0), false)
            .unwrap()
            .unwrap();
        assert_eq!(img.size, (40, 20), "retina: twice the points");
        assert_eq!(&img.data[..4], &[20, 20, 0, 255]);
    }

    /// The second display is read from its own frame at its own offset, not from the first one's pixels.
    #[test]
    fn a_region_on_the_second_display_is_cut_from_the_second_frame() {
        let img = still_of(&world(), &region(2, 110.0, 5.0, 10.0, 10.0), false)
            .unwrap()
            .unwrap();
        assert_eq!(img.size, (10, 10));
        assert_eq!(&img.data[..4], &[10, 5, 0, 255]);
    }

    /// "1×" asks for one pixel per point: the same region, box-averaged down from the retina freeze.
    #[test]
    fn the_one_x_setting_gives_one_pixel_per_point() {
        let img = still_of(&world(), &region(1, 10.0, 10.0, 20.0, 10.0), true)
            .unwrap()
            .unwrap();
        assert_eq!(img.size, (20, 10));
    }

    #[test]
    fn a_screen_pick_is_the_whole_frozen_display() {
        let img = still_of(&world(), &Still::Display { display: 2 }, false)
            .unwrap()
            .unwrap();
        assert_eq!(img.size, (100, 50));
        let img = still_of(&world(), &Still::Display { display: 1 }, true)
            .unwrap()
            .unwrap();
        assert_eq!(img.size, (100, 50), "1x of a retina display is its points");
    }

    /// A window pick needs the window's own pixels and alpha: the freeze cannot stand in for it.
    #[test]
    fn a_window_pick_and_a_fullscreen_key_are_not_answered_from_the_freeze() {
        let w = world();
        assert!(
            still_of(&w, &Still::Window { id: 9 }, false)
                .unwrap()
                .is_none()
        );
        assert!(still_of(&w, &Still::Screen, false).unwrap().is_none());
    }

    #[test]
    fn a_display_with_no_frozen_frame_or_a_region_outside_it_is_an_error() {
        let w = world();
        assert!(still_of(&w, &Still::Display { display: 7 }, false).is_err());
        assert!(still_of(&w, &region(1, 900.0, 900.0, 10.0, 10.0), false).is_err());
    }
}
