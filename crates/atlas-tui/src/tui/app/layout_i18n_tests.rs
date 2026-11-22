//! The TUI in the display language: every panel at 80x24 and 120x40 through the real `App` and a test backend.
//!
//! The language is the test process's: English, or what `ATLAS_TEST_LANG` asks for, so the same test runs in English in CI and by hand in
//! `ATLAS_TEST_LANG=pt-BR` and `ATLAS_TEST_LANG=en-XA` (the pseudo-locale of debug builds). With `ATLAS_TUI_DUMP=<dir>` it writes
//! each frame as text to `<dir>/<lang>-<panel>-<w>x<h>.txt` for the layout pass.
//!
//! What it asserts holds in every language, because it compares with the catalog: a help row or a modal line that the
//! translation made longer than the panel is clipped, and that is a bug of the batch, not of the layout.

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use super::*;
use crate::config::{ActionConfig, ProjectConfig, ServiceDef};
use crate::event::EventHandler;
use crate::runtime::manager::ServiceManager;
use crate::tui::views::help;

const SIZES: [(u16, u16); 2] = [(80, 24), (120, 40)];
const LONG_NAME: &str = "payments-gateway-worker";

fn app() -> App {
    let mut services = HashMap::new();
    for (name, port) in [("api", 4000), (LONG_NAME, 4001), ("web", 4002)] {
        services.insert(
            name.to_string(),
            ServiceDef {
                command: Some("sleep 30".into()),
                port: Some(port),
                ..Default::default()
            },
        );
    }
    let action: ActionConfig = serde_json::from_value(serde_json::json!({
        "key": "x", "label": "Reset the database", "command": "make reset"
    }))
    .unwrap();
    let cfg = ProjectConfig {
        name: "demo".into(),
        services,
        actions: vec![action],
        ..Default::default()
    };
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp"), &cfg, tx);
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);
    app.splash_done = true;
    app
}

