//! The per-terminal render thread: sleeps until new bytes, a resize, a blink/bell deadline or
//! the watchdog's next check, then snapshots the VT and draws it.
//!
//! Locking invariant (#108): nothing in here takes `TERMS`. The thread owns its renderer and
//! reaches the rest of the world only through its own `Shared` (a per-terminal mutex held for a
//! few field reads, never across a draw) and the callbacks it is given. A renderer that blocks
//! (`get_current_texture` waits for vsync, a wedged Metal layer waits forever) therefore blocks
//! this tab's thread and nothing else: not the main thread, not the registry, not other tabs.
//! Recovery is driven from here too (see `watchdog`): a frame that is owed but not presented is
//! drawn again, and after ~2 s the surface is rebuilt by `rebuild`.

use std::sync::Arc;
use std::time::Instant;

use atlas_term::{FontResolution, FontSpec, Frame, Renderer};

use super::blink::Blink;
use super::release::Release;
use super::shared::Shared;
use super::watchdog::{Action, Watchdog, Why};

/// The shortest time between two draws of one terminal: one display refresh (60 Hz floor; a faster panel only
/// loses the ability to draw twice in the time it can show one). Output marks the screen dirty; the frame that
/// follows is the latest state (Ghostty draws on the display link, not per read).
pub(super) const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_micros(16_667);

/// When the next frame may be drawn after the last one at `last`: `None` = now (an idle terminal draws at once, so
/// typing never waits a refresh); otherwise one refresh interval after the last frame.
pub(super) fn pace_until(last: Instant, now: Instant) -> Option<Instant> {
    let ready = last + FRAME_INTERVAL;
    (now < ready).then_some(ready)
}

/// Selection highlight: #264F78 at ~67% opacity.
const SELECTION: [u8; 4] = [38, 79, 120, 170];
/// Search matches (dim amber) and the current match (strong orange), alpha-blended.
const SEARCH: [u8; 4] = [255, 200, 0, 90];
const SEARCH_CURRENT: [u8; 4] = [255, 140, 0, 200];

/// What the loop needs from a renderer; `Renderer` is the real one, tests supply a fake.
pub(super) trait Present {
    fn set_ligatures(&mut self, on: bool);
    fn font_spec(&self) -> &FontSpec;
    fn set_font(&mut self, pt: f32, spec: &FontSpec) -> ((u16, u16), FontResolution);
    fn resize(&mut self, w: u32, h: u32) -> (u16, u16);
    fn pixel_size(&self) -> (u32, u32);
    fn cell_px(&self) -> [f32; 2];
    fn stats(&self) -> atlas_term::Stats;
    fn reset_stats(&mut self);
    /// `Ok(true)`: a frame was presented. `Ok(false)`: the surface had no texture.
    fn render(&mut self, f: &Frame) -> Result<bool, String>;
}

impl Present for Renderer {
    fn set_ligatures(&mut self, on: bool) {
        Renderer::set_ligatures(self, on)
    }
    fn font_spec(&self) -> &FontSpec {
        Renderer::font_spec(self)
    }
    fn set_font(&mut self, pt: f32, spec: &FontSpec) -> ((u16, u16), FontResolution) {
        Renderer::set_font(self, pt, spec)
    }
    fn resize(&mut self, w: u32, h: u32) -> (u16, u16) {
        Renderer::resize(self, w, h)
    }
    fn pixel_size(&self) -> (u32, u32) {
        Renderer::pixel_size(self)
    }
    fn cell_px(&self) -> [f32; 2] {
        Renderer::cell_px(self)
    }
    fn stats(&self) -> atlas_term::Stats {
        Renderer::stats(self)
    }
    fn reset_stats(&mut self) {
        Renderer::reset_stats(self)
    }
    fn render(&mut self, f: &Frame) -> Result<bool, String> {
        Renderer::render(self, f)
    }
}

/// Told when the font request resolved to a family other than the one asked for.
pub(super) type FontHook = Box<dyn Fn(&FontSpec, &FontResolution) + Send>;
/// Replaces a renderer by one at `(physical w, h)`; `Err` gives the old one back, boxed so the
/// failure path stays small (a `Renderer` is kilobytes).
pub(super) type RebuildHook<R> = Box<dyn FnMut(R, (u32, u32), Why) -> Result<R, Box<R>> + Send>;
/// Records an incident: `(kind, detail)`.
pub(super) type IncidentHook = Box<dyn Fn(&str, &str) + Send>;

/// The side effects of a terminal's render thread, so the loop itself is plain logic.
pub(super) struct Hooks<R> {
    pub(super) on_resize: Box<dyn Fn(u16, u16) + Send>,
    pub(super) on_font: FontHook,
    /// A frame is on the layer: the native view may be shown (see `Placement`).
    pub(super) on_first_frame: Box<dyn FnMut() + Send>,
    /// Build a replacement surface and renderer at `(physical w, h)` (a new native view, so it
    /// runs on the main thread) and retire the old one, which it drops before releasing the old
    /// view (a surface must be gone before its view). `Err` gives the old renderer back (boxed):
    /// the replacement could not be made.
    pub(super) rebuild: RebuildHook<R>,
    /// Build a renderer on the terminal's CURRENT view at `(physical w, h)`, after the loop released the old one
    /// (#254). The font, scale and ligatures come from the terminal's state. `Err` is why not: the loop retries.
    pub(super) acquire: AcquireHook<R>,
    /// How long a hidden view keeps its renderer; `None` never releases it (`release::ENABLED`).
    pub(super) release_after: Option<std::time::Duration>,
    /// An incident for the diagnostics report: `(kind detail)`.
    pub(super) incident: IncidentHook,
}

/// Builds a renderer on the terminal's own view at `(physical w, h)`.
pub(super) type AcquireHook<R> = Box<dyn FnMut((u32, u32)) -> Result<R, String> + Send>;

