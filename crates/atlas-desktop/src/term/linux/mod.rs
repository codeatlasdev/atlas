//! The Linux native terminal: a `wl_subsurface` of GDK's Wayland window, BELOW the page, drawn by `atlas-term` on Vulkan.
//!
//! This is the platform seam `term` calls (`native::*`), the Linux twin of `term/mac.rs`: where the AppKit view was made,
//! placed, slid and destroyed, a child Wayland surface is. The pty reader, the render thread and every `term_*` command
//! are the shared ones. `capability` decides whether the session can run it at all (everything else keeps xterm.js),
//! `wl` is the protocol work, `geometry` the placement arithmetic, `clipboard` the GTK clipboard.
//!
//! The page reserves a transparent slot; the compositor stacks the page over the terminal, so menus, tooltips and dialogs
//! need no hiding logic. The terminal takes no input itself (empty input region): keys, IME and the pointer reach the
//! webview and the page forwards them over the same `term_*` commands the macOS terminal uses.

mod capability;
mod clipboard;
mod fault;
mod geometry;
mod wl;

use atlas_term::{FontResolution, FontSpec, Renderer};
use tauri::AppHandle;

pub use capability::{Capability, window_wanted};
pub use clipboard::{read_clipboard, read_primary, write_clipboard, write_primary};

/// Nothing to hand over: the toplevel is found from the app when the surface is made.
pub fn window(_app: &AppHandle) -> Result<usize, String> {
    Ok(0)
}

/// Physical pixels per logical pixel for the terminals: the compositor's fractional scale for the toplevel (1, 1.5, 2 ...),
/// GTK's integer scale when it sends none. The first call connects on the main thread.
pub fn scale(app: &AppHandle) -> Result<f64, String> {
    if wl::ready() {
        return Ok(wl::scale());
    }
    super::on_main(app, wl::ensure)
}

/// Did the scale change enough to remake the surface (and its font) at the new one?
pub fn scale_changed(old: f64, new: f64) -> bool {
    (old - new).abs() > 1e-6
}

/// The swapchain size for the slot at (`x`, `y`) `w` x `h` logical: the same rounding the placement uses.
pub fn pixel_size(x: f64, y: f64, w: f64, h: f64, scale: f64) -> (u32, u32) {
    geometry::pixel_size(x, y, w, h, scale)
}

/// A new child surface, parked off screen, destination `w` x `h` logical. Main thread. Returns its handle.
pub fn create(_win: usize, _x: f64, _y: f64, w: f64, h: f64) -> Result<usize, String> {
    wl::create(w, h)
}

/// A renderer drawing into `view`'s surface at `pixels`.
///
/// # Safety
/// `view` is live and outlives the renderer (the render thread is joined, or the renderer dropped, before `destroy`).
pub unsafe fn renderer(
    view: usize,
    pixels: (u32, u32),
    font_pt: f32,
    scale: f32,
    spec: FontSpec,
) -> Result<(Renderer, FontResolution), String> {
    #[cfg(debug_assertions)]
    {
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(e) = fault::fault(std::env::var("ATLAS_TERM_FAULT").ok().as_deref(), call) {
            return Err(e);
        }
    }
    let (display, window) = wl::raw_handles(view)?;
    // SAFETY: the display is GDK's and lives for the process; the surface is `view`'s, per the caller's contract.
    unsafe { Renderer::new_raw(display, window, pixels.0, pixels.1, font_pt, scale, spec) }
}

pub fn set_frame(view: usize, x: f64, y: f64, w: f64, h: f64) {
    wl::set_frame(view, x, y, w, h);
}

pub fn slide_from(view: usize, dx: f64, ms: f64) {
    wl::slide_from(view, dx, ms);
}

pub fn destroy(view: usize) {
    wl::destroy(view);
}

/// No wake or display-change notifications to hook: a lost or outdated Vulkan swapchain is reconfigured by the renderer,
/// and a new toplevel surface or scale is noticed by `wl`'s tick.
pub fn install_system_observers() {}

/// What this session can do, for the Settings toggle and the page's choice of terminal. `gpu` is the GPU probe, which
/// is slow (a Vulkan device) and so runs off the main thread by the caller.
pub async fn capability(app: &AppHandle) -> Capability {
    let probe = super::on_main(app, || {
        let wayland = wl::gdk_is_wayland();
        Ok((wayland, if wayland { wl::probe() } else { (None, vec![]) }))
    });
    let (gdk_wayland, (connect_error, missing_globals)) = probe.unwrap_or((false, (None, vec![])));
    let gpu = if gdk_wayland && connect_error.is_none() && missing_globals.is_empty() {
        tokio::task::spawn_blocking(|| {
            std::panic::catch_unwind(atlas_term::probe)
                .unwrap_or_else(|_| Err("the Vulkan loader panicked".into()))
        })
        .await
        .ok()
    } else {
        None
    };
    let env = capability::Env {
        gdk_wayland,
        connect_error,
        missing_globals,
        gpu,
    };
    let decision = capability::decide(&env);
    let setting = capability::setting_on();
    if let Some(line) = capability::incident(&decision, setting) {
        crate::diag::log(&line);
    }
    capability::reply(decision, setting, window_clear())
}

/// Was the main window made see-through at launch? Set once by `main` from [`window_wanted`].
static WINDOW_CLEAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_window_clear(v: bool) {
    WINDOW_CLEAR.store(v, std::sync::atomic::Ordering::Relaxed);
}

fn window_clear() -> bool {
    WINDOW_CLEAR.load(std::sync::atomic::Ordering::Relaxed)
}
