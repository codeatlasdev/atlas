//! Live settings and diagnostics: theme, `terminal.*` prefs, font size, focus, and the
//! frame statistics / screen text the debug bridge reads.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use super::lifecycle::{ExtraColors, build_palette};
use super::{TERMS, TermPrefs, targets, with_term};

/// Visible screen text (debug bridge / tests).
#[tauri::command]
pub fn term_text(tab: String) -> Result<String, String> {
    Ok(with_term(&tab)?.vt.text())
}

/// Frame timings per open terminal.
#[tauri::command]
pub fn term_stats(app: AppHandle) -> Value {
    let scale = app
        .get_window("main")
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(2.0);
    let terms = TERMS.lock();
    json!(
        terms
            .iter()
            .map(|(tab, t)| {
                let (s, cell) = {
                    let st = t.shared.state.lock();
                    (st.stats, st.cell)
                };
                let (cols, rows) = t.shared.vt.size();
                (
                    tab.clone(),
                    json!({
                        "frames": s.frames, "cols": cols, "rows": rows,
                        "cell_w": cell[0] as f64 / scale, "cell_h": cell[1] as f64 / scale,
                        "work_p50_us": s.work_p50_us, "work_p95_us": s.work_p95_us,
                        "work_max_us": s.work_max_us,
                        "interval_p50_us": s.interval_p50_us, "interval_p95_us": s.interval_p95_us,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>()
    )
}

/// Start a fresh measurement window on every open terminal.
#[tauri::command]
pub fn term_stats_reset() {
    for t in TERMS.lock().values() {
        t.shared.update(|s| s.reset = true);
    }
}

/// Live theme update for `tab`, or every open terminal when omitted.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn term_recolor(
    tab: Option<String>,
    fg: String,
    bg: String,
    ansi16: Option<Vec<Option<String>>>,
    min_contrast: Option<f32>,
    cursor: Option<String>,
    cursor_text: Option<String>,
    selection_bg: Option<String>,
    selection_fg: Option<String>,
) -> Result<(), String> {
    let extra = ExtraColors {
        cursor,
        cursor_text,
        selection_bg,
        selection_fg,
    };
    let pal = build_palette(&fg, &bg, ansi16.as_deref(), min_contrast, &extra)?;
    let bg = {
        let c = pal.base[atlas_term::colors::NamedColor::Background as usize];
        super::layering::bg_floats(c.r, c.g, c.b)
    };
    for sh in targets(tab.as_deref())? {
        sh.vt.set_palette(&pal);
        let pal = pal.clone();
        sh.update(|s| {
            s.pal = pal;
            s.pal_new = true;
            s.dirty = true;
        });
        // The layer behind the Metal drawable follows the theme too: a gap shows the new colour.
        let view = sh.view();
        if let Some(app) = super::APP.get() {
            let _ = app.run_on_main_thread(move || super::mac::recolor(view, bg));
        }
    }
    Ok(())
}

/// Live `terminal.*` settings for `tab`, or every open terminal when omitted:
/// scrollback, cursor, OSC 52, font size, family and thickening apply at once (a font
/// change re-fits the grid and tells hostd the new size).
#[tauri::command]
pub fn term_set_prefs(tab: Option<String>, prefs: TermPrefs) -> Result<(), String> {
    let pt = prefs.font_size.clamp(6.0, 48.0);
    let spec = prefs.font_spec();
    for sh in targets(tab.as_deref())? {
        sh.vt
            .set_options(prefs.scrollback, &prefs.cursor_shape, prefs.cursor_blink);
        *sh.osc52.lock() = prefs.osc52.clone();
        sh.update(|s| {
            if s.font_cur != pt {
                s.font_cur = pt;
                s.font_pt = Some(pt);
            }
            if s.font_spec != spec {
                s.font_spec = spec.clone();
                s.font_new = Some(spec.clone());
            }
            s.ligatures = Some(prefs.ligatures);
            s.dirty = true;
        });
    }
    Ok(())
}

/// ⌘+ / ⌘− / ⌘0 (Ghostty `increase_font_size` / `decrease_font_size` / `reset_font_size`):
/// this terminal only, live. `size` is the new absolute size in pt (the frontend steps it
/// from the size it last applied; reset passes the configured one). Returns the applied size.
#[tauri::command]
pub fn term_font(tab: String, size: f32) -> Result<f32, String> {
    let pt = size.clamp(6.0, 48.0);
    with_term(&tab)?.update(|s| {
        if s.font_cur != pt {
            s.font_cur = pt;
            s.font_pt = Some(pt);
        }
        s.dirty = true;
    });
    Ok(pt)
}

/// Input sink focus: unfocused terminals draw a hollow, non-blinking cursor.
#[tauri::command]
pub fn term_focus(tab: String, focused: bool) -> Result<(), String> {
    with_term(&tab)?.update(|s| {
        s.focused = focused;
        s.dirty = true;
    });
    Ok(())
}
