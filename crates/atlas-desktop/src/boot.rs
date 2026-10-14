//! Startup: the pre-paint theme and the "show the window when the first frame is real" gate.
//!
//! The frontend theme engine stays the single source of truth. Whenever it applies a theme it
//! persists the result with `theme_cache_write` (`~/.atlas/desktop/theme-cache.json`: the exact CSS
//! variables it set, the scheme and the surface colors). At launch this module reads that file
//! *before* the webview exists and
//!
//! - injects it as an initialization script, so `<html>` carries the user's variables, `.dark` and
//!   `color-scheme` before the first style computation: no default-theme frame, ever;
//! - pins the native window appearance (vibrancy material) to the cached scheme at creation.
//!
//! The window is created hidden and shown by `app_ready` (the frontend, after its first real
//! frame) or by a hard timeout, whichever comes first.
//!
//! `ATLAS_BOOT_TRACE=<file>` additionally appends `<ms since process start> <mark>` lines (Rust
//! marks here, frontend marks through `boot_mark`); unset = no-ops.

use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// The hidden window is shown this long after creation at the latest, ready or not: a stuck
/// frontend must not read as "the app did not open". Creation to ready takes ~380 ms on an M-class
/// Mac (WebContent spawn, page load, three IPCs), so a shorter net would fire on every normal
/// launch and show a half-built window; 1 s is only ever reached when something is wrong.
const SHOW_DEADLINE: Duration = Duration::from_millis(1000);

static T0: LazyLock<Instant> = LazyLock::new(Instant::now);
static TRACE: LazyLock<Option<String>> = LazyLock::new(|| std::env::var("ATLAS_BOOT_TRACE").ok());
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Call first thing in `main` so `T0` is the process start.
pub fn start() {
    LazyLock::force(&T0);
}

pub fn enabled() -> bool {
    TRACE.is_some()
}

pub fn mark(name: &str) {
    let Some(path) = TRACE.as_deref() else {
        return;
    };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{:.1} {name}", T0.elapsed().as_secs_f64() * 1000.0);
    }
}

#[tauri::command]
pub fn boot_mark(name: String) {
    mark(&format!("js:{name}"));
}

// ---------- theme cache ----------

/// What the frontend theme engine applied last (see `lib/theme/cache.ts`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThemeCache {
    /// Theme name, mirrored to `data-atlas-theme`.
    #[serde(default)]
    pub name: String,
    /// The kit's stored scheme preference: `dark`, `light` or `system`.
    #[serde(default = "system")]
    pub scheme: String,
    /// Whether that theme is dark (decides `.dark`/`color-scheme` for `system`, too).
    #[serde(default = "yes")]
    pub dark: bool,
    /// `--css-var` → value, exactly what `cssVars()` produced.
    #[serde(default)]
    pub vars: std::collections::BTreeMap<String, String>,
}

fn system() -> String {
    "system".into()
}

fn yes() -> bool {
    true
}

fn cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".atlas/desktop/theme-cache.json"))
}

pub fn load_cache() -> Option<ThemeCache> {
    let text = std::fs::read_to_string(cache_path()?).ok()?;
    let c: ThemeCache = serde_json::from_str(&text).ok()?;
    (!c.vars.is_empty()).then_some(c)
}

#[tauri::command]
pub fn theme_cache_write(cache: ThemeCache) -> Result<(), String> {
    let p = cache_path().ok_or("cannot locate home directory")?;
    if load_cache().as_ref() == Some(&cache) {
        return Ok(());
    }
    let text = serde_json::to_string(&cache).map_err(|e| e.to_string())?;
    crate::registry::write_atomic(&p, &text)
}