fn draw(app: &App, w: u16, h: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Write the frame for the layout pass when `ATLAS_TUI_DUMP` names a folder.
fn dump(panel: &str, w: u16, h: u16, rows: &[String]) {
    let Ok(dir) = std::env::var("ATLAS_TUI_DUMP") else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let file = format!("{dir}/{}-{panel}-{w}x{h}.txt", atlas_i18n::lang());
    std::fs::write(file, rows.join("\n") + "\n").unwrap();
}

/// A message's words may wrap in the panel, so compare with the panel's text without the line breaks and the
/// padding between columns, and without the panel's border.
fn squash(s: &str) -> String {
    s.replace(['│', '┃'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The help (`?`) shows every row of the keymap whole: the panel scrolls vertically, never cuts a label sideways.
#[test]
fn help_rows_are_never_cut_sideways() {
    for (w, h) in SIZES {
        let mut app = app();
        app.update(Message::ShowHelp);
        let first = draw(&app, w, h);
        dump("help", w, h, &first);
        let max = app.help_max_scroll.get();
        let mut seen = first.join("\n");
        for scroll in 1..=max {
            app.help_scroll = scroll;
            seen.push('\n');
            seen.push_str(&draw(&app, w, h).join("\n"));
        }
        let seen = squash(&seen);
        let rows = help::rows(
            &app.config.actions,
            &app.manager.open_services(),
            app.key_ctx(),
        );
        for row in rows.iter().flatten() {
            if let help::Row::Key { label, .. } = row {
                assert!(
                    seen.contains(&squash(label)),
                    "{}: help row cut at {w}x{h}: {label:?}",
                    atlas_i18n::lang()
                );
            }
        }
    }
}

/// The quit modal fits its three lines and both buttons, in both quit modes.
#[test]
fn the_quit_modal_shows_every_line() {
    for (w, h) in SIZES {
        let mut app = app();
        app.update(Message::ShowQuit);
        let rows = draw(&app, w, h);
        dump("quit", w, h, &rows);
        let seen = squash(&rows.join("\n"));
        for want in [
            atlas_i18n::t!("tui_runtime_views_quit_owner_headline"),
            atlas_i18n::t!(
                "tui_runtime_views_quit_owner_hint",
                command = "atlas dev --detach"
            ),
            atlas_i18n::t!("common_action_cancel"),
            atlas_i18n::t!("tui_runtime_views_quit_quit"),
        ] {
            assert!(
                seen.contains(&squash(&want)),
                "{}: quit modal cut at {w}x{h}: {want:?}\n{}",
                atlas_i18n::lang(),
                rows.join("\n")
            );
        }
    }
}

/// The dashboard, the command palette, the service menu and the restart-all confirmation draw at both sizes; the
/// footer keeps its help and quit hints (the last to be dropped) and the longest service name stays inside the sidebar.
#[test]
fn every_panel_draws_at_both_sizes() {
    for (w, h) in SIZES {
        let mut app = app();
        let rows = draw(&app, w, h);
        dump("dashboard", w, h, &rows);
        let seen = rows.join("\n");
        for key in [
            "tui_runtime_keys_footer_help",
            "tui_runtime_keys_footer_quit",
        ] {
            let label = match key {
                "tui_runtime_keys_footer_help" => atlas_i18n::t!("tui_runtime_keys_footer_help"),
                _ => atlas_i18n::t!("tui_runtime_keys_footer_quit"),
            };
            assert!(
                seen.contains(&label),
                "{}: footer lost {label:?} at {w}x{h}\n{seen}",
                atlas_i18n::lang()
            );
        }

        app.update(Message::ShowPalette);
        dump("palette", w, h, &draw(&app, w, h));
        app.update(Message::HidePalette);

        app.update(Message::ShowServiceMenu(LONG_NAME.to_string()));
        let rows = draw(&app, w, h);
        dump("menu", w, h, &rows);
        let seen = rows.join("\n");
        for want in [
            atlas_i18n::t!("tui_runtime_views_menu_start"),
            atlas_i18n::t!(
                "tui_runtime_views_menu_cancel",
                key = atlas_i18n::t!("common_keys_escape").to_lowercase()
            ),
        ] {
            assert!(
                seen.contains(&want),
                "{}: menu cut at {w}x{h}: {want:?}",
                atlas_i18n::lang()
            );
        }
        app.update(Message::HideServiceMenu);

        app.update(Message::RestartAll);
        let rows = draw(&app, w, h);
        dump("confirm", w, h, &rows);
        let body = atlas_i18n::t!("tui_runtime_app_restart_all_body", count = 3);
        assert!(
            squash(&rows.join("\n")).contains(&squash(&body)),
            "{}: confirm body cut at {w}x{h}: {body:?}\n{}",
            atlas_i18n::lang(),
            rows.join("\n")
        );
    }
}

/// Below the minimum the TUI says so, with both sizes, in the language.
#[test]
fn a_too_small_terminal_says_so() {
    let app = app();
    let rows = draw(&app, 50, 10);
    dump("small", 50, 10, &rows);
    let seen = rows.join("\n");
    assert!(
        seen.contains(&atlas_i18n::t!("tui_runtime_views_dash_too_small")),
        "{seen}"
    );
    assert!(
        seen.contains(&atlas_i18n::t!(
            "tui_runtime_views_dash_size_current",
            width = 50,
            height = 10
        )),
        "{seen}"
    );
}

/// The search bar and the custom time-range prompt are the longest line of the dashboard; at 80 columns they drop
/// trailing parts instead of overflowing, and the prompt's own hint survives at 120.
#[test]
fn the_search_bar_fits_the_width() {
    for (w, h) in SIZES {
        let mut app = app();
        app.update(Message::StartLogSearch);
        for c in "timeout".chars() {
            app.update(Message::LogSearchInput(c));
        }
        let rows = draw(&app, w, h);
        dump("search", w, h, &rows);
        assert!(rows.iter().all(|r| r.chars().count() <= w as usize));
    }
}

/// The sidebar only exists from 134 columns; at 160x40 its headings, its health line and the event log are there in
/// the language, and the longest service name is cut to its column rather than pushed into the next one.
#[test]
fn the_sidebar_shows_its_headings() {
    let app = app();
    let rows = draw(&app, 160, 40);
    dump("sidebar", 160, 40, &rows);
    let seen = rows.join("\n");
    for want in [
        atlas_i18n::t!("tui_runtime_widgets_sidebar_services").to_uppercase(),
        atlas_i18n::t!("tui_runtime_widgets_events_title").to_uppercase(),
        atlas_i18n::t!("tui_runtime_widgets_events_empty"),
    ] {
        assert!(
            seen.contains(&want),
            "{}: sidebar lost {want:?}\n{seen}",
            atlas_i18n::lang()
        );
    }
    assert!(rows.iter().all(|r| r.chars().count() <= 160));
}
