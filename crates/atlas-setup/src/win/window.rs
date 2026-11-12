//! The one Win32 window: a borderless, per-pixel-alpha popup that shows what the worker thread
//! reports. Painting is `ui::render` into a DIB, presented with `UpdateLayeredWindow`; the rest is
//! hit testing, a 60 fps timer, dragging, the close box and "Copy details".

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HGDIOBJ,
    ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, VK_ESCAPE, VK_RETURN};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetMessageW, GetSystemMetrics, HTCAPTION, HTCLIENT, IDC_ARROW, KillTimer,
    LoadCursorW, MSG, PostQuitMessage, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL,
    SetCursor, SetTimer, ShowWindow, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_CLOSE,
    WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCHITTEST, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

use crate::progress::ease;
use crate::state::{Phase, Shared};
use crate::ui::font::Fonts;
use crate::ui::layout::{self, Button, Hit};
use crate::ui::render::{self, Frame, SHADOW};

const TIMER_ID: usize = 1;
const FRAME_MS: u32 = 16;
/// Seconds the "ready" screen stays before the window closes by itself.
const DONE_LINGER: f32 = 1.4;
const CF_UNICODETEXT: u32 = 13;

struct App {
    shared: Shared,
    fonts: Fonts,
    started: Instant,
    last: Instant,
    shown: f32,
    done_at: Option<Instant>,
    hover: Option<Button>,
    close_hover: bool,
    scale: f32,
    /// Quit after this many seconds in `Done` (0: never; a silent run has no window at all).
    auto_close: bool,
    /// What the process exits with: 0 only after a successful run.
    exit_code: i32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn font_dir() -> std::path::PathBuf {
    std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:\\Windows".into())
        .join("Fonts")
}

/// Show the window until it closes. Returns the process exit code: 0 when the run succeeded.
pub fn run(shared: Shared, auto_close: bool) -> i32 {
    // The manifest already asks for per-monitor v2; this covers a manifest-less dev build.
    // SAFETY: no preconditions; failure (already set) is fine.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // SAFETY: null asks for this module's handle.
    let hinst = unsafe { GetModuleHandleW(null()) };
    let class = wide("AtlasSetupWindow");
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        // SAFETY: a stock cursor id.
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hIcon: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    // SAFETY: `wc` is fully initialised and `class` outlives the call.
    unsafe { RegisterClassW(&wc) };

    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            shared,
            fonts: Fonts::from_dir(&font_dir()),
            started: Instant::now(),
            last: Instant::now(),
            shown: 0.0,
            done_at: None,
            hover: None,
            close_hover: false,
            scale: 1.0,
            auto_close,
            exit_code: 1,
        })
    });

    // Initial size at 100 %; the first paint resizes to the monitor's real scale.
    let title = wide("Atlas Setup");
    // SAFETY: valid class and title; no parent.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            (layout::WIDTH + 2.0 * SHADOW) as i32,
            (layout::HEIGHT + 2.0 * SHADOW) as i32,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        )
    };
    if hwnd.is_null() {
        return 1;
    }
    // SAFETY: `hwnd` is ours.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    set_scale(dpi as f32 / 96.0);
    place_center(hwnd);
    paint(hwnd);
    // SAFETY: `hwnd` is ours.
    unsafe {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetTimer(hwnd, TIMER_ID, FRAME_MS, None);
    }

    let mut msg = MSG {
        hwnd: null_mut(),
        message: 0,
        wParam: 0,
        lParam: 0,
        time: 0,
        pt: POINT { x: 0, y: 0 },
    };
    // SAFETY: standard message loop on the thread that created the window.
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        // SAFETY: `msg` was filled by GetMessageW.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    APP.with(|a| a.borrow().as_ref().map_or(1, |a| a.exit_code))
}

