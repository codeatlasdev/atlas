//! The one virtual display of an agent instance, and the parking of its window on it. Pure policy is in
//! `agent_mode.rs`, the display's lifecycle in `display_mac.rs`; this is the glue that owns it for the process.

use std::sync::LazyLock;

use objc2::msg_send;
use objc2::runtime::AnyObject;
use tauri::{AppHandle, Manager};

use crate::agent_mode::{Facts, Rect, ShowGranted, show_decision};
use crate::display_mac::{Cg, ConfigLock, Display};

/// Logical size of the agent display: the app's default window size, so a window fills it.
const LOGICAL: (u32, u32) = (1280, 820);

/// Kept for the life of the process: the display exists exactly as long as this instance does. A clean exit, a panic
/// and a SIGKILL all end the process, and the OS removes a virtual display with its owner.
static DISPLAY: LazyLock<parking_lot::Mutex<Option<Display<Cg>>>> =
    LazyLock::new(|| parking_lot::Mutex::new(None));

/// Create the display once. Errors are returned, never papered over: an agent instance without its display must not
/// fall back to the user's monitors. Returns the id we own, its bounds and the pixel ratio asked for.
pub fn ensure() -> Result<(u32, Rect, u32), String> {
    let mut g = DISPLAY.lock();
    let scale = crate::agent_mode::scale_from(std::env::var("ATLAS_AGENT_SCALE").ok().as_deref());
    if g.is_none() {
        // Concurrent instances take turns configuring displays (a failed lock only costs the ordering).
        let _turn =
            ConfigLock::acquire(&std::env::temp_dir().join("atlas-agent-display.lock")).ok();
        *g = Some(Display::create(Cg::new(), LOGICAL, scale)?);
    }
    let d = g.as_ref().expect("just created");
    Ok((d.id, d.bounds, scale))
}

/// The `NSScreen` of display `id`, by its `NSScreenNumber`.
fn screen_of(id: u32) -> Option<*mut AnyObject> {
    use objc2::runtime::AnyClass;
    let cls = AnyClass::get(c"NSScreen")?;
    // SAFETY: plain AppKit getters on the main thread; every object is checked for nil before use.
    unsafe {
        let screens: *mut AnyObject = msg_send![cls, screens];
        if screens.is_null() {
            return None;
        }
        let n: usize = msg_send![screens, count];
        let key = objc2_foundation::NSString::from_str("NSScreenNumber");
        for i in 0..n {
            let sc: *mut AnyObject = msg_send![screens, objectAtIndex: i];
            let desc: *mut AnyObject = msg_send![sc, deviceDescription];
            let num: *mut AnyObject = msg_send![desc, objectForKey: &*key];
            if num.is_null() {
                continue;
            }
            let v: u32 = msg_send![num, unsignedIntValue];
            if v == id {
                return Some(sc);
            }
        }
    }
    None
}

/// An AppKit (bottom-left) rect as a global top-left one.
fn flip(f: objc2_foundation::NSRect, primary_h: f64) -> Rect {
    Rect {
        x: f.origin.x,
        y: primary_h - (f.origin.y + f.size.height),
        w: f.size.width,
        h: f.size.height,
    }
}

/// The window's frame now, in global top-left points (AppKit reports bottom-left; `primary_h` flips it).
fn frame_of(ns: *mut AnyObject, primary_h: f64) -> Rect {
    // SAFETY: `-frame` is a plain getter on the live NSWindow, called on the main thread.
    let f: objc2_foundation::NSRect = unsafe { msg_send![ns, frame] };
    Rect {
        x: f.origin.x,
        y: primary_h - (f.origin.y + f.size.height),
        w: f.size.width,
        h: f.size.height,
    }
}

/// `ATLAS_AGENT_DRYRUN=1` (agent instances only): do everything up to the show, log the decision, never order the
/// window in, and exit. The proof that the decision is right before a real window is risked.
fn dry_run() -> bool {
    std::env::var("ATLAS_AGENT_DRYRUN").is_ok_and(|v| v == "1")
}

