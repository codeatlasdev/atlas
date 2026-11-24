//! The Wayland side: a `wl_subsurface` of GDK's own toplevel surface, placed BELOW the page.
//!
//! GDK owns the connection and the toplevel `wl_surface`; this module wraps GDK's `wl_display` (not a second socket) in
//! a `wayland-client` connection with its OWN event queue, binds the globals it needs on that queue, and makes one child
//! surface per terminal. The page above is transparent over the terminal's slot, so everything the page draws (menus,
//! tooltips, dialogs) is painted over the terminal by the compositor with nothing to hide or restore. The child's input
//! region is empty: the pointer and keyboard go to the webview, which owns focus, selection and IME.
//!
//! Threading: every call here is on the GTK main thread (`super::super::on_main`), like the AppKit work on macOS. The
//! renderer draws from its own thread through the raw `wl_surface` pointer ([`Sub::raw_handles`]), which wgpu's
//! Vulkan WSI drives on its own queue of the shared display.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use tauri::Manager;
use wayland_backend::sys::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry,
    wl_subcompositor::WlSubcompositor, wl_subsurface::WlSubsurface, wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

use super::geometry::{self, Rect};

/// Where a parked subsurface sits: far outside any output.
const OFFSCREEN: i32 = -20_000;

/// The globals the native terminal cannot do without.
pub const REQUIRED: [&str; 3] = ["wl_compositor", "wl_subcompositor", "wp_viewporter"];

/// Queue state: the fractional scale the compositor prefers for the toplevel (times 120; 0 = none yet).
struct State {
    scale120: Arc<AtomicU32>,
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &WpFractionalScaleV1,
        e: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = e {
            s.scale120.store(scale, Ordering::Relaxed);
        }
    }
}

delegate_noop!(State: WlCompositor);
delegate_noop!(State: WlSubcompositor);
delegate_noop!(State: WpViewporter);
delegate_noop!(State: WpViewport);
delegate_noop!(State: WlSubsurface);
delegate_noop!(State: WlRegion);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);

/// The connection and the bound globals, made once on the main thread.
struct Wl {
    conn: Connection,
    qh: QueueHandle<State>,
    queue: Mutex<(EventQueue<State>, State)>,
    compositor: WlCompositor,
    subcompositor: WlSubcompositor,
    viewporter: WpViewporter,
    frac: Option<WpFractionalScaleManagerV1>,
    /// GDK's `wl_display*`, for the Vulkan surface.
    display: usize,
}

/// Connected on first use, which is always on the main thread (`create` and the capability probe).
static WL: LazyLock<Result<Wl, String>> = LazyLock::new(|| {
    // With `dlopen` a machine without libwayland-client panics at the first call instead of failing to link.
    std::panic::catch_unwind(Wl::connect)
        .unwrap_or_else(|_| Err("libwayland-client could not be loaded".into()))
});

/// The start of the error for a compositor that lacks required globals (the list follows).
const MISSING: &str = "the compositor lacks ";

/// The compositor's preferred scale for the toplevel, times 120; 0 until it says (or never, without the protocol).
static SCALE120: LazyLock<Arc<AtomicU32>> = LazyLock::new(Default::default);
/// The integer scale GTK draws the page at, the fallback when the compositor sends no fractional scale.
static GTK_SCALE: AtomicU32 = AtomicU32::new(1);

/// The toplevel's `wl_surface*` as of the last check, and the fractional-scale object bound on it.
struct Parent {
    ptr: usize,
    surface: WlSurface,
    _frac: Option<WpFractionalScaleV1>,
}

static PARENT: Mutex<Option<Parent>> = Mutex::new(None);
/// Subsurfaces alive: the tick runs while there are any.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static TICKING: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn main_window() -> Result<gtk::ApplicationWindow, String> {
    let app = super::super::APP.get().ok_or("no app yet")?;
    app.get_window("main")
        .ok_or("no main window")?
        .gtk_window()
        .map_err(|e| e.to_string())
}

/// GDK's Wayland display, or why it is not one.
fn gdk_display() -> Result<*mut c_void, String> {
    let d = gtk::gdk::Display::default().ok_or("no GDK display")?;
    if d.type_().name() != "GdkWaylandDisplay" {
        return Err(format!("GDK display is {}, not Wayland", d.type_().name()));
    }
    // SAFETY: a GdkWaylandDisplay, checked by type just above.
    let p = unsafe {
        gdk_wayland_sys::gdk_wayland_display_get_wl_display(
            ToGlibPtr::<*mut gtk::gdk::ffi::GdkDisplay>::to_glib_none(&d)
                .0
                .cast(),
        )
    };
    NonNull::new(p)
        .map(NonNull::as_ptr)
        .ok_or("GDK has no wl_display".into())
}

