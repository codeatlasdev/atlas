//! Live settings and diagnostics: theme, `terminal.*` prefs, font size, focus, and the
//! frame statistics / screen text the debug bridge reads.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use super::lifecycle::{ExtraColors, build_palette};
use super::{TermPrefs, open_terms, targets, with_term};

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
    // Handles first, `TERMS` released: `vt.size()` waits for the VT lock a feeding reader holds,
    // and with 7+ busy agents that wait used to be spent holding the registry every command needs.
    let rows = open_terms().into_iter().map(|(tab, sh)| {
        let (stats, cell) = {
            let st = sh.state.lock();
            (st.stats, st.cell)
        };
        let mut row = stats_row(stats, cell, sh.vt.size(), scale);
        row["vt"] = wheel_row(sh.vt.wheel_state());
        (tab, row)
    });
    json!(rows.collect::<serde_json::Map<_, _>>())
}

/// The `vt` block of a `term_stats` entry: where the viewport is and which modes the wheel rules read, so a
/// report of "the wheel does nothing" can be diagnosed from the running app (`atlas desktop debug eval`).
fn wheel_row(s: atlas_term::WheelState) -> Value {
    json!({
        "display_offset": s.display_offset, "max_offset": s.max_offset,
        "mouse": s.mouse, "alt_screen": s.alt_screen, "alt_scroll": s.alt_scroll,
    })
}

/// One terminal's entry of `term_stats`: what the frontend (`cell_w`/`cell_h`, `cols`, `rows`) and
/// `scripts/agents-load-bench.ts` (frames, work and interval percentiles) read.
fn stats_row(s: atlas_term::Stats, cell: [f32; 2], (cols, rows): (u16, u16), scale: f64) -> Value {
    json!({
        "frames": s.frames, "cols": cols, "rows": rows,
        "cell_w": cell[0] as f64 / scale, "cell_h": cell[1] as f64 / scale,
        "work_p50_us": s.work_p50_us, "work_p95_us": s.work_p95_us,
        "work_max_us": s.work_max_us,
        "interval_p50_us": s.interval_p50_us, "interval_p95_us": s.interval_p95_us,
    })
}

/// Start a fresh measurement window on every open terminal.
#[tauri::command]
pub fn term_stats_reset() {
    for (_, sh) in open_terms() {
        sh.update(|s| s.reset = true);
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
    for sh in targets(tab.as_deref())? {
        sh.vt.set_palette(&pal);
        let pal = pal.clone();
        sh.update(|s| {
            s.pal = pal;
            s.pal_new = true;
            s.dirty = true;
        });
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
        sh.vt.set_scrollback_bytes(prefs.scrollback_bytes);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Contract with `geometry.ts::cellFromStats` (`cell_w`/`cell_h` in window points, i.e. the
    /// renderer's physical pixels over the backing scale) and with `scripts/agents-load-bench.ts`
    /// (frame and work/interval percentiles, `cols`/`rows`). Splitting `term_stats` to read the
    /// handles outside `TERMS` (many busy agents) must not move a key.
    #[test]
    fn a_stats_row_carries_the_keys_the_frontend_and_the_bench_read() {
        let s = atlas_term::Stats {
            frames: 7,
            work_p50_us: 220,
            work_p95_us: 290,
            work_max_us: 1600,
            interval_p50_us: 8330,
            interval_p95_us: 8450,
        };
        let v = stats_row(s, [16.0, 34.0], (144, 44), 2.0);
        assert_eq!(v["cell_w"], 8.0);
        assert_eq!(v["cell_h"], 17.0);
        assert_eq!(
            (v["cols"].as_u64(), v["rows"].as_u64()),
            (Some(144), Some(44))
        );
        for (k, want) in [
            ("frames", 7),
            ("work_p50_us", 220),
            ("work_p95_us", 290),
            ("work_max_us", 1600),
            ("interval_p50_us", 8330),
            ("interval_p95_us", 8450),
        ] {
            assert_eq!(v[k].as_u64(), Some(want), "{k}");
        }
    }

    /// Before the first frame the renderer has not measured a cell: the frontend treats 0 as "unknown".
    #[test]
    fn a_terminal_that_never_drew_reports_a_zero_cell_not_nan() {
        let v = stats_row(atlas_term::Stats::default(), [0.0, 0.0], (80, 24), 2.0);
        assert_eq!(
            (v["cell_w"].as_f64(), v["cell_h"].as_f64()),
            (Some(0.0), Some(0.0))
        );
        assert_eq!(v["frames"].as_u64(), Some(0));
    }
}
