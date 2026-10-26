//! The native stage, AppKit side: draws the page slide from cached snapshots, moved at display rate.
//!
//! Decisions live in `page_stage.rs` (pure, tested); this file holds the snapshots (`CGImage`s taken with
//! `WKWebView.takeSnapshot` of the pager rect), the overlay `CALayer` tree and executes what the stage returns.
//! The overlay sits just above the main webview's layer and below the native child views, clipped to the pager
//! rect, so the sidebar and chrome stay live. Layers move in a `CATransaction` with actions disabled: the window
//! server composites them every display frame, whatever WebContent is doing.
//!
//! JS owns the DOM, so it owns the *warm-up*: to snapshot a page that is parked out of view it first asks for a
//! **cover** (the page on screen is snapshotted and the overlay raised with it, so whatever JS does underneath is
//! invisible), shows the page, asks for its snapshot, parks it again and lifts the cover. A gesture that starts
//! while a warm-up runs wins: the cover is already the right picture, so it simply becomes the slide.
//!
//! Main thread only. The statics are for Tauri commands, which hop to the main thread.

use std::collections::HashMap;
use std::sync::mpsc;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSImage, NSWindow};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSError, NSNumber, NSPoint, NSRect, NSSize};
use objc2_quartz_core::{CALayer, CATransaction};
use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};
use parking_lot::Mutex;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::page_stage::{Do, Stage};

/// The pager's viewport in the main webview (CSS px = points, top-left origin).
#[derive(Clone, Copy, Deserialize, Default, Debug, PartialEq)]
pub struct PagerRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

struct Shell {
    stage: Stage,
    rect: PagerRect,
    /// Retained `CGImage*` per page id (as usize: AppKit objects are main-thread, the map only moves pointers).
    images: HashMap<String, usize>,
}

struct Overlay {
    root: Retained<CALayer>,
    cur: Retained<CALayer>,
    inc: Retained<CALayer>,
}

/// How many snapshots / covers / pages pushes ran (the bench reads them: warm-up work must not pile up).
static SNAPSHOTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static COVERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

static SHELL: Mutex<Option<Shell>> = Mutex::new(None);

thread_local! {
    /// The overlay layers: AppKit objects, main thread only (every caller hops there).
    static OVERLAY: std::cell::RefCell<Option<Overlay>> = const { std::cell::RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Shell) -> R) -> R {
    let mut g = SHELL.lock();
    let sh = g.get_or_insert_with(|| Shell {
        stage: Stage::default(),
        rect: PagerRect::default(),
        images: HashMap::new(),
    });
    f(sh)
}

fn main_webview(app: &AppHandle) -> Option<(Retained<WKWebView>, Retained<NSWindow>)> {
    let win = app.get_window("main")?.ns_window().ok()?;
    // SAFETY: Tauri's live main NSWindow, on the main thread.
    let win = unsafe { Retained::retain(win as *mut NSWindow)? };
    let content = win.contentView()?;
    // The main webview is the content view's first WKWebView subview.
    for v in content.subviews().iter() {
        let any: &AnyObject = &v;
        if any.class().name().to_string_lossy().contains("WKWebView")
            || any.class().name().to_string_lossy().contains("WryWebView")
        {
            // SAFETY: class checked above; WKWebView subclasses WryWebView.
            let wk = unsafe { Retained::retain(Retained::as_ptr(&v) as *mut WKWebView)? };
            return Some((wk, win));
        }
    }
    None
}

fn cg_of(img: &NSImage) -> Option<Retained<CGImage>> {
    // SAFETY: plain getter; null rect/context/hints are allowed.
    unsafe { img.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None) }
}

/// Snapshot the pager rect now and hand the `CGImage` to `done` on the main thread.
fn snapshot(
    app: &AppHandle,
    rect: PagerRect,
    scale: f64,
    done: impl FnOnce(Option<Retained<CGImage>>) + 'static,
) {
    let Some((wk, _)) = main_webview(app) else {
        return done(None);
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return done(None);
    };
    let done = std::cell::RefCell::new(Some(done));
    let block = RcBlock::new(move |img: *mut NSImage, _e: *mut NSError| {
        // SAFETY: WebKit passes a valid image or null for the callback's duration.
        let cg = unsafe { img.as_ref() }.and_then(cg_of);
        if let Some(d) = done.borrow_mut().take() {
            d(cg);
        }
    });
    // SAFETY: live WKWebView on the main thread; the block is copied by WebKit.
    unsafe {
        let cfg = WKSnapshotConfiguration::new(mtm);
        cfg.setAfterScreenUpdates(false);
        cfg.setRect(NSRect::new(
            NSPoint::new(rect.x, rect.y),
            NSSize::new(rect.w, rect.h),
        ));
        // snapshotWidth is in points: the image has `width * backing scale` pixels, so `scale` < 1 renders fewer.
        cfg.setSnapshotWidth(Some(&NSNumber::new_f64((rect.w * scale).round().max(1.0))));
        wk.takeSnapshotWithConfiguration_completionHandler(Some(&cfg), &block);
    }
}