/// Refuse for good: the window stays hidden and the instance ends with the reason, so whoever started it sees an
/// error instead of an instance whose window may be anywhere.
fn refuse(app: &AppHandle, why: &str) {
    eprintln!("atlas-agent: REFUSED to show the window: {why}");
    app.exit(3);
}

/// Move `win` onto the agent display and show it without activating the app, but only if [`show_decision`] says
/// the placement is provably on our own display. Anything else hides the window and ends the instance (exit 3).
pub fn park_and_show(app: &AppHandle, win: &tauri::Window) {
    let (display_id, bounds, scale) = match ensure() {
        Ok(b) => b,
        Err(e) => return refuse(app, &format!("no virtual display ({e})")),
    };
    let size = win
        .outer_size()
        .ok()
        .zip(win.scale_factor().ok())
        .map_or((f64::from(LOGICAL.0), f64::from(LOGICAL.1)), |(s, f)| {
            (f64::from(s.width) / f, f64::from(s.height) / f)
        });
    let size = crate::agent_mode::fit_size(size, bounds);
    let Ok(ns) = win.ns_window() else {
        return refuse(app, "the window has no NSWindow");
    };
    let w = ns as *mut AnyObject;
    // Place it with AppKit itself, synchronously: Tauri's `set_position` on a window that has never been shown is
    // applied later, which left the frame on the user's monitor at the moment of the check (first agent run).
    let primary_h = crate::display_mac::primary_height();
    let Some(screen) = screen_of(display_id) else {
        return refuse(app, "AppKit has no screen for our virtual display");
    };
    // SAFETY: plain getter on a live NSScreen, main thread.
    let allowed = flip(unsafe { msg_send![screen, visibleFrame] }, primary_h);
    let top_left = crate::agent_mode::target_frame(bounds, allowed, size);
    let target = crate::agent_mode::cocoa_frame(top_left, primary_h);
    let frame = objc2_foundation::NSRect::new(
        objc2_foundation::NSPoint::new(target.x, target.y),
        objc2_foundation::NSSize::new(target.w, target.h),
    );
    // SAFETY: Tauri's live NSWindow on the main thread (`show_main` runs there); `setFrame:display:` takes a rect.
    unsafe {
        let _: () = msg_send![w, setFrame: frame, display: false];
    }
    // What AppKit would make of that frame when the window is ordered in (it moves a window out of the reserved
    // menu-bar band): asked of AppKit itself, before anything is shown.
    let placed: objc2_foundation::NSRect = unsafe { msg_send![w, frame] };
    // SAFETY: `constrainFrameRect:toScreen:` is a pure function of the rect and the live screen.
    let constrained: objc2_foundation::NSRect =
        unsafe { msg_send![w, constrainFrameRect: placed, toScreen: screen] };
    // The last check, in code, before a single pixel can reach a screen. Every fact is read LIVE after the
    // placement: where AppKit really put the window, every active display, the mode of the one we own.
    let displays = crate::display_mac::active_displays();
    let facts = Facts {
        own_id: display_id,
        logical: (f64::from(LOGICAL.0), f64::from(LOGICAL.1)),
        scale,
        displays: &displays,
        own_mode: crate::display_mac::display_mode(display_id),
        target: top_left,
        frame: frame_of(w, crate::display_mac::primary_height()),
        constrained: flip(constrained, crate::display_mac::primary_height()),
    };
    let granted = match show_decision(&facts) {
        Ok(g) => g,
        Err(e) => return refuse(app, &e),
    };
    if dry_run() {
        eprintln!(
            "atlas-agent: DRYRUN decision: would show {:?} on display {display_id} (not ordered in)",
            facts.frame
        );
        app.exit(0);
        return;
    }
    order_front(w, granted);
}

/// The one place an agent window is ordered in. `orderFront:` shows it without making it key and without activating
/// the application. Needs the proof from [`show_decision`], so no other path can get here.
fn order_front(w: *mut AnyObject, _granted: ShowGranted) {
    // SAFETY: Tauri's live NSWindow, main thread; `orderFront:` takes a sender or nil.
    unsafe {
        let _: () = msg_send![w, orderFront: std::ptr::null::<AnyObject>()];
    }
}