/// The toplevel's `wl_surface*` now. It changes when GDK unmaps and maps the window again.
fn toplevel_surface(win: &gtk::ApplicationWindow) -> Option<*mut c_void> {
    let gw = win.window()?;
    if gw.type_().name() != "GdkWaylandWindow" {
        return None;
    }
    // SAFETY: a GdkWaylandWindow, checked by type just above.
    let p = unsafe {
        gdk_wayland_sys::gdk_wayland_window_get_wl_surface(
            ToGlibPtr::<*mut gtk::gdk::ffi::GdkWindow>::to_glib_none(&gw)
                .0
                .cast(),
        )
    };
    NonNull::new(p).map(NonNull::as_ptr)
}

impl Wl {
    fn connect() -> Result<Self, String> {
        let display = gdk_display()?;
        // SAFETY: GDK's display outlives the process's use of it; the backend does not own or close it.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let (list, queue) = registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let missing: Vec<String> = REQUIRED
            .iter()
            .filter(|n| {
                !list
                    .contents()
                    .with_list(|g| g.iter().any(|x| x.interface == **n))
            })
            .map(|n| (*n).to_string())
            .collect();
        if !missing.is_empty() {
            return Err(format!("{MISSING}{}", missing.join(", ")));
        }
        let bind_err = |e: wayland_client::globals::BindError| e.to_string();
        let compositor = list.bind(&qh, 1..=6, ()).map_err(bind_err)?;
        let subcompositor = list.bind(&qh, 1..=1, ()).map_err(bind_err)?;
        let viewporter = list.bind(&qh, 1..=1, ()).map_err(bind_err)?;
        let frac = list.bind(&qh, 1..=1, ()).ok();
        let state = State {
            scale120: SCALE120.clone(),
        };
        Ok(Self {
            conn,
            qh,
            queue: Mutex::new((queue, state)),
            compositor,
            subcompositor,
            viewporter,
            frac,
            display: display as usize,
        })
    }

    /// Dispatch what GDK's reads left on our queue (the fractional scale), and send our requests.
    fn pump(&self) {
        let mut q = lock(&self.queue);
        let (queue, state) = &mut *q;
        let _ = queue.dispatch_pending(state);
        let _ = self.conn.flush();
    }
}

/// The connection, made on first use. `Err` is why the native terminal cannot run in this session.
fn wl() -> Result<&'static Wl, String> {
    WL.as_ref().map_err(Clone::clone)
}

/// Is GDK's default display a Wayland one? (An X11 session and XWayland, `GDK_BACKEND=x11`, are not.) Main thread.
pub fn gdk_is_wayland() -> bool {
    gtk::gdk::Display::default().is_some_and(|d| d.type_().name() == "GdkWaylandDisplay")
}

/// For the capability check, on the main thread: why GDK's connection cannot be wrapped, and the required globals the
/// compositor lacks.
pub fn probe() -> (Option<String>, Vec<String>) {
    match wl() {
        Ok(_) => (None, vec![]),
        Err(e) => match e.strip_prefix(MISSING) {
            Some(list) => (None, list.split(", ").map(str::to_string).collect()),
            None => (Some(e.clone()), vec![]),
        },
    }
}

/// Has the toplevel been bound (so [`scale`] is the compositor's answer, not a guess)?
pub fn ready() -> bool {
    lock(&PARENT).is_some()
}

/// Connect and bind the toplevel now, then say the scale. Main thread. Before the first terminal is made, so its font
/// and swapchain are sized at the right scale from the start.
pub fn ensure() -> Result<f64, String> {
    let w = wl()?;
    let win = main_window()?;
    GTK_SCALE.store(win.scale_factor().max(1) as u32, Ordering::Relaxed);
    sync_parent(w, &win)?;
    Ok(scale())
}

/// Make sure the parent binding (the toplevel surface and its fractional-scale object) matches GDK's current toplevel.
/// Returns whether the toplevel surface changed since the last call (the terminals' subsurfaces are children of the old
/// one and must be remade).
fn sync_parent(w: &Wl, win: &gtk::ApplicationWindow) -> Result<bool, String> {
    let ptr = toplevel_surface(win).ok_or("the window has no Wayland surface yet")? as usize;
    let mut p = lock(&PARENT);
    if p.as_ref().is_some_and(|p| p.ptr == ptr) {
        return Ok(false);
    }
    let changed = p.is_some();
    // SAFETY: `ptr` is GDK's live wl_surface; the foreign proxy is only passed as an argument and never destroyed here.
    let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), ptr as *mut _) }
        .map_err(|e| e.to_string())?;
    let surface = WlSurface::from_id(&w.conn, id).map_err(|e| e.to_string())?;
    SCALE120.store(0, Ordering::Relaxed);
    let frac = w
        .frac
        .as_ref()
        .map(|m| m.get_fractional_scale(&surface, &w.qh, ()));
    *p = Some(Parent {
        ptr,
        surface,
        _frac: frac,
    });
    drop(p);
    // The preferred scale arrives as the answer to the request above.
    let mut q = lock(&w.queue);
    let (queue, state) = &mut *q;
    let _ = queue.roundtrip(state);
    Ok(changed)
}