pub(super) fn render_loop<R: Present>(first: R, sh: Arc<Shared>, mut hooks: Hooks<R>) {
    // `None` while the renderer is released (a view hidden for `release_after`): the GPU holds nothing for this
    // terminal and geometry, font and ligature requests wait in `State` until the view is shown and one is built.
    let mut r: Option<R> = Some(first);
    // The surface size of the renderer that was released, for the one that replaces it.
    let mut last_px = (1, 1);
    // The ligature setting last applied, re-applied to a renderer built after a release.
    let mut ligatures_cur: Option<bool> = None;
    let mut release = Release::new(hooks.release_after);
    let mut first_frame_pending = true;
    let mut pal = sh.state.lock().pal.clone();
    let mut frame = Frame::default();
    let mut blink = Blink::new(Instant::now());
    let mut dog = Watchdog::default();
    // A rebuild was recorded and no frame has been presented since: say so when one is.
    let mut stall_logged = false;
    // Output arrived while the view was parked: draw once when it is shown again.
    let mut stale = false;
    // When this terminal last started a draw: the next one waits for a refresh interval after it.
    let mut last_draw: Option<Instant> = None;
    loop {
        let (size, font_pt, font_new, ligatures, focused, bell, forced, demand) = {
            let mut s = sh.state.lock();
            loop {
                // A parked view has no blink or bell to show; waking for them is pure waste.
                let hidden = !first_frame_pending && !sh.placement.lock().is_shown();
                // Hidden with no renderer: output, geometry and fonts wait for the show; only `closed` and a
                // rebuild request (which is dropped below) end the wait.
                let idle = hidden && r.is_none();
                if s.closed || s.rebuild.is_some() || (!idle && (s.size.is_some() || s.dirty)) {
                    // New output or input: restart the blink cycle visible.
                    blink.restart(Instant::now());
                    break;
                }
                let now = Instant::now();
                release.observe(hidden, now);
                let release_at = r.as_ref().and_then(|_| release.deadline());
                let blink_at = blink.deadline(frame.cursor_blinks, hidden);
                let bell = s.bell_until.filter(|_| !hidden);
                let next = [blink_at, bell, release_at, dog.next_check(now)]
                    .into_iter()
                    .flatten()
                    .min();
                let Some(next) = next else {
                    sh.wake.wait(&mut s);
                    continue;
                };
                if !sh.wake.wait_until(&mut s, next).timed_out() {
                    continue;
                }
                let now = Instant::now();
                let mut owed = false;
                if s.bell_until.is_some_and(|b| b <= now) {
                    s.bell_until = None;
                    owed = true;
                }
                if frame.cursor_blinks {
                    owed |= blink.tick(now);
                }
                owed |= r.is_some() && release.due(now);
                // A timer that changed nothing (the blink came due while resting, a watchdog check with no frame
                // owed) draws nothing: the watchdog's own `poll` below decides whether a redraw is due.
                if owed || dog.next_check(now).is_some_and(|c| c <= now) {
                    break;
                }
                continue;
            }
            if s.closed {
                return;
            }
            let hidden = !first_frame_pending && !sh.placement.lock().is_shown();
            if r.is_none() && hidden {
                // Nothing to draw with and nothing to show: take only the rebuild request (there is no surface to
                // rebuild; the one built on show is fresh) and leave the rest for then.
                s.rebuild = None;
                (None, None, None, None, s.focused, false, None, false)
            } else {
                // One frame per display refresh, always of the latest state: output that arrives while we wait just
                // marks the screen dirty again (it is drawn by the frame that ends the wait). Geometry, a rebuild and
                // `closed` are not paced.
                if let Some(t) = last_draw
                    && s.rebuild.is_none()
                    && s.size.is_none()
                    && s.font_pt.is_none()
                    && s.font_new.is_none()
                {
                    let ready = pace_until(t, Instant::now()).unwrap_or_else(Instant::now);
                    while Instant::now() < ready
                        && !s.closed
                        && s.rebuild.is_none()
                        && s.size.is_none()
                    {
                        sh.wake.wait_until(&mut s, ready);
                    }
                    if s.closed {
                        return;
                    }
                }
                if let Some(r) = r.as_mut()
                    && std::mem::take(&mut s.reset)
                {
                    r.reset_stats();
                }
                let demand = std::mem::take(&mut s.dirty);
                if std::mem::take(&mut s.pal_new) {
                    pal = s.pal.clone();
                }
                (
                    s.size.take(),
                    s.font_pt.take(),
                    s.font_new.take(),
                    s.ligatures.take(),
                    s.focused,
                    s.bell_until.is_some(),
                    s.rebuild.take(),
                    demand,
                )
            }
        };
        let now = Instant::now();
        // A view parked by the page (background tab or worktree, dialog open, space moving) is
        // invisible: drawing it only queues vsync-blocked work on the one shared GPU queue, once
        // per byte burst per tab (many streaming agents made that the freeze). Its first frame
        // still draws (the view is shown by it), and the watchdog stands down: nothing is owed.
        // Shown again after being parked: the view waits off screen for a fresh frame (`Placement::request`), so
        // one is drawn now and the hook puts the view over its pane, like a first frame.
        if sh.placement.lock().awaiting() {
            first_frame_pending = true;
            stale = true;
        }
        let parked = !first_frame_pending && !sh.placement.lock().is_shown();
        release.observe(parked, now);
        if parked
            && release.due(now)
            && let Some(old) = r.take()
        {
            // Hidden for the whole delay: give the GPU back (swapchain, atlases, buffers). The view stays; the
            // screen and scrollback are in the `Vt`.
            last_px = old.pixel_size();
            drop(old);
        }
        if let Some(on) = ligatures {
            ligatures_cur = Some(on);
        }
        if parked {
            stale |= demand;
            if forced.is_none() {
                dog.settle();
            }
        } else if demand
            || stale
            || forced.is_some()
            || size.is_some()
            || font_pt.is_some()
            || font_new.is_some()
            || ligatures.is_some()
        {
            // Anything that got us here but a bare timer tick owes a frame; a tick itself is judged
            // by `poll` below (it must not count as new demand, or an idle tab would redraw forever).
            dog.owe(now);
        }
        // Shown again after a release: build the renderer on the same view at the size asked for, then carry on as
        // for any frame (the pending font, size and ligatures apply to it below).
        let mut acquired = false;
        if r.is_none() {
            if parked {
                continue;
            }
            match (hooks.acquire)(size.unwrap_or(last_px)) {
                Ok(mut new) => {
                    if let Some(on) = ligatures_cur {
                        new.set_ligatures(on);
                    }
                    r = Some(new);
                    acquired = true;
                    first_frame_pending = true;
                }
                Err(e) => {
                    (hooks.incident)("terminal-reacquire-failed", &e);
                    // Try again on the watchdog's schedule; the requests already taken go back.
                    sh.update(|s| {
                        s.size = size.or(s.size);
                        s.font_pt = font_pt.or(s.font_pt);
                        s.font_new = font_new.or_else(|| s.font_new.take());
                        s.ligatures = ligatures.or(s.ligatures);
                    });
                    dog.rebuilt(Instant::now());
                    continue;
                }
            }
        }
        let Some(mut cur) = r.take() else { continue };
        let mut why = forced.filter(|_| !acquired);
        if why.is_none() && !parked && !acquired && dog.poll(now) == Action::Rebuild {
            why = Some(Why::Stalled);
        }
        if let Some(why) = why {
            let (w, h) = size.unwrap_or_else(|| cur.pixel_size());
            let age = dog.frame_age(now);
            stall_logged = true;
            (hooks.incident)(
                "terminal-rebuild",
                &format!(
                    "{}: last frame {} ago",
                    why.as_str(),
                    age.map_or("never".into(), |a| format!("{} ms", a.as_millis()))
                ),
            );
            cur = match (hooks.rebuild)(cur, (w, h), why) {
                Ok(new) => {
                    // The new surface is blank: its first frame is the one that shows the view.
                    first_frame_pending = true;
                    new
                }
                // Could not (no main thread, no GPU): keep the old one and try again on the
                // watchdog's schedule.
                Err(old) => *old,
            };
            dog.rebuilt(Instant::now());
        }
        if let Some(on) = ligatures {
            cur.set_ligatures(on);
        }
        // A new font size or family changes the cell size, so the grid is re-fitted (and
        // the pty told) exactly like a resize.
        let font_change = match (font_new, font_pt) {
            (None, None) => None,
            (spec, pt) => {
                let spec = spec.unwrap_or_else(|| cur.font_spec().clone());
                let pt = pt.unwrap_or_else(|| sh.state.lock().font_cur);
                let (grid, res) = cur.set_font(pt, &spec);
                (hooks.on_font)(&spec, &res);
                Some(grid)
            }
        };
        let fitted = match (font_change, size) {
            (Some(grid), size) => Some(size.map_or(grid, |(w, h)| cur.resize(w, h))),
            (None, Some((w, h))) => Some(cur.resize(w, h)),
            (None, None) => None,
        };
        if let Some((cols, rows)) = fitted {
            let [cw, ch] = cur.cell_px();
            sh.vt.set_cell_px(cw, ch);
            if sh.vt.size() != (cols, rows) {
                sh.vt.resize(cols, rows);
                (hooks.on_resize)(cols, rows);
            }
        }
        if parked && forced.is_none() {
            // Geometry and fonts above stay current so showing it again is one draw; the content
            // is drawn then (`stale`).
            r = Some(cur);
            continue;
        }
        stale = false;
        frame.selection_color = SELECTION;
        frame.search_color = SEARCH;
        frame.search_current_color = SEARCH_CURRENT;
        frame.focused = focused;
        frame.blink_on = blink.on();
        // Smooth scrolling: the offset the wheel left, drawn with the row above and below the viewport.
        let (frac, preedit) = {
            let s = sh.state.lock();
            (s.scroll_frac, s.preedit.clone())
        };
        frame.over = u32::from(frac != 0.0);
        frame.scroll_frac = frac;
        sh.vt.snapshot(&pal, &mut frame);
        if !preedit.is_empty() {
            frame.overlay_preedit(&preedit);
        }
        last_draw = Some(Instant::now());
        if bell {
            let tint = |c: [u8; 4]| {
                [
                    c[0].saturating_add(40),
                    c[1].saturating_add(40),
                    c[2].saturating_add(40),
                    c[3],
                ]
            };
            frame.bg.iter_mut().for_each(|c| *c = tint(*c));
            frame.clear = tint(frame.clear);
        }
        match cur.render(&frame) {
            Ok(true) => {
                dog.presented(Instant::now());
                if stall_logged {
                    (hooks.incident)("terminal-recovered", "frames are being presented again");
                    stall_logged = false;
                }
                // The first presented frame: the native view may be shown now (see `Placement`).
                if std::mem::take(&mut first_frame_pending) {
                    (hooks.on_first_frame)();
                }
            }
            // No texture to draw into: still owed, the watchdog draws again without new bytes.
            Ok(false) => {}
            // i18n-ignore: log
            Err(e) => eprintln!("atlas-term render: {e}"),
        }
        {
            let mut s = sh.state.lock();
            s.stats = cur.stats();
            s.cell = cur.cell_px();
        }
        r = Some(cur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::scaling::HANG;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    /// What the fake renderer does on each `render`.
    #[derive(Clone, Copy)]
    enum Mode {
        Draw,
        /// The swapchain had no texture (lost/outdated after sleep).
        NoTexture,
        /// `get_current_texture` never returns: the layer is wedged.
        Hang,
    }

    #[derive(Default)]
    struct Log {
        renders: AtomicUsize,
        presented: AtomicUsize,
        rebuilds: Mutex<Vec<Why>>,
        incidents: Mutex<Vec<String>>,
        first_frames: AtomicUsize,
        /// Text of the VT at each render, for "draws the latest content".
        seen: Mutex<Vec<String>>,
        /// `(over, scroll_frac)` of each frame handed to the renderer.
        smooth: Mutex<Vec<(u32, f32)>>,
        /// The characters of the glyphs of each frame handed to the renderer (preedit included).
        drawn: Mutex<Vec<String>>,
        release: AtomicBool,
        /// While set, `rebuild` cannot make a new surface and gives the old renderer back.
        rebuild_fails: AtomicBool,
        /// Renderers alive now (a `Fake` counts itself in `new` and out in `drop`): what a release gives back.
        alive: AtomicUsize,
        /// The size each renderer built after a release was asked for.
        acquires: Mutex<Vec<(u32, u32)>>,
        /// Every `acquire` attempt, failed or not.
        acquire_tries: AtomicUsize,
        /// While set, `acquire` fails.
        acquire_fails: AtomicBool,
        /// The renderers' sizes as `resize` set them, in order.
        resizes: Mutex<Vec<(u32, u32)>>,
    }

    struct Fake {
        mode: std::sync::Arc<Mutex<Mode>>,
        log: std::sync::Arc<Log>,
        sh: Arc<Shared>,
        size: (u32, u32),
        spec: FontSpec,
        /// Which generation of surface this is: a rebuild produces the next.
        generation: usize,
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.log.alive.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl Present for Fake {
        fn set_ligatures(&mut self, _: bool) {}
        fn font_spec(&self) -> &FontSpec {
            &self.spec
        }
        fn set_font(&mut self, _: f32, _: &FontSpec) -> ((u16, u16), FontResolution) {
            unreachable!("no font change in these tests")
        }
        fn resize(&mut self, w: u32, h: u32) -> (u16, u16) {
            self.size = (w, h);
            self.log.resizes.lock().push((w, h));
            ((w / 8) as u16, (h / 16) as u16)
        }
        fn pixel_size(&self) -> (u32, u32) {
            self.size
        }
        fn cell_px(&self) -> [f32; 2] {
            [8.0, 16.0]
        }
        fn stats(&self) -> atlas_term::Stats {
            Default::default()
        }
        fn reset_stats(&mut self) {}
        fn render(&mut self, f: &Frame) -> Result<bool, String> {
            self.log.renders.fetch_add(1, Ordering::SeqCst);
            self.log.smooth.lock().push((f.over, f.scroll_frac));
            self.log
                .drawn
                .lock()
                .push(f.glyphs.iter().map(|g| g.ch).collect());
            self.log.seen.lock().push(self.sh.vt.text());
            match *self.mode.lock() {
                Mode::Draw => {
                    self.log.presented.fetch_add(1, Ordering::SeqCst);
                    Ok(true)
                }
                Mode::NoTexture => Ok(false),
                Mode::Hang => {
                    while !self.log.release.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(false)
                }
            }
        }
    }

    struct Rig {
        sh: Arc<Shared>,
        log: std::sync::Arc<Log>,
        mode: std::sync::Arc<Mutex<Mode>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Rig {
        fn start(mode: Mode) -> Rig {
            Self::start_releasing(mode, None)
        }

        /// A terminal whose hidden view gives its renderer back after `release_after`.
        fn start_releasing(mode: Mode, release_after: Option<Duration>) -> Rig {
            let sh = Shared::for_test(20, 4);
            let log = std::sync::Arc::new(Log::default());
            let mode = std::sync::Arc::new(Mutex::new(mode));
            log.alive.fetch_add(1, Ordering::SeqCst);
            let fake = Fake {
                mode: mode.clone(),
                log: log.clone(),
                sh: sh.clone(),
                size: (160, 64),
                spec: FontSpec::default(),
                generation: 0,
            };
            let (l1, l2, l3, l4, m2, s2) = (
                log.clone(),
                log.clone(),
                log.clone(),
                log.clone(),
                mode.clone(),
                sh.clone(),
            );
            let (l5, m3, s3) = (log.clone(), mode.clone(), sh.clone());
            let hooks = Hooks {
                on_resize: Box::new(|_, _| {}),
                on_font: Box::new(|_, _| {}),
                on_first_frame: Box::new(move || {
                    l1.first_frames.fetch_add(1, Ordering::SeqCst);
                }),
                rebuild: Box::new(move |old: Fake, size, why| {
                    l2.rebuilds.lock().push(why);
                    if l2.rebuild_fails.load(Ordering::SeqCst) {
                        return Err(Box::new(old));
                    }
                    l3.alive.fetch_add(1, Ordering::SeqCst);
                    Ok(Fake {
                        mode: m2.clone(),
                        log: l3.clone(),
                        sh: s2.clone(),
                        size,
                        spec: old.spec.clone(),
                        generation: old.generation + 1,
                    })
                }),
                acquire: Box::new(move |size| {
                    l5.acquire_tries.fetch_add(1, Ordering::SeqCst);
                    if l5.acquire_fails.load(Ordering::SeqCst) {
                        return Err("no GPU".into());
                    }
                    l5.acquires.lock().push(size);
                    l5.alive.fetch_add(1, Ordering::SeqCst);
                    Ok(Fake {
                        mode: m3.clone(),
                        log: l5.clone(),
                        sh: s3.clone(),
                        size,
                        spec: FontSpec::default(),
                        generation: 0,
                    })
                }),
                release_after,
                incident: Box::new(move |k, d| l4.incidents.lock().push(format!("{k}: {d}"))),
            };
            let s = sh.clone();
            let thread = std::thread::spawn(move || render_loop(fake, s, hooks));
            Rig {
                sh,
                log,
                mode,
                thread: Some(thread),
            }
        }

        fn show(&self) {
            self.sh
                .placement
                .lock()
                .request((0.0, 0.0, 160.0, 64.0), true);
        }

        /// The page parks the view (`term_bounds` with `visible: false`), which also wakes the thread.
        fn hide(&self) {
            self.sh
                .placement
                .lock()
                .request((0.0, 0.0, 160.0, 64.0), false);
            self.sh.wake.notify_one();
        }

        /// The page shows it again at `size` physical pixels (`term_bounds` with `visible: true`).
        fn show_at(&self, size: (u32, u32)) {
            self.sh
                .placement
                .lock()
                .request((0.0, 0.0, size.0 as f64, size.1 as f64), true);
            self.sh.update(|s| s.size = Some(size));
        }

        /// Poll until `cond` or fail after a generous (never-sleeping-to-pass) deadline.
        fn until(&self, what: &str, cond: impl Fn(&Log) -> bool) {
            let end = Instant::now() + HANG;
            while !cond(&self.log) {
                assert!(Instant::now() < end, "timed out waiting for: {what}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn stop(mut self) {
            self.log.release.store(true, Ordering::SeqCst);
            self.sh.update(|s| s.closed = true);
            self.thread.take().unwrap().join().unwrap();
        }
    }

    // Regression #104/#108: `render` said Ok(false) after wake, nothing asked again, and the tab
    // stayed blank until the next byte. It must come back with no new output.
    #[test]
    fn a_surface_that_had_no_texture_is_drawn_again_without_new_bytes() {
        let rig = Rig::start(Mode::NoTexture);
        rig.show();
        rig.sh.vt.feed(b"hello");
        rig.sh.redraw();
        rig.until("first failed attempt", |l| {
            l.renders.load(Ordering::SeqCst) >= 1
        });
        assert_eq!(rig.log.presented.load(Ordering::SeqCst), 0);
        // The layer recovers; no byte arrives, no resize, nothing asks for a frame.
        *rig.mode.lock() = Mode::Draw;
        rig.until("a frame is presented on its own", |l| {
            l.presented.load(Ordering::SeqCst) >= 1
        });
        assert!(rig.log.seen.lock().last().unwrap().contains("hello"));
        assert_eq!(rig.log.first_frames.load(Ordering::SeqCst), 1);
        rig.stop();
    }

    #[test]
    fn a_surface_that_never_recovers_is_rebuilt_and_the_new_one_draws() {
        let rig = Rig::start(Mode::NoTexture);
        rig.show();
        rig.sh.vt.feed(b"scrollback line");
        rig.sh.redraw();
        rig.until("watchdog rebuilds", |l| !l.rebuilds.lock().is_empty());
        assert_eq!(rig.log.rebuilds.lock()[0], Why::Stalled);
        *rig.mode.lock() = Mode::Draw;
        rig.until("new surface presents", |l| {
            l.presented.load(Ordering::SeqCst) >= 1
        });
        assert!(
            rig.log
                .seen
                .lock()
                .last()
                .unwrap()
                .contains("scrollback line")
        );
        assert!(
            rig.log
                .incidents
                .lock()
                .iter()
                .any(|i| i.starts_with("terminal-rebuild: stalled")),
            "{:?}",
            rig.log.incidents.lock()
        );
        rig.until("recovery is recorded", |l| {
            l.incidents
                .lock()
                .iter()
                .any(|i| i.starts_with("terminal-recovered"))
        });
        rig.stop();
    }

    // #166: a rebuild that cannot make a surface ("receiving on a closed channel", a configure
    // error) gives the old renderer back. The terminal must stay alive, keep its screen, and be
    // rebuilt again once the GPU cooperates, not stay dead.
    #[test]
    fn a_failed_rebuild_leaves_the_terminal_alive_and_a_later_one_recovers() {
        let rig = Rig::start(Mode::Draw);
        rig.show();
        rig.sh.vt.feed(b"survives");
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);

        rig.log.rebuild_fails.store(true, Ordering::SeqCst);
        rig.sh.rebuild(Why::Screens);
        rig.until("the failing rebuild ran", |l| !l.rebuilds.lock().is_empty());
        // Still alive: new output is drawn by the old renderer (a frame queued by the rebuild
        // itself may present first, so wait for the content, not a frame count).
        rig.sh.vt.feed(b"\r\nstill draws");
        rig.sh.redraw();
        rig.until("the old renderer draws new output", |l| {
            l.seen
                .lock()
                .last()
                .is_some_and(|s| s.contains("still draws"))
        });

        // The GPU cooperates again: the next rebuild takes effect.
        rig.log.rebuild_fails.store(false, Ordering::SeqCst);
        let n = rig.log.rebuilds.lock().len();
        rig.sh.rebuild(Why::Wake);
        rig.until("a rebuild after the failure", |l| {
            l.rebuilds.lock().len() > n
        });
        let before = rig.log.presented.load(Ordering::SeqCst);
        rig.sh.redraw();
        rig.until("the new surface draws", |l| {
            l.presented.load(Ordering::SeqCst) > before
        });
        assert!(rig.log.seen.lock().last().unwrap().contains("survives"));
        rig.stop();
    }

    // "Reload terminal" and a wake event both ask for a rebuild by flag: the screen and scrollback
    // live in the `Vt`, which the rebuild never touches.
    #[test]
    fn a_requested_rebuild_keeps_the_screen_and_scrollback() {
        let rig = Rig::start(Mode::Draw);
        rig.show();
        for i in 0..50 {
            rig.sh.vt.feed(format!("line {i}\r\n").as_bytes());
        }
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        let (before_scroll, before_text) = (rig.sh.vt.scrollbar(), rig.sh.vt.text());
        assert!(
            before_scroll.1 > 4,
            "the test needs scrollback: {before_scroll:?}"
        );
        let n = rig.log.presented.load(Ordering::SeqCst);
        rig.sh.rebuild(Why::Manual);
        rig.until("rebuilt and redrawn", |l| {
            l.presented.load(Ordering::SeqCst) > n && !l.rebuilds.lock().is_empty()
        });
        assert_eq!(rig.log.rebuilds.lock()[0], Why::Manual);
        assert_eq!(rig.sh.vt.scrollbar(), before_scroll);
        assert_eq!(rig.sh.vt.text(), before_text);
        // The new surface is blank until it presents: the view is shown by its first frame again.
        assert_eq!(rig.log.first_frames.load(Ordering::SeqCst), 2);
        rig.stop();
    }

    // Acceptance #108: a stuck renderer blocks neither the registry nor the other tabs.
    #[test]
    fn a_stuck_renderer_does_not_block_other_terminals_or_commands() {
        let stuck = Rig::start(Mode::Hang);
        stuck.show();
        stuck.sh.redraw();
        stuck.until("it is inside the hung render", |l| {
            l.renders.load(Ordering::SeqCst) >= 1
        });
        let healthy = Rig::start(Mode::Draw);
        healthy.show();
        healthy.sh.vt.feed(b"still alive");
        healthy.sh.redraw();
        healthy.until("the healthy tab presents while the other hangs", |l| {
            l.presented.load(Ordering::SeqCst) >= 1
        });
        // Commands on the stuck terminal return: they only take its small state lock. The renderer stays hung until
        // `stop`, so a command that waited for it would never return: the wait on the real event (the commands
        // finished) is bounded by the hang ceiling, not by a stopwatch.
        let sh = stuck.sh.clone();
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            sh.rebuild(Why::Manual);
            sh.redraw();
            sh.update(|s| s.focused = true);
            let _ = sh.vt.text();
            let _ = done.send(());
        });
        assert!(
            finished.recv_timeout(HANG).is_ok(),
            "a command on the stuck terminal blocked behind the hung renderer"
        );
        healthy.stop();
        stuck.stop();
    }

    // FreezeHunt: a parked (invisible) terminal must not draw on every byte burst, and the
    // watchdog must not call that a stall.
    #[test]
    fn a_parked_terminal_skips_frames_and_is_never_rebuilt_then_draws_once_when_shown() {
        let rig = Rig::start(Mode::Draw);
        rig.show();
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        rig.sh
            .placement
            .lock()
            .request((0.0, 0.0, 160.0, 64.0), false);
        let drawn = rig.log.renders.load(Ordering::SeqCst);
        for i in 0..40 {
            rig.sh.vt.feed(format!("burst {i}\r\n").as_bytes());
            rig.sh.redraw();
            std::thread::sleep(Duration::from_millis(2));
        }
        // A failing renderer would be rebuilt if parked frames counted as owed.
        *rig.mode.lock() = Mode::NoTexture;
        std::thread::sleep(crate::term::watchdog::REBUILD_AFTER + Duration::from_millis(600));
        assert_eq!(
            rig.log.renders.load(Ordering::SeqCst),
            drawn,
            "parked: nothing drawn"
        );
        assert!(
            rig.log.rebuilds.lock().is_empty(),
            "parked: nothing to rebuild"
        );
        *rig.mode.lock() = Mode::Draw;
        rig.show();
        rig.sh.update(|s| s.size = Some((160, 64)));
        rig.until("one draw on show", |l| {
            l.renders.load(Ordering::SeqCst) == drawn + 1
        });
        assert!(rig.log.seen.lock().last().unwrap().contains("burst 39"));
        rig.stop();
    }

    #[test]
    fn a_terminal_parked_before_its_first_frame_still_draws_that_frame() {
        let rig = Rig::start(Mode::Draw);
        rig.sh
            .placement
            .lock()
            .request((0.0, 0.0, 160.0, 64.0), false);
        rig.sh.redraw();
        rig.until("first frame despite being parked", |l| {
            l.first_frames.load(Ordering::SeqCst) == 1
        });
        rig.stop();
    }

    // #104 at the logic level: the wake notification flags every terminal, and each rebuilds on
    // its own thread with the reason recorded.
    #[test]
    fn a_wake_event_rebuilds_every_terminal() {
        use crate::term::system::{SystemEvent, on_system_event};
        let (a, b) = (Rig::start(Mode::Draw), Rig::start(Mode::Draw));
        for r in [&a, &b] {
            r.show();
            r.sh.redraw();
            r.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        }
        let n = on_system_event(SystemEvent::Wake, &[a.sh.clone(), b.sh.clone()]);
        assert_eq!(n, 2);
        for r in [&a, &b] {
            r.until("rebuilt", |l| !l.rebuilds.lock().is_empty());
            assert_eq!(r.log.rebuilds.lock()[0], Why::Wake);
        }
        on_system_event(SystemEvent::ScreensChanged, std::slice::from_ref(&a.sh));
        a.until("rebuilt for the screen change", |l| {
            l.rebuilds.lock().len() == 2
        });
        assert_eq!(a.log.rebuilds.lock()[1], Why::Screens);
        a.stop();
        b.stop();
    }

    // Smooth scrolling: while a sub-row offset is on screen the frame carries the overscan rows and the offset;
    // at rest it is the plain snapshot (no extra rows to build or draw).
    #[test]
    fn a_sub_row_offset_is_drawn_with_overscan_and_a_rest_frame_is_plain() {
        let rig = Rig::start(Mode::Draw);
        rig.show();
        rig.sh.vt.feed(
            &(0..40)
                .flat_map(|i| format!("l{i}\r\n").into_bytes())
                .collect::<Vec<_>>(),
        );
        rig.sh.vt.scroll(10);
        rig.sh.update(|s| {
            s.scroll_frac = 0.4;
            s.dirty = true;
        });
        rig.until("an offset frame", |l| {
            l.smooth.lock().iter().any(|&(o, f)| o == 1 && f == 0.4)
        });
        rig.sh.update(|s| {
            s.scroll_frac = 0.0;
            s.dirty = true;
        });
        let n = rig.log.renders.load(Ordering::SeqCst);
        rig.until("a plain frame after", |l| {
            l.renders.load(Ordering::SeqCst) > n && l.smooth.lock().last() == Some(&(0, 0.0))
        });
        rig.stop();
    }

    // Incident (agent TUI flicker): every pty read woke the render thread and drew, so a redraw an agent wrote in
    // many small chunks showed each intermediate state. Ghostty draws at most once per display refresh and always
    // the latest state (`renderer/Thread.zig`: output only marks the screen dirty).
    #[test]
    fn a_burst_of_output_is_one_frame_per_refresh_not_one_per_read() {
        let rig = Rig::start(Mode::Draw);
        rig.show();
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        let before = rig.log.renders.load(Ordering::SeqCst);
        let t0 = Instant::now();
        for i in 0..40 {
            rig.sh.vt.feed(format!("chunk {i}\r\n").as_bytes());
            rig.sh.redraw();
            std::thread::sleep(Duration::from_millis(2));
        }
        rig.until("the last state is drawn", |l| {
            l.seen.lock().last().is_some_and(|t| t.contains("chunk 39"))
        });
        // From the first byte to the last frame: frames are at least one refresh interval apart, so no more than
        // one per interval of that span fit, plus the first (an idle terminal draws at once). The span is
        // measured, so a loaded runner that stretches it gets a larger allowance and a per-read loop (40 frames
        // however long it takes) is still over.
        let span = t0.elapsed();
        let drawn = rig.log.renders.load(Ordering::SeqCst) - before;
        let allowed = (span.as_micros() / FRAME_INTERVAL.as_micros()) as usize + 1;
        assert!(
            drawn <= allowed,
            "{drawn} frames in a {span:?} burst (at most {allowed} at one per refresh)"
        );
        rig.stop();
    }

    // The cost side of pacing: output after an idle period is drawn at once (no refresh to wait for), so typing
    // latency does not gain a frame. A decision, not a wall-clock race: the threaded version of this test measured
    // the runner's scheduler (39 ms on a loaded macOS CI runner, 19551a16).
    #[test]
    fn output_after_idle_is_drawn_without_waiting_for_a_refresh() {
        let last = Instant::now();
        assert_eq!(pace_until(last, last + FRAME_INTERVAL * 3), None);
        assert_eq!(pace_until(last, last + FRAME_INTERVAL), None);
        let soon = last + FRAME_INTERVAL / 4;
        assert_eq!(pace_until(last, soon), Some(last + FRAME_INTERVAL));
    }

    // IME on Linux: the page forwards the input method's preedit (`term_preedit`); it is drawn at the cursor on the next
    // frame, never written to the grid, and gone with the next empty preedit.
    #[test]
    fn a_preedit_is_drawn_until_it_is_cleared_and_never_reaches_the_screen() {
        let rig = Rig::start(Mode::Draw);
        rig.show();
        rig.sh.vt.feed(b"$ ");
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        rig.sh.update(|s| {
            s.preedit = "ni".into();
            s.dirty = true;
        });
        rig.until("a frame with the preedit", |l| {
            l.drawn.lock().last().is_some_and(|d| d.ends_with("ni"))
        });
        assert_eq!(rig.sh.vt.text(), "$", "the preedit is not program output");
        rig.sh.update(|s| {
            s.preedit.clear();
            s.dirty = true;
        });
        rig.until("a frame without it", |l| {
            l.drawn.lock().last().is_some_and(|d| !d.contains('n'))
        });
        rig.stop();
    }

    // #254: a hidden view keeps its renderer for this long in these tests (the real delay is `RELEASE_AFTER`).
    const SHORT: Duration = Duration::from_millis(60);

    /// Show the terminal, let it draw once, hide it, and wait until its renderer is gone.
    fn released_rig() -> Rig {
        let rig = Rig::start_releasing(Mode::Draw, Some(SHORT));
        rig.show();
        rig.sh.vt.feed(b"before");
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 1);
        rig.hide();
        rig.until("the renderer is released", |l| {
            l.alive.load(Ordering::SeqCst) == 0
        });
        rig
    }

    // #254: 20 terminals of a crew kept 20 swapchains, atlases and buffers for views nobody sees.
    #[test]
    fn a_view_hidden_for_the_delay_gives_its_renderer_back_and_draws_nothing_more() {
        let rig = released_rig();
        let renders = rig.log.renders.load(Ordering::SeqCst);
        // Output and a resize while hidden and released: nothing to draw with, nothing built, nothing applied.
        rig.sh.vt.feed(b"\r\nwhile hidden");
        rig.sh.update(|s| {
            s.size = Some((320, 128));
            s.dirty = true;
        });
        std::thread::sleep(SHORT * 3);
        assert_eq!(rig.log.renders.load(Ordering::SeqCst), renders);
        assert_eq!(rig.log.acquire_tries.load(Ordering::SeqCst), 0);
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 0);
        rig.stop();
    }

    // The rebuild on show draws the CURRENT screen at the size the page asks for, and shows the view only with its
    // first frame (no flash of an empty or stale surface).
    #[test]
    fn showing_a_released_view_builds_a_renderer_at_the_new_size_and_draws_the_latest_screen() {
        let rig = released_rig();
        rig.sh.vt.feed(b"\r\nwhile hidden");
        let (first_frames, presented) = (
            rig.log.first_frames.load(Ordering::SeqCst),
            rig.log.presented.load(Ordering::SeqCst),
        );
        rig.show_at((320, 128));
        rig.until("a frame from the new renderer", |l| {
            l.presented.load(Ordering::SeqCst) > presented
        });
        assert_eq!(*rig.log.acquires.lock(), vec![(320, 128)]);
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 1);
        assert!(rig.log.seen.lock().last().unwrap().contains("while hidden"));
        assert_eq!(
            rig.log.first_frames.load(Ordering::SeqCst),
            first_frames + 1,
            "the view is shown by the new renderer's first frame"
        );
        assert_eq!(rig.log.resizes.lock().last(), Some(&(320, 128)));
        rig.stop();
    }

    // A worktree switch A, B, A must not rebuild A: it was hidden for less than the delay.
    #[test]
    fn a_view_shown_again_before_the_delay_keeps_its_renderer() {
        let rig = Rig::start_releasing(Mode::Draw, Some(Duration::from_secs(3600)));
        rig.show();
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        rig.hide();
        std::thread::sleep(Duration::from_millis(80));
        rig.show();
        rig.sh.redraw();
        rig.until("drawn again", |l| l.presented.load(Ordering::SeqCst) >= 2);
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 1);
        assert_eq!(rig.log.acquire_tries.load(Ordering::SeqCst), 0);
        rig.stop();
    }

    // A platform that has not opted in never releases, however long the view is hidden.
    #[test]
    fn without_a_delay_a_hidden_view_keeps_its_renderer() {
        let rig = Rig::start_releasing(Mode::Draw, None);
        rig.show();
        rig.sh.redraw();
        rig.until("first frame", |l| l.presented.load(Ordering::SeqCst) >= 1);
        rig.hide();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 1);
        rig.stop();
    }

    // "Reload terminal", a wake or a display change while released has no surface to rebuild: the renderer built on
    // show is already fresh, so no rebuild runs (a rebuild of nothing would also mean a second surface).
    #[test]
    fn a_rebuild_request_while_released_builds_nothing_until_the_view_is_shown() {
        let rig = released_rig();
        rig.sh.rebuild(Why::Screens);
        std::thread::sleep(SHORT * 3);
        assert!(rig.log.rebuilds.lock().is_empty());
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 0);
        let presented = rig.log.presented.load(Ordering::SeqCst);
        rig.show_at((160, 64));
        rig.until("drawn after the show", |l| {
            l.presented.load(Ordering::SeqCst) > presented
        });
        assert!(
            rig.log.rebuilds.lock().is_empty(),
            "the new renderer is not rebuilt again"
        );
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 1);
        rig.stop();
    }

    // A GPU that refuses the new surface leaves the terminal released and retrying, never a blank view shown: the view
    // stays off screen until a frame is on it. When the GPU is back the next try draws.
    #[test]
    fn a_failed_acquire_is_retried_until_it_draws_and_the_requests_survive() {
        let rig = released_rig();
        rig.log.acquire_fails.store(true, Ordering::SeqCst);
        let presented = rig.log.presented.load(Ordering::SeqCst);
        rig.show_at((320, 128));
        rig.until("two failed tries", |l| {
            l.acquire_tries.load(Ordering::SeqCst) >= 2
        });
        assert_eq!(rig.log.presented.load(Ordering::SeqCst), presented);
        assert_eq!(
            rig.log.first_frames.load(Ordering::SeqCst),
            1,
            "never shown without a frame"
        );
        rig.log.acquire_fails.store(false, Ordering::SeqCst);
        rig.until("the retry draws", |l| {
            l.presented.load(Ordering::SeqCst) > presented
        });
        assert_eq!(
            *rig.log.acquires.lock(),
            vec![(320, 128)],
            "the size asked for survived the failures"
        );
        rig.stop();
    }

    // Hidden again after a rebuild: the cycle repeats (the delay restarts from the new hide).
    #[test]
    fn a_view_can_be_released_shown_and_released_again() {
        let rig = released_rig();
        let presented = rig.log.presented.load(Ordering::SeqCst);
        rig.show_at((160, 64));
        rig.until("shown", |l| l.presented.load(Ordering::SeqCst) > presented);
        rig.hide();
        rig.until("released again", |l| l.alive.load(Ordering::SeqCst) == 0);
        assert_eq!(rig.log.acquires.lock().len(), 1);
        rig.stop();
    }

    // #247 with #254: an input method composing into a terminal whose renderer was given back. The preedit update has
    // nothing to draw with (and must not touch the missing renderer); the renderer built on show draws it.
    #[test]
    fn a_preedit_set_while_the_renderer_is_released_is_drawn_by_the_one_built_on_show() {
        let rig = released_rig();
        let renders = rig.log.renders.load(Ordering::SeqCst);
        rig.sh.update(|s| {
            s.preedit = "ni".into();
            s.dirty = true;
        });
        std::thread::sleep(SHORT * 3);
        assert_eq!(rig.log.renders.load(Ordering::SeqCst), renders);
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 0);
        let presented = rig.log.presented.load(Ordering::SeqCst);
        rig.show_at((320, 128));
        rig.until("a frame of the new renderer with the preedit", |l| {
            l.presented.load(Ordering::SeqCst) > presented
                && l.drawn.lock().last().is_some_and(|d| d.ends_with("ni"))
        });
        assert_eq!(rig.log.alive.load(Ordering::SeqCst), 1);
        assert_eq!(
            rig.sh.vt.text(),
            "before",
            "the preedit is not program output"
        );
        rig.stop();
    }
}