/// A plain system message box, for a command line the window never opened for.
pub fn message_box(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let (t, m) = (wide(title), wide(text));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe { MessageBoxW(null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

fn set_scale(s: f32) {
    with_app(|a| a.scale = s);
}

fn place_center(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SWP_NOSIZE, SWP_NOZORDER, SetWindowPos};
    let scale = with_app(|a| a.scale).unwrap_or(1.0);
    let (w, h) = (
        ((layout::WIDTH + 2.0 * SHADOW) * scale) as i32,
        ((layout::HEIGHT + 2.0 * SHADOW) * scale) as i32,
    );
    // SAFETY: plain metric queries.
    let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    // SAFETY: `hwnd` is ours.
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            (sw - w) / 2,
            (sh - h) / 2 - sh / 20,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER,
        )
    };
}

/// Render the current frame and hand it to the layered window.
fn paint(hwnd: HWND) {
    let Some((bgra, w, h)) = with_app(|a| {
        let snap = a.shared.snapshot();
        let now = Instant::now();
        let dt = now.duration_since(a.last).as_secs_f32();
        a.last = now;
        a.shown = if snap.phase == Phase::Done {
            1.0f32.min(ease(a.shown, 1.0, dt * 2.0))
        } else {
            ease(a.shown, snap.progress, dt)
        };
        let done_t = a
            .done_at
            .map_or(0.0, |t| now.duration_since(t).as_secs_f32());
        let frame = Frame {
            snap: &snap,
            shown: a.shown,
            t: now.duration_since(a.started).as_secs_f32(),
            done_t,
            hover: a.hover,
            close_hover: a.close_hover,
            scale: a.scale,
            fonts: &a.fonts,
        };
        let pm = render::paint_window(&frame);
        (render::to_bgra(&pm), pm.width() as i32, pm.height() as i32)
    }) else {
        return;
    };
    present(hwnd, &bgra, w, h);
}

fn present(hwnd: HWND, bgra: &[u8], w: i32, h: i32) {
    // SAFETY: standard GDI sequence; every created object is released below.
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [std::mem::zeroed()],
        };
        let mut bits: *mut core::ffi::c_void = null_mut();
        let bmp = CreateDIBSection(mem, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if !bmp.is_null() && !bits.is_null() {
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
            let old = SelectObject(mem, bmp as HGDIOBJ);
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(hwnd, screen, null(), &size, mem, &src, 0, &blend, ULW_ALPHA);
            SelectObject(mem, old);
            DeleteObject(bmp as HGDIOBJ);
        }
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
    }
}

fn client_to_logical(hwnd: HWND, lparam: LPARAM) -> (f32, f32) {
    let _ = hwnd;
    let scale = with_app(|a| a.scale).unwrap_or(1.0);
    let x = (lparam & 0xFFFF) as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
    (x / scale - SHADOW, y / scale - SHADOW)
}

fn copy_to_clipboard(hwnd: HWND, text: &str) {
    let data = wide(&text.replace('\n', "\r\n"));
    let bytes = data.len() * 2;
    // SAFETY: documented clipboard sequence; the HGLOBAL is handed to the system on success.
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return;
        }
        EmptyClipboard();
        let h: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !h.is_null() {
            let p = GlobalLock(h) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
                GlobalUnlock(h);
                SetClipboardData(CF_UNICODETEXT, h as _);
            }
        }
        CloseClipboard();
    }
}

/// What a click on `b` does.
fn activate(hwnd: HWND, b: Button) {
    match b {
        Button::Close | Button::Dismiss => close(hwnd),
        Button::CopyDetails => {
            let text = with_app(|a| {
                let s = a.shared.snapshot();
                format!("{}\n\n{}", s.status, s.details)
            })
            .unwrap_or_default();
            copy_to_clipboard(hwnd, &text);
        }
        Button::KeepData => {
            with_app(|a| a.shared.answer_data(false));
        }
        Button::RemoveData => {
            with_app(|a| a.shared.answer_data(true));
        }
    }
}

