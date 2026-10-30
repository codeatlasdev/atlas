//! WebView2 side of `browser_keys`: the embedded browser's key path on Windows.
//!
//! A browser tab is a child WebView2: the main webview's keymap (`windowKeys.ts`) never sees its keys, so Ctrl+1…9,
//! Ctrl+Tab, Ctrl+PgUp/PgDn, Ctrl+Shift+T/W … did nothing while a page had focus. `AcceleratorKeyPressed` fires on the
//! child's controller before the page handles a key; the handler here decides with [`browser_keys::forward_off_mac`]
//! (the same pushed stroke list macOS uses) and, for a bound key, marks it handled (the page never sees it) and emits
//! `browser://key`. Every other key (typing, Ctrl+F, F5, Ctrl+C/V …) is left to the page, whose own accelerators stay
//! on: users expect Ctrl+F and F5 in a browser tab.

use tauri::{AppHandle, Emitter};
use webview2_com::AcceleratorKeyPressedEventHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RMENU, VK_RWIN, VK_SHIFT,
};

use crate::browser_keys::{EVENT, Mods, dom_code_win, forward_off_mac};

/// Is the virtual key down right now (high bit of `GetKeyState`)? The accelerator event carries no modifier state,
/// but it fires synchronously on the UI thread while the key is pressed, so the thread's key state is the event's.
fn down(vk: u16) -> bool {
    // SAFETY: plain user32 call with a constant virtual-key code.
    unsafe { GetKeyState(i32::from(vk)) < 0 }
}

/// Forward bound keys of the browser child `v` to the app. Call right after the webview is created.
pub fn install(app: &AppHandle, v: &tauri::Webview) {
    let app = app.clone();
    let _ = v.with_webview(move |pw| unsafe {
        let mut token = 0i64;
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = Default::default();
            args.KeyEventKind(&mut kind)?;
            // Alt chords arrive as system keys; releases and repeats of a handled key follow its press.
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }
            let mut vk = 0u32;
            args.VirtualKey(&mut vk)?;
            let Some(code) = dom_code_win(vk) else {
                return Ok(());
            };
            let mods = Mods {
                ctrl: down(VK_CONTROL.0),
                alt: down(VK_MENU.0),
                shift: down(VK_SHIFT.0),
                meta: down(VK_LWIN.0) || down(VK_RWIN.0),
            };
            // AltGr is Ctrl+right Alt: a chord that types a character is text (`keys.ts` `strokesOf`).
            let typed = mods.ctrl && mods.alt && down(VK_RMENU.0);
            if let Some(f) = forward_off_mac(code, mods, typed) {
                args.SetHandled(true)?;
                let _ = app.emit(EVENT, f);
            }
            Ok(())
        }));
        let _ = pw
            .controller()
            .add_AcceleratorKeyPressed(&handler, &mut token);
    });
}