fn set_contents(layer: &CALayer, image: Option<usize>) {
    // SAFETY: `image` is a retained CGImage* kept alive by the shell's map for as long as it is set.
    unsafe {
        let any = image.map(|p| &*(p as *const AnyObject));
        layer.setContents(any);
    }
}

fn layer_frame(rect: PagerRect, window_h: f64) -> NSRect {
    // Core Animation's y is bottom-up in the content view's layer.
    NSRect::new(
        NSPoint::new(rect.x, window_h - rect.y - rect.h),
        NSSize::new(rect.w, rect.h),
    )
}

fn raise(app: &AppHandle, sh: &mut Shell, cur: &str, inc: Option<&str>) {
    let Some((wk, win)) = main_webview(app) else {
        return;
    };
    let Some(content) = win.contentView() else {
        return;
    };
    let Some(host) = content.layer() else { return };
    let h = content.bounds().size.height;
    let frame = layer_frame(sh.rect, h);
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    let root = CALayer::new();
    root.setFrame(frame);
    root.setMasksToBounds(true);
    let scale = win.backingScaleFactor();
    let make = |image: Option<usize>| {
        let l = CALayer::new();
        l.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
        l.setContentsScale(scale);
        set_contents(&l, image);
        root.addSublayer(&l);
        l
    };
    let cur_l = make(sh.images.get(cur).copied());
    let inc_l = make(inc.and_then(|i| sh.images.get(i).copied()));
    // Above the webview, below the native child views (terminal, browser): the sidebar stays live.
    match wk.layer() {
        Some(wl) => host.insertSublayer_above(&root, Some(&wl)),
        None => host.addSublayer(&root),
    }
    CATransaction::commit();
    OVERLAY.with(|o| {
        *o.borrow_mut() = Some(Overlay {
            root,
            cur: cur_l,
            inc: inc_l,
        })
    });
}

fn run(app: &AppHandle, sh: &mut Shell, todo: Vec<Do>) {
    for d in todo {
        match d {
            Do::Raise { cur, inc } => raise(app, sh, &cur, inc.as_deref()),
            Do::Move { cur_x, inc_x, inc } => {
                let w = sh.rect.w;
                let (h, img) = (sh.rect.h, inc.and_then(|i| sh.images.get(&i).copied()));
                OVERLAY.with(|o| {
                    let Some(o) = &*o.borrow() else { return };
                    CATransaction::begin();
                    CATransaction::setDisableActions(true);
                    o.cur
                        .setPosition(CGPoint::new(w / 2.0 + cur_x * w, h / 2.0));
                    o.inc
                        .setPosition(CGPoint::new(w / 2.0 + inc_x * w, h / 2.0));
                    set_contents(&o.inc, img);
                    CATransaction::commit();
                });
            }
            Do::Rebase { cur } => {
                let img = sh.images.get(&cur).copied();
                OVERLAY.with(|o| {
                    if let Some(o) = &*o.borrow() {
                        CATransaction::begin();
                        CATransaction::setDisableActions(true);
                        set_contents(&o.cur, img);
                        CATransaction::commit();
                    }
                });
            }
            Do::AwaitAck { seq } => {
                let _ = app.emit("pager://await-ack", seq);
            }
            Do::Lower => {
                if let Some(o) = OVERLAY.with(|o| o.borrow_mut().take()) {
                    CATransaction::begin();
                    CATransaction::setDisableActions(true);
                    o.root.removeFromSuperlayer();
                    CATransaction::commit();
                }
            }
        }
    }
}

// ---- called by the swipe adapter (main thread) -------------------------------------------------------------

/// An engine frame of the space zone. True when the stage draws it (JS must not move layers).
pub fn on_frame(
    app: &AppHandle,
    seq: u64,
    amount: f64,
    rest: bool,
    target: Option<&str>,
    now: f64,
) -> bool {
    with(|sh| {
        let todo = sh.stage.frame(seq, amount, rest, target, now);
        let drawn = sh.stage.is_up();
        run(app, sh, todo);
        drawn
    })
}

/// The overlay is still up (waiting for JS's ack): the clock keeps ticking for the watchdog.
pub fn is_up() -> bool {
    with(|sh| sh.stage.is_up())
}

pub fn on_commit(seq: u64, dir: i8) {
    with(|sh| sh.stage.commit(seq, dir));
}

/// Watchdog; the engine clock calls it while a gesture is in flight.
pub fn on_tick(app: &AppHandle, now: f64) {
    with(|sh| {
        let todo = sh.stage.tick(now);
        run(app, sh, todo);
    });
}