/// Close the window. Mid-install this is allowed (the NSIS child keeps going and finishes the
/// install); the exit code then says whether the run had succeeded by that moment.
fn close(hwnd: HWND) {
    with_app(|a| {
        let s = a.shared.snapshot();
        a.exit_code = i32::from(s.phase != Phase::Done);
        // A closed question window answers "keep": removing data is only ever an explicit click.
        if s.phase == Phase::AskData {
            a.shared.answer_data(false);
        }
    });
    // SAFETY: `hwnd` is ours.
    unsafe { DestroyWindow(hwnd) };
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            let snap = with_app(|a| {
                let s = a.shared.snapshot();
                if s.phase == Phase::Done && a.done_at.is_none() {
                    a.done_at = Some(Instant::now());
                }
                s.phase
            });
            paint(hwnd);
            if snap == Some(Phase::Done) {
                let linger =
                    with_app(|a| a.done_at.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0))
                        .unwrap_or(0.0);
                if linger >= DONE_LINGER && with_app(|a| a.auto_close).unwrap_or(false) {
                    close(hwnd);
                }
            }
            0
        }
        WM_NCHITTEST => {
            // Drag by anything that is not a button: HTCAPTION lets Windows move the window.
            let mut pt = POINT { x: 0, y: 0 };
            // SAFETY: plain cursor query.
            unsafe { GetCursorPos(&mut pt) };
            use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
            // SAFETY: `hwnd` is ours.
            unsafe { ScreenToClient(hwnd, &mut pt) };
            let scale = with_app(|a| a.scale).unwrap_or(1.0);
            let (x, y) = (pt.x as f32 / scale - SHADOW, pt.y as f32 / scale - SHADOW);
            let snap = with_app(|a| a.shared.snapshot());
            match snap.map(|s| layout::hit(&s, x, y)) {
                Some(Hit::Drag) => HTCAPTION as LRESULT,
                _ => HTCLIENT as LRESULT,
            }
        }
        WM_MOUSEMOVE => {
            let (x, y) = client_to_logical(hwnd, lparam);
            with_app(|a| {
                let s = a.shared.snapshot();
                match layout::hit(&s, x, y) {
                    Hit::Button(Button::Close) => {
                        a.close_hover = true;
                        a.hover = None;
                    }
                    Hit::Button(b) => {
                        a.hover = Some(b);
                        a.close_hover = false;
                    }
                    _ => {
                        a.hover = None;
                        a.close_hover = false;
                    }
                }
            });
            0
        }
        WM_LBUTTONDOWN => {
            // Buttons are HTCLIENT, so the click arrives here.
            let (x, y) = client_to_logical(hwnd, lparam);
            if let Some(s) = with_app(|a| a.shared.snapshot())
                && let Hit::Button(b) = layout::hit(&s, x, y)
            {
                activate(hwnd, b);
            }
            0
        }
        WM_LBUTTONUP => {
            // SAFETY: releases any capture a drag took.
            unsafe { ReleaseCapture() };
            0
        }
        WM_KEYDOWN => {
            let phase = with_app(|a| a.shared.snapshot().phase);
            match (wparam as u16, phase) {
                (VK_ESCAPE, Some(Phase::Failed | Phase::Done)) => close(hwnd),
                (VK_RETURN, Some(Phase::AskData)) => activate(hwnd, Button::KeepData),
                (VK_RETURN, Some(Phase::Failed)) => close(hwnd),
                (VK_ESCAPE, Some(Phase::AskData)) => activate(hwnd, Button::KeepData),
                _ => {}
            }
            0
        }
        WM_SETCURSOR => {
            // SAFETY: a stock cursor id.
            unsafe { SetCursor(LoadCursorW(null_mut(), IDC_ARROW)) };
            1
        }
        WM_DPICHANGED => {
            set_scale((wparam & 0xFFFF) as f32 / 96.0);
            place_center(hwnd);
            paint(hwnd);
            0
        }
        WM_CLOSE => {
            close(hwnd);
            0
        }
        WM_DESTROY => {
            // SAFETY: ends the message loop of this thread.
            unsafe {
                KillTimer(hwnd, TIMER_ID);
                PostQuitMessage(0);
            }
            0
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