/// Keep watching, for the life of the process, that every visible window of this agent instance is wholly on the
/// display we own; if one is not (the placement check passed once and something moved it later), hide it at once
/// and say so. Second line behind [`show_decision`]. Cheap: one main-thread hop every 250 ms.
pub fn guard_user_displays(app: &AppHandle) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    let app = app.clone();
    STARTED.call_once(move || {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let a = app.clone();
                let _ = app.run_on_main_thread(move || hide_if_off_our_display(&a));
            }
        });
    });
}

/// The display we own, for the watchdog. `Err(())` while `ensure` holds the lock: creating the display services the
/// main run loop (`select_hidpi_mode`), which runs the watchdog's tick on the SAME thread, and the mutex is not
/// reentrant, so a blocking lock there deadlocked agent starts ("window was not shown after 90s"). Mid-creation no
/// window has been ordered in yet, so the tick has nothing to check and is skipped.
fn own_display() -> Result<Option<u32>, ()> {
    DISPLAY
        .try_lock()
        .map(|g| g.as_ref().map(|d| d.id))
        .ok_or(())
}

fn hide_if_off_our_display(app: &AppHandle) {
    let Ok(own) = own_display() else { return };
    let primary_h = crate::display_mac::primary_height();
    let displays = crate::display_mac::active_displays();
    for win in app.windows().values() {
        let Ok(ns) = win.ns_window() else { continue };
        let w = ns as *mut AnyObject;
        // SAFETY: plain getter on Tauri's live NSWindow, on the main thread.
        let visible: bool = unsafe { msg_send![w, isVisible] };
        if !visible {
            continue;
        }
        let frame = frame_of(w, primary_h);
        // No display of ours yet means no window may be visible at all.
        let why = match own {
            Some(id) => crate::agent_mode::watchdog_verdict(frame, id, &displays),
            None => Some("no virtual display exists yet".into()),
        };
        let Some(why) = why else { continue };
        eprintln!("atlas-agent: a window was visible where it must not be ({why}); hiding it");
        // SAFETY: `orderOut:` takes a sender or nil.
        unsafe {
            let _: () = msg_send![w, orderOut: std::ptr::null::<AnyObject>()];
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let name = p
                        .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    out.push((name, std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
        out
    }

    /// Code of `src` without comments and without its `#[cfg(test)]` module.
    fn code(src: &str) -> String {
        let end = src.find("#[cfg(test)]\nmod tests").unwrap_or(src.len());
        src[..end]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn hits(needle: &str) -> Vec<String> {
        sources()
            .into_iter()
            .filter(|(_, s)| code(s).contains(needle))
            .map(|(n, _)| n)
            .collect()
    }

    /// Incident (2026-10-26, agent starts hanging "window was not shown after 90s"): `ensure` holds DISPLAY while
    /// creating the display, which services the run loop and runs the watchdog's tick on the same thread. The tick's
    /// read must not block on the held lock (parking_lot is not reentrant: a blocking lock there is a deadlock).
    #[test]
    fn the_watchdog_read_does_not_block_while_the_display_is_being_created() {
        let held = super::DISPLAY.lock();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(super::own_display()).unwrap());
        let r = rx.recv_timeout(std::time::Duration::from_secs(2));
        drop(held);
        assert_eq!(r, Ok(Err(())), "the watchdog read blocked on the held lock");
        assert_eq!(super::own_display(), Ok(None));
    }

    /// Incident (agent window on the user's built-in display for ~25 s): with `ATLAS_AGENT=1` nothing may order a
    /// window onto a screen except the one guarded path (`order_front`, which needs the `ShowGranted` proof that
    /// only `show_decision` can issue). Any new caller of these AppKit/Tauri calls fails this test and must either
    /// be unreachable in agent mode or go through the guard.
    #[test]
    fn no_path_shows_an_agent_window_except_the_guarded_one() {
        for needle in [
            "orderFront",
            "makeKeyAndOrderFront",
            "orderFrontRegardless",
            "makeKeyWindow",
            "activateIgnoringOtherApps",
        ] {
            let at = hits(needle);
            let allowed: &[&str] = match needle {
                "orderFront" => &["src/agent_display.rs"],
                // The dock panel becomes key (without activating the app) so its input gets keys; unreachable in
                // agent mode, which the dock walls below assert.
                "makeKeyWindow" => &["src/dock_mac.rs"],
                _ => &[],
            };
            assert_eq!(at, allowed, "{needle} appears in {at:?}");
        }
        // the Tauri window calls that show or focus: only the user's branch of `show_main` and the tray popover
        // (which agent mode never creates: `tray_allowed`)
        for (needle, allowed) in [
            (
                ".show()",
                &[
                    "src/dock.rs",
                    "src/main.rs",
                    "src/tray.rs",
                    "src/agent/notify.rs",
                    "src/live.rs",
                    "src/shots/surface.rs",
                    // a system notification (`NotificationExt`), not a window
                    "src/shots/runner.rs",
                ][..],
            ),
            (".set_focus()", &["src/main.rs", "src/tray.rs"][..]),
            (".unminimize()", &["src/main.rs"][..]),
            (".maximize()", &[][..]),
            (".set_fullscreen(", &[][..]),
        ] {
            let mut at = hits(needle);
            at.sort();
            let mut want: Vec<_> = allowed.iter().map(|s| (*s).to_string()).collect();
            want.sort();
            assert_eq!(at, want, "{needle}");
        }
        // The dock: its `.show()` sits in the user branch of `show` (agent instances go through `park_and_show`),
        // its key status and its hotkey are walled off in agent mode. Whitespace-insensitive: rustfmt re-wraps.
        let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let dock = squash(&code(
            &sources()
                .iter()
                .find(|(f, _)| f == "src/dock.rs")
                .unwrap()
                .1,
        ));
        let gate = "if crate::agent_mode::Mode::current().focus_allowed() {";
        let (_, show_fn) = dock
            .split_once("fn show(app: &AppHandle) {")
            .expect("dock::show");
        let show_fn = show_fn.split_once(" fn hide").unwrap().0;
        let (user, agent) = show_fn
            .split_once(gate)
            .expect("dock::show keeps its focus gate");
        assert!(!user.contains(".show()"), "nothing shows before the gate");
        let (user_branch, agent_branch) = agent.split_once("} else {").unwrap();
        assert!(user_branch.contains(".show()"));
        assert!(agent_branch.contains("park_and_show") && !agent_branch.contains(".show()"));
        let wall = "if !crate::agent_mode::Mode::current().focus_allowed() { return; }";
        let keys = dock.split_once("pub fn dock_keys").unwrap().1;
        assert!(
            keys.split_once('{')
                .unwrap()
                .1
                .trim_start()
                .starts_with(wall),
            "dock_keys wall"
        );
        // Hotkeys: an agent instance gets an empty set (the previous one is released), and `wanted_keys` is only
        // reachable from the else branch.
        let (_, hk) = dock
            .split_once("fn apply_hotkeys(")
            .expect("dock::apply_hotkeys");
        let hk = hk.split_once(" /// A hotkey fired").map_or(hk, |(a, _)| a);
        let (agent_arm, user_arm) = hk
            .split_once("if !crate::agent_mode::Mode::current().focus_allowed() {")
            .expect("apply_hotkeys keeps its focus gate")
            .1
            .split_once("} else {")
            .unwrap();
        assert!(
            agent_arm.contains("Vec::new()") && !agent_arm.contains("wanted_keys"),
            "an agent instance owns no global hotkey"
        );
        assert!(user_arm.contains("wanted_keys"));
        // `dock::summon` is called from async command threads (a capture sent to an agent): showing the panel is an
        // AppKit window call that traps off the main thread, so the whole body hops to it.
        let (_, summon_fn) = dock.split_once("pub fn summon(").expect("dock::summon");
        let summon_fn = summon_fn
            .split_once(" /// A capture for the composer")
            .map_or(summon_fn, |(a, _)| a);
        assert!(
            summon_fn.contains("run_on_main_thread(move || { show(&a);"),
            "dock::summon must show the dock on the main thread: {summon_fn}"
        );
        // Capture windows (overlay, card, HUD, editor): every one is ordered in through `surface::show` (user branch
        // `show`, agent branch `park_and_show`), and key status is only ever granted through `dock_mac::overlay_keys`,
        // which is the one place that refuses in an agent instance (the overlay itself is never built there).
        let mac = squash(&code(
            &sources()
                .iter()
                .find(|(f, _)| f == "src/dock_mac.rs")
                .unwrap()
                .1,
        ));
        let (_, keys_fn) = mac
            .split_once("pub fn overlay_keys(")
            .expect("dock_mac::overlay_keys");
        let body = keys_fn.split_once('{').unwrap().1.trim_start();
        assert!(
            body.starts_with("if !crate::agent_mode::Mode::current().focus_allowed() { return; }"),
            "overlay_keys wall: {body}"
        );
        let surface_src = squash(&code(
            &sources()
                .iter()
                .find(|(f, _)| f == "src/shots/surface.rs")
                .unwrap()
                .1,
        ));
        let (_, show_fn) = surface_src
            .split_once("pub fn show(")
            .expect("surface::show");
        let (user, agent) = show_fn
            .split_once("if crate::agent_mode::Mode::current().focus_allowed() {")
            .expect("surface::show keeps its focus gate");
        let _ = user;
        let (user_branch, agent_branch) = agent.split_once("} else {").unwrap();
        assert!(user_branch.contains(".show()"));
        assert!(agent_branch.contains("park_and_show") && !agent_branch.contains(".show()"));
        // Pins and the quick stack (many windows, relaid out while visible): they never show or position a window
        // themselves. Showing is `surface::show`; every relayout is `surface::place`, whose agent arm never sets a
        // position (a global point is a real display, and a visible agent window there ends the instance). The
        // keyboard is only ever asked through `dock_mac::overlay_keys`.
        for file in ["src/shots/pin.rs", "src/shots/quick.rs"] {
            let text = code(&sources().iter().find(|(f, _)| f == file).unwrap().1);
            for banned in [
                ".show()",
                "set_position",
                "set_focus",
                "park_and_show",
                "makeKeyWindow",
                "make_key(",
                "order_front",
            ] {
                assert!(
                    !text.contains(banned),
                    "{file} must go through surface::show / surface::place, found {banned}"
                );
            }
            assert!(text.contains("surface::"), "{file} uses the surface module");
        }
        let pin = code(
            &sources()
                .iter()
                .find(|(f, _)| f == "src/shots/pin.rs")
                .unwrap()
                .1,
        );
        assert!(
            pin.contains("surface::show(") && pin.contains("surface::place("),
            "a pin is ordered in by surface::show and moved by surface::place"
        );
        assert!(
            pin.contains("dock_mac::overlay_keys("),
            "a pin asks for the keyboard only through overlay_keys"
        );
        // the page shows the dock by itself only for the user's app
        assert!(dock.contains("enabled && crate::agent_mode::Mode::current().focus_allowed()"));
        let all = sources();
        let src = |n: &str| code(&all.iter().find(|(f, _)| f == n).unwrap().1);
        // `show_main`: the three window calls sit inside the branch that agent mode never takes, and the agent
        // branch goes through `park_and_show` only.
        let main = src("src/main.rs");
        let (gate, rest) = main
            .split_once("if agent_mode::Mode::current().focus_allowed() {")
            .expect("show_main keeps its focus gate");
        assert!(!gate.contains(".show()") && !gate.contains(".set_focus()"));
        let (user_branch, agent_branch) = rest.split_once("return;").unwrap();
        assert!(user_branch.contains(".show()") && user_branch.contains(".set_focus()"));
        assert!(agent_branch.contains("park_and_show") && !agent_branch.contains(".show()"));
        // the tray is only started when the mode allows it
        assert!(main.contains("if agent_mode::Mode::current().tray_allowed() {"));
        // the guarded path itself: it takes the proof by value and nothing else constructs one
        assert!(
            src("src/agent_display.rs")
                .contains("fn order_front(w: *mut AnyObject, _granted: ShowGranted)")
        );
        let ctor = hits("ShowGranted(())");
        assert_eq!(
            ctor,
            ["src/agent_mode.rs"],
            "only the decision issues the proof"
        );
    }
}
