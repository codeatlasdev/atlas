//! Neither macOS nor Linux (Windows): no native GPU terminal. The frontend sees `term_capability` say "not available"
//! and renders every terminal with xterm.js, so these commands are only reached by a stale caller; they refuse instead
//! of pretending.

macro_rules! unsupported {
    ($($name:ident),* $(,)?) => {$(
        #[tauri::command]
        pub fn $name() -> Result<(), String> {
            Err(UNSUPPORTED.into())
        }
    )*};
}

/// No native terminal views exist here (listed for the registration test:
///     term_list,
///     term_capability,
/// each answers instead of refusing: an empty list, and "not available").
#[tauri::command]
pub fn term_list() -> Vec<String> {
    Vec::new()
}

#[tauri::command]
pub fn term_capability() -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "reason": "no native terminal on this platform",
        "adapter": null,
        "setting": false,
        "active": false,
    })
}

const UNSUPPORTED: &str =
    "the GPU terminal is not available on this platform; this build uses xterm.js";

unsupported!(
    term_open,
    term_bounds,
    term_slide,
    term_close,
    term_reload,
    term_stats,
    term_stats_reset,
    term_key,
    term_paste,
    term_text,
    term_wheel,
    term_copy,
    term_select_start,
    term_select_update,
    term_select_end,
    term_clear_selection,
    term_link_at,
    term_select_extend,
    term_select_scroll,
    term_pointer_state,
    term_hover,
    term_search,
    term_search_clear,
    term_recolor,
    term_set_prefs,
    term_focus,
    term_preedit,
    term_paste_primary,
    term_jump_prompt,
    term_scroll_to,
    term_select_all,
    term_clear,
    term_click_move,
    term_font,
    term_paste_clipboard,
    term_select_output,
    term_mouse,
    term_mouse_captured,
);

/// No GPU terminals to reattach after a hostd restart.
pub async fn reattach_all() {}

/// No native terminals to rebuild on wake.
pub fn install_system_observers() {}