/// The scale the terminals draw at: the compositor's fractional preference, else GTK's integer scale. Any thread.
pub fn scale() -> f64 {
    match SCALE120.load(Ordering::Relaxed) {
        0 => f64::from(GTK_SCALE.load(Ordering::Relaxed).max(1)),
        v => geometry::scale_from_120(v),
    }
}

/// Where the webview sits in the toplevel (a menu bar or a frame above it); (0, 0) when it fills the window.
fn webview_offset(win: &gtk::ApplicationWindow) -> (i32, i32) {
    fn find(w: &gtk::Widget) -> Option<gtk::Widget> {
        if w.clone().downcast::<webkit2gtk::WebView>().is_ok() {
            return Some(w.clone());
        }
        w.clone()
            .downcast::<gtk::Container>()
            .ok()?
            .children()
            .iter()
            .find_map(find)
    }
    find(win.upcast_ref())
        .and_then(|v| v.translate_coordinates(win, 0, 0))
        .unwrap_or((0, 0))
}

/// One terminal's child surface.
pub struct Sub {
    surface: WlSurface,
    subsurface: WlSubsurface,
    viewport: WpViewport,
    /// The rect last placed (without a slide in flight), in the toplevel's units.
    rect: Mutex<Rect>,
    /// A slide in flight; a new placement cancels it.
    slide: Mutex<Option<gtk::glib::SourceId>>,
}

impl Sub {
    /// The handles wgpu needs for the Vulkan surface: GDK's display and this child surface.
    pub fn raw_handles(
        &self,
    ) -> Result<
        (
            raw_window_handle::RawDisplayHandle,
            raw_window_handle::RawWindowHandle,
        ),
        String,
    > {
        let w = wl()?;
        let surface =
            NonNull::new(self.surface.id().as_ptr()).ok_or("the child surface has no proxy")?;
        let display = NonNull::new(w.display as *mut c_void).ok_or("no display")?;
        Ok((
            raw_window_handle::RawDisplayHandle::Wayland(
                raw_window_handle::WaylandDisplayHandle::new(display),
            ),
            raw_window_handle::RawWindowHandle::Wayland(
                raw_window_handle::WaylandWindowHandle::new(surface.cast()),
            ),
        ))
    }

    fn place(&self, w: &Wl, r: Rect) {
        self.subsurface.set_position(r.x, r.y);
        self.viewport.set_destination(r.w.max(1), r.h.max(1));
        // Position and stacking are applied with the PARENT's next commit: make it now instead of when GDK next draws.
        if let Some(p) = lock(&PARENT).as_ref() {
            p.surface.commit();
        }
        let _ = w.conn.flush();
    }
}

/// A new child surface of the toplevel, parked off screen, destination `w` x `h` logical. Main thread.
pub fn create(w: f64, h: f64) -> Result<usize, String> {
    let wl = wl()?;
    let win = main_window()?;
    GTK_SCALE.store(win.scale_factor().max(1) as u32, Ordering::Relaxed);
    sync_parent(wl, &win)?;
    let parent = lock(&PARENT)
        .as_ref()
        .map(|p| (p.surface.clone(), p.ptr))
        .ok_or("no parent")?;
    let surface = wl.compositor.create_surface(&wl.qh, ());
    let subsurface = wl
        .subcompositor
        .get_subsurface(&surface, &parent.0, &wl.qh, ());
    // Its own clock: the terminal commits when it has drawn, never waiting for the page's frame.
    subsurface.set_desync();
    subsurface.place_below(&parent.0);
    // No input: pointer and keyboard stay with the webview.
    let none = wl.compositor.create_region(&wl.qh, ());
    surface.set_input_region(Some(&none));
    none.destroy();
    let viewport = wl.viewporter.get_viewport(&surface, &wl.qh, ());
    let sub = Sub {
        surface,
        subsurface,
        viewport,
        rect: Mutex::new(Rect {
            x: OFFSCREEN,
            y: OFFSCREEN,
            w: w.round().max(1.0) as i32,
            h: h.round().max(1.0) as i32,
        }),
        slide: Mutex::new(None),
    };
    sub.place(wl, *lock(&sub.rect));
    LIVE.fetch_add(1, Ordering::AcqRel);
    start_tick();
    Ok(Box::into_raw(Box::new(sub)) as usize)
}