// ---- commands (JS → native) --------------------------------------------------------------------------------

fn on_main<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .ok()?;
    rx.recv().ok()
}

/// The pages in order and the active one; the pager's viewport rect.
#[tauri::command]
pub async fn pager_pages(
    app: AppHandle,
    order: Vec<String>,
    active: Option<String>,
    rect: PagerRect,
) {
    on_main(&app, move || {
        with(|sh| {
            sh.rect = rect;
            sh.stage.set_pages(order.clone(), active);
            sh.images.retain(|id, _| order.contains(id));
        })
    });
}

/// Snapshot the pager rect as page `id` (JS has `id` in view). Resolves once cached.
#[tauri::command]
pub async fn pager_snapshot(app: AppHandle, id: String, scale: Option<f64>) -> bool {
    SNAPSHOTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    let a = app.clone();
    let queued = app
        .run_on_main_thread(move || {
            let rect = with(|sh| sh.rect);
            snapshot(&a, rect, scale.unwrap_or(1.0), move |cg| {
                let ok = cg.is_some();
                if let Some(cg) = cg {
                    with(|sh| {
                        if let Some(old) = sh
                            .images
                            .insert(id.clone(), Retained::into_raw(cg) as usize)
                        {
                            // SAFETY: balances the `into_raw` that stored it.
                            drop(unsafe { Retained::from_raw(old as *mut CGImage) });
                        }
                        sh.stage.snapped(&id, true);
                    });
                }
                let _ = tx.send(ok);
            });
        })
        .is_ok();
    if !queued {
        return false;
    }
    tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// The page changed: its snapshot is stale (kept as a placeholder until refreshed).
#[tauri::command]
pub async fn pager_invalidate(app: AppHandle, id: String) {
    on_main(&app, move || with(|sh| sh.stage.invalidate(&id)));
}

/// Raise the overlay with a snapshot of what is on screen, still: whatever JS does under it is invisible. A
/// gesture that starts while it is up simply slides it (the stage's first frame finds it raised).
#[tauri::command]
pub async fn pager_cover(app: AppHandle) -> bool {
    COVERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    let a = app.clone();
    let queued = app
        .run_on_main_thread(move || {
            let (rect, already) = with(|sh| {
                (
                    sh.rect,
                    sh.stage.is_up() || OVERLAY.with(|o| o.borrow().is_some()),
                )
            });
            if already {
                let _ = tx.send(false);
                return;
            }
            let a2 = a.clone();
            snapshot(&a, rect, 1.0, move |cg| {
                let Some(cg) = cg else {
                    let _ = tx.send(false);
                    return;
                };
                with(|sh| {
                    let key = "\u{0}cover".to_string();
                    if let Some(old) = sh
                        .images
                        .insert(key.clone(), Retained::into_raw(cg) as usize)
                    {
                        // SAFETY: balances the `into_raw` that stored it.
                        drop(unsafe { Retained::from_raw(old as *mut CGImage) });
                    }
                    raise(&a2, sh, &key, None);
                    sh.stage.cover();
                });
                let _ = tx.send(true);
            });
        })
        .is_ok();
    if !queued {
        return false;
    }
    tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// Lift the cover raised by `pager_cover` (never an overlay a gesture is drawing).
#[tauri::command]
pub async fn pager_uncover(app: AppHandle) {
    let a = app.clone();
    on_main(&app, move || {
        with(|sh| {
            let todo = sh.stage.uncover();
            run(&a, sh, todo);
        })
    });
}

/// What the stage holds, for the bench and `atlas desktop debug`: overlay up, snapshots cached.
#[tauri::command]
pub async fn pager_state(app: AppHandle) -> serde_json::Value {
    on_main(&app, || {
        let overlay = OVERLAY.with(|o| o.borrow().is_some());
        with(|sh| {
            let mut ids: Vec<&String> = sh.images.keys().filter(|k| !k.starts_with('\u{0}')).collect();
            ids.sort();
            serde_json::json!({ "overlay": overlay, "snapshots": ids, "rect": [sh.rect.x, sh.rect.y, sh.rect.w, sh.rect.h], "zones": crate::swipe_mac::zones_json(), "snapshotsTaken": SNAPSHOTS.load(std::sync::atomic::Ordering::Relaxed), "coversRaised": COVERS.load(std::sync::atomic::Ordering::Relaxed) })
        })
    })
    .unwrap_or(serde_json::Value::Null)
}

/// JS applied the committed page and it painted: the overlay may come down.
#[tauri::command]
pub async fn pager_ready(app: AppHandle, seq: u64) {
    let a = app.clone();
    on_main(&app, move || {
        with(|sh| {
            let todo = sh.stage.ack(seq);
            run(&a, sh, todo);
        })
    });
}