/// Script run in the document before any page script: the cached theme on `<html>`, and the
/// flag that turns on frontend trace marks. A stale cache only costs a one-frame difference that
/// `themes.init()` corrects before the window is shown.
pub fn init_script(cache: Option<&ThemeCache>) -> String {
    let mut js = String::new();
    if enabled() {
        js.push_str("window.__ATLAS_BOOT__=true;");
    }
    if let Some(c) = cache {
        let vars = serde_json::to_string(&c.vars).unwrap_or_else(|_| "{}".into());
        let name = serde_json::to_string(&c.name).unwrap_or_else(|_| "\"\"".into());
        let scheme = serde_json::to_string(&c.scheme).unwrap_or_else(|_| "\"system\"".into());
        // WebView2 (Windows) runs this before the document has its <html> element (WKWebView already has
        // it): apply as soon as it exists rather than throwing on a null `documentElement`.
        js.push_str(&format!(
            "(function(){{function a(){{var h=document.documentElement;if(!h)return false;\
             var v={vars},s=h.style,k;\
             for(k in v)s.setProperty(k,v[k]);\
             h.dataset.atlasTheme={name};\
             h.classList.toggle('dark',{dark});\
             s.colorScheme={scheme_css};\
             try{{localStorage.setItem('ca-scheme',JSON.stringify({scheme}))}}catch(e){{}}\
             return true}}\
             if(!a()){{var o=new MutationObserver(function(){{if(a())o.disconnect()}});\
             o.observe(document,{{childList:true}})}}}})();",
            dark = c.dark,
            scheme_css = if c.dark { "'dark'" } else { "'light'" },
        ));
    }
    js
}

/// Window appearance (and so the native material) for the cached theme: the applied theme's own, whatever the
/// scheme preference says. `None` (follow macOS) only without a cache.
pub fn window_theme(cache: Option<&ThemeCache>) -> Option<tauri::Theme> {
    cache.map(|c| {
        if c.dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }
    })
}

/// Everything a window needs to be born in the app's theme: the native appearance and the init script. The main window
/// and the menu-bar popover share it, so neither starts on the kit's defaults.
pub fn window_boot(cache: Option<&ThemeCache>) -> (Option<tauri::Theme>, String) {
    (window_theme(cache), init_script(cache))
}

/// A user theme file as read from disk.
#[derive(Serialize)]
pub struct ThemeFile {
    pub file: String,
    /// File text, or the reason it could not be read.
    #[serde(flatten)]
    pub content: ThemeFileContent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeFileContent {
    Text(String),
    Error(String),
}

/// Theme files are small; a larger one is a mistake, not a theme.
const MAX_THEME_BYTES: u64 = 2_000_000;

/// `*.json` of the fixed user theme dirs (`~/.atlas/desktop/themes`, then `~/.atlas/themes`).
/// Plain local reads: theme loading must not wait for the terminal host to connect. The webview
/// supplies no path, so this cannot be pointed at other files.
#[tauri::command]
pub fn themes_user_read() -> Vec<ThemeFile> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in [".atlas/desktop/themes", ".atlas/themes"] {
        let Ok(entries) = std::fs::read_dir(home.join(dir)) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json") && p.is_file())
            .collect();
        paths.sort();
        for p in paths {
            let content = match std::fs::metadata(&p) {
                Ok(m) if m.len() > MAX_THEME_BYTES => {
                    ThemeFileContent::Error(format!("larger than {MAX_THEME_BYTES} bytes"))
                }
                _ => match std::fs::read_to_string(&p) {
                    Ok(t) => ThemeFileContent::Text(t),
                    Err(e) => ThemeFileContent::Error(e.to_string()),
                },
            };
            out.push(ThemeFile {
                file: p.to_string_lossy().into_owned(),
                content,
            });
        }
    }
    out
}

// ---------- show gate ----------

fn show_now(app: &AppHandle, why: &str) {
    if SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    mark(&format!("show:{why}"));
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        crate::show_main(&app);
    });
}

/// The frontend painted its first real frame.
#[tauri::command]
pub fn app_ready(app: AppHandle) {
    show_now(&app, "ready");
}