/// # Safety
/// `view` came from [`create`] and has not been destroyed.
unsafe fn sub<'a>(view: usize) -> &'a Sub {
    // SAFETY: the caller's contract.
    unsafe { &*(view as *const Sub) }
}

/// Move and resize the child to the slot rect (logical CSS px of the page), or park it (`x`, `y` = the parking spot).
/// Main thread.
pub fn set_frame(view: usize, x: f64, y: f64, w: f64, h: f64) {
    let Ok(wl) = wl() else { return };
    // SAFETY: the registry hands out live views only.
    let s = unsafe { sub(view) };
    cancel_slide(s);
    let parked = x <= f64::from(OFFSCREEN) / 2.0;
    let r = if parked {
        Rect {
            x: OFFSCREEN,
            y: OFFSCREEN,
            w: w.round().max(1.0) as i32,
            h: h.round().max(1.0) as i32,
        }
    } else {
        let off = main_window()
            .map(|win| webview_offset(&win))
            .unwrap_or((0, 0));
        geometry::quantize(x, y, w, h, off)
    };
    *lock(&s.rect) = r;
    s.place(wl, r);
}

fn cancel_slide(s: &Sub) {
    if let Some(id) = lock(&s.slide).take() {
        id.remove();
    }
}

/// The pane slid right by `dx` over `ms`: the child is already at its final rect; play the page's slide on it, from `dx`
/// to the left of it, with the page's own easing, one placement per display frame. Main thread.
pub fn slide_from(view: usize, dx: f64, ms: f64) {
    let Ok(wl) = wl() else { return };
    // SAFETY: the registry hands out live views only.
    let s = unsafe { sub(view) };
    cancel_slide(s);
    let end = *lock(&s.rect);
    if end.x <= OFFSCREEN / 2 || ms < 1.0 || dx.abs() < 0.5 {
        return;
    }
    let t0 = Instant::now();
    let addr = view;
    let id = gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
        // SAFETY: the source is removed by `cancel_slide` before `destroy` frees the view.
        let s = unsafe { sub(addr) };
        let t = (t0.elapsed().as_secs_f64() * 1000.0 / ms).min(1.0);
        let left = dx * (1.0 - geometry::ease_out(t));
        s.place(
            wl,
            Rect {
                x: end.x - left.round() as i32,
                ..end
            },
        );
        if t >= 1.0 {
            lock(&s.slide).take();
            gtk::glib::ControlFlow::Break
        } else {
            gtk::glib::ControlFlow::Continue
        }
    });
    *lock(&s.slide) = Some(id);
}

/// Destroy the child surface. The renderer drawing into it must already be dropped. Main thread.
pub fn destroy(view: usize) {
    // SAFETY: balances `Box::into_raw` in `create`; the caller passes each view once.
    let s = unsafe { Box::from_raw(view as *mut Sub) };
    cancel_slide(&s);
    s.viewport.destroy();
    s.subsurface.destroy();
    s.surface.destroy();
    if let Ok(w) = wl() {
        let _ = w.conn.flush();
    }
    LIVE.fetch_sub(1, Ordering::AcqRel);
}

/// While terminals exist: once a second take what GDK's reads left on our queue (a fractional scale change when the
/// window moves to another output), and notice that GDK made a new toplevel surface (the window was hidden and shown).
/// Either one means every terminal must be drawn again (`super::super::rescale_all`).
fn start_tick() {
    if TICKING.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut seen = scale();
    gtk::glib::timeout_add_seconds_local(1, move || {
        if LIVE.load(Ordering::Acquire) == 0 {
            TICKING.store(false, Ordering::Release);
            return gtk::glib::ControlFlow::Break;
        }
        let (Ok(w), Ok(win)) = (wl(), main_window()) else {
            return gtk::glib::ControlFlow::Continue;
        };
        w.pump();
        GTK_SCALE.store(win.scale_factor().max(1) as u32, Ordering::Relaxed);
        let moved = sync_parent(w, &win).unwrap_or(false);
        let now = scale();
        if moved || (now - seen).abs() > 1e-6 {
            seen = now;
            super::super::rescale_all();
        }
        gtk::glib::ControlFlow::Continue
    });
}

/// The Vulkan-surface handles of `view`.
pub fn raw_handles(
    view: usize,
) -> Result<
    (
        raw_window_handle::RawDisplayHandle,
        raw_window_handle::RawWindowHandle,
    ),
    String,
> {
    // SAFETY: the registry hands out live views only.
    unsafe { sub(view) }.raw_handles()
}
