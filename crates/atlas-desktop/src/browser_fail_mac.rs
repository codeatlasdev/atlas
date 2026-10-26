//! WebKit side of `browser_fail`: reports a failed navigation of a browser tab.
//!
//! wry installs its own `WKNavigationDelegate` and implements neither failure callback. WebKit asks the delegate
//! `respondsToSelector:` at call time, so the two optional methods are added to wry's delegate class once with
//! `class_addMethod`: nothing of wry is replaced or swizzled, it only learns two selectors it never had. The
//! callback finds its tab by the `WKWebView*` it is given (`track`).
//!
//! The failing URL comes from the error (`NSURLErrorFailingURLStringErrorKey`, whose string value is `NSErrorFailingURLStringKey`): on a provisional failure
//! `webView.URL` still holds the page before it.

use std::sync::Once;

use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyObject, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSError, NSString};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

/// `(WKWebView*, app, tab)` of every live browser child. Pointers are only compared, never dereferenced.
static TABS: Mutex<Vec<(usize, AppHandle, String)>> = Mutex::new(Vec::new());
static HOOK: Once = Once::new();

/// Report failed loads of the child webview `v` (tab `tab`) as `browser://<tab>` `fail` events. Runs on the main thread.
pub fn track(app: &AppHandle, v: &tauri::Webview, tab: String) {
    let app = app.clone();
    let _ = v.with_webview(move |pw| {
        let view = pw.inner() as usize;
        {
            let mut tabs = TABS.lock();
            tabs.retain(|(p, ..)| *p != view);
            tabs.push((view, app, tab));
        }
        // SAFETY: `inner()` is the live WKWebView*; with_webview runs on the main thread.
        let delegate: *mut AnyObject =
            unsafe { msg_send![view as *mut AnyObject, navigationDelegate] };
        if let Some(d) = unsafe { delegate.as_ref() } {
            let cls = d.class();
            HOOK.call_once(|| install(cls as *const _ as *mut _));
        }
    });
}

/// Forget a closed tab's view.
pub fn untrack(tab: &str) {
    TABS.lock().retain(|(_, _, t)| t != tab);
}

/// `v@:@@@` = (void) self, _cmd, WKWebView*, WKNavigation*, NSError*.
const TYPES: &std::ffi::CStr = c"v@:@@@";

fn install(cls: *mut objc2::runtime::AnyClass) {
    type Cb = unsafe extern "C-unwind" fn(
        *mut AnyObject,
        Sel,
        *mut AnyObject,
        *mut AnyObject,
        *mut AnyObject,
    );
    let provisional: Cb = did_fail;
    let committed: Cb = did_fail;
    // SAFETY: both selectors are optional `WKNavigationDelegate` methods wry's class does not define; the
    // signature matches `TYPES`. A false return (already defined) leaves wry's own implementation in place.
    unsafe {
        class_addMethod(
            cls,
            sel!(webView:didFailProvisionalNavigation:withError:),
            std::mem::transmute::<Cb, Imp>(provisional),
            TYPES.as_ptr(),
        );
        class_addMethod(
            cls,
            sel!(webView:didFailNavigation:withError:),
            std::mem::transmute::<Cb, Imp>(committed),
            TYPES.as_ptr(),
        );
    }
}

unsafe extern "C-unwind" fn did_fail(
    _this: *mut AnyObject,
    _cmd: Sel,
    view: *mut AnyObject,
    _navigation: *mut AnyObject,
    error: *mut AnyObject,
) {
    let Some(error) = (unsafe { (error as *const NSError).as_ref() }) else {
        return;
    };
    let hit = TABS
        .lock()
        .iter()
        .find(|(p, ..)| *p == view as usize)
        .map(|(_, app, tab)| (app.clone(), tab.clone()));
    let Some((app, tab)) = hit else {
        return;
    };
    let url = failing_url(error)
        .or_else(|| current_url(view))
        .unwrap_or_default();
    let payload = crate::browser_fail::payload(
        &url,
        error.code() as i64,
        &error.domain().to_string(),
        &error.localizedDescription().to_string(),
    );
    let _ = app.emit(&format!("browser://{tab}"), payload);
}

/// `userInfo[NSURLErrorFailingURLStringErrorKey]` (`"NSErrorFailingURLStringKey"`).
fn failing_url(error: &NSError) -> Option<String> {
    let info = error.userInfo();
    let key = NSString::from_str("NSErrorFailingURLStringKey");
    let v = info.objectForKey(&key)?;
    let s: &NSString = v.downcast_ref()?;
    Some(s.to_string())
}

/// `webView.URL.absoluteString`, for an error that names no URL.
fn current_url(view: *mut AnyObject) -> Option<String> {
    // SAFETY: `view` is the WKWebView WebKit passed to the callback.
    let url: *mut AnyObject = unsafe { msg_send![view, URL] };
    let url = unsafe { url.as_ref() }?;
    let s: *mut NSString = unsafe { msg_send![url, absoluteString] };
    unsafe { s.as_ref() }.map(|s| s.to_string())
}