/// Arm the hard deadline for the hidden main window.
pub fn arm_deadline(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_DEADLINE).await;
        show_now(&app, "deadline");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(scheme: &str, dark: bool) -> ThemeCache {
        ThemeCache {
            name: "Atlas Dark".into(),
            scheme: scheme.into(),
            dark,
            vars: [("--ca-surface-base".to_string(), "#131c2b".to_string())].into(),
        }
    }

    #[test]
    fn init_script_replays_the_cached_theme_before_paint() {
        let js = init_script(Some(&cache("dark", true)));
        assert!(js.contains(r##"{"--ca-surface-base":"#131c2b"}"##));
        assert!(js.contains("classList.toggle('dark',true)"));
        assert!(js.contains("colorScheme='dark'"));
        assert!(js.contains(r#"dataset.atlasTheme="Atlas Dark""#));
        // The kit reads its scheme from localStorage: it must agree with the cache.
        assert!(js.contains(r#"JSON.stringify("dark")"#));
    }

    // Incident (v1.5.0-alpha.7): the menu-bar popover was pure black. Only the main window was built with the cached
    // theme on <html>; the popover started on the kit's defaults (no Atlas surface ladder, no cached scheme), so its
    // cards sat on black instead of the theme's ground. Both windows now share one boot recipe.
    #[test]
    fn the_popover_boots_with_the_same_cached_theme_as_the_main_window() {
        let c = cache("dark", true);
        let (theme, js) = window_boot(Some(&c));
        assert_eq!(theme, Some(tauri::Theme::Dark));
        assert_eq!(js, init_script(Some(&c)));
        assert!(js.contains("--ca-surface-base"));
    }

    #[test]
    fn a_light_cache_boots_a_light_window_even_when_the_scheme_is_system() {
        let (theme, js) = window_boot(Some(&cache("system", false)));
        assert_eq!(theme, Some(tauri::Theme::Light));
        assert!(js.contains("classList.toggle('dark',false)"));
    }

    #[test]
    fn without_a_cache_the_popover_follows_macos_and_replays_nothing() {
        let (theme, js) = window_boot(None);
        assert_eq!(theme, None);
        assert!(!js.contains("setProperty"));
    }

    // Incident (Windows, first run): WebView2 runs initialization scripts before <html> exists, so
    // `document.documentElement.style` threw on every load and the cached theme was never applied. The
    // script waits for the element instead of assuming it.
    #[test]
    fn init_script_waits_for_the_html_element_instead_of_assuming_it() {
        let js = init_script(Some(&cache("dark", true)));
        assert!(js.contains("if(!h)return false"), "{js}");
        assert!(js.contains("new MutationObserver"), "{js}");
        assert!(js.contains("o.observe(document,{childList:true})"), "{js}");
        // Every use of the element is behind the guard.
        let guard = js.find("if(!h)return false").unwrap();
        assert!(js.find(".style").unwrap() > guard);
    }

    #[test]
    fn init_script_escapes_values_instead_of_splicing_them() {
        let mut c = cache("system", false);
        c.name = r#"x"});alert(1);({"#.into();
        let js = init_script(Some(&c));
        assert!(js.contains(r#"dataset.atlasTheme="x\"});alert(1);({""#));
    }

    #[test]
    fn no_cache_means_no_script_and_the_system_appearance() {
        assert!(!init_script(None).contains("documentElement"));
        assert_eq!(window_theme(None), None);
    }

    #[test]
    fn window_theme_follows_the_scheme_preference() {
        assert_eq!(
            window_theme(Some(&cache("dark", true))),
            Some(tauri::Theme::Dark)
        );
        assert_eq!(
            window_theme(Some(&cache("light", false))),
            Some(tauri::Theme::Light)
        );
    }

    // Incident (user, Atlas Dark, 2026-10-13): `scheme: system` (the default) left the window on "follow macOS", so the
    // native material could be light under a dark theme. The window follows the theme that was APPLIED (`dark`).
    #[test]
    fn window_theme_follows_the_applied_theme_even_in_system_mode() {
        assert_eq!(
            window_theme(Some(&cache("system", true))),
            Some(tauri::Theme::Dark)
        );
        assert_eq!(
            window_theme(Some(&cache("system", false))),
            Some(tauri::Theme::Light)
        );
        // a pinned scheme and a theme of the other appearance (a light theme chosen under `dark`) cannot happen
        // through the UI, but the cache is a file: the applied theme still wins, it is what the page paints
        assert_eq!(
            window_theme(Some(&cache("dark", false))),
            Some(tauri::Theme::Light)
        );
    }

    #[test]
    fn a_cache_file_without_variables_is_ignored() {
        let c: ThemeCache = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
        assert!(c.vars.is_empty());
        assert_eq!(c.scheme, "system");
        assert!(c.dark);
    }
}
