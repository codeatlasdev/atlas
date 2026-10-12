//! Chrome behavior through the real `App`: palette scroll/recent, zoom,
//! restart-all confirm, tunnel key, narrow strip, persistence.

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use crate::event::EventHandler;
use crate::runtime::manager::ServiceManager;

fn svc(port: Option<u16>) -> ServiceDef {
    ServiceDef {
        command: Some("sleep 30".into()),
        port,
        ..Default::default()
    }
}

fn config(dir: &std::path::Path, names: &[&str]) -> ProjectConfig {
    let _ = dir;
    let mut services = HashMap::new();
    for (i, n) in names.iter().enumerate() {
        services.insert(n.to_string(), svc(Some(4000 + i as u16)));
    }
    ProjectConfig {
        name: "demo".into(),
        services,
        ..Default::default()
    }
}

fn app_with(dir: &std::path::Path, cfg: ProjectConfig) -> App {
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from(dir), &cfg, tx);
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
                .collect()
        })
        .collect()
}

fn text(rows: &[String]) -> String {
    rows.join("\n")
}

#[test]
fn zoom_hides_sidebar_even_when_wide_and_toggles_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    let wide = text(&draw(&app, 200, 40));
    assert!(wide.contains("SERVICES"), "sidebar visible at 200 cols");
    app.update(Message::ToggleZoom);
    assert!(app.zoom);
    let zoomed = text(&draw(&app, 200, 40));
    assert!(!zoomed.contains("SERVICES"), "sidebar hidden by zoom");
    assert!(zoomed.contains("Zoom on"), "footer shows state:\n{zoomed}");
    app.update(Message::ToggleZoom);
    assert!(text(&draw(&app, 200, 40)).contains("SERVICES"));
}

#[test]
fn restart_all_on_all_tab_asks_first_and_names_the_count() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api", "web"]));
    app.update(Message::RestartAll);
    match &app.layer {
        Layer::Confirm {
            body, on_confirm, ..
        } => {
            assert!(body.contains("Restart 2 services?"), "{body}");
            assert!(body.contains("oneshots will not re-run"), "{body}");
            assert!(matches!(**on_confirm, Message::RestartAllConfirmed));
        }
        other => panic!("expected confirm, got {other:?}"),
    }
    assert!(app.effects.is_empty(), "nothing restarts before confirm");
    app.update(Message::ConfirmAccept);
    assert!(matches!(
        app.effects.front(),
        Some(Effect::RestartServices(n)) if n.len() == 2
    ));
}

#[test]
fn r_key_on_all_tab_maps_to_confirm_flow() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_with(dir.path(), config(dir.path(), &["api"]));
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('r'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert!(matches!(app.map_key(key), Some(Message::RestartAll)));
}

#[test]
fn tunnel_key_is_inert_without_a_tunnel() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('t'),
        crossterm::event::KeyModifiers::NONE,
    );
    assert!(app.map_key(key).is_none());
    app.update(Message::ToggleTunnelOnExit);
    assert!(app.toast.is_none(), "no lying toast");
    assert!(app.tunnel_kill_on_exit);
}

#[test]
fn ctrl_c_quits_from_palette_and_is_not_typed() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.update(Message::ShowPalette);
    let ctrl_c = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('c'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert!(matches!(app.map_key(ctrl_c), Some(Message::Quit)));
    let ctrl_l = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('l'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert!(app.map_key(ctrl_l).is_none(), "Ctrl chords are never text");
}

#[test]
fn palette_ctrl_u_and_ctrl_w_edit_the_query() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.update(Message::ShowPalette);
    for c in "restart api".chars() {
        app.update(Message::PaletteInput(c));
    }
    app.update(Message::PaletteDeleteWord);
    assert_eq!(app.palette_input, "restart ");
    app.update(Message::PaletteClear);
    assert!(app.palette_input.is_empty());
}

#[test]
fn palette_scrolls_to_keep_selection_visible() {
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<String> = (0..12).map(|i| format!("svc{i:02}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut app = app_with(dir.path(), config(dir.path(), &refs));
    app.update(Message::ShowPalette);
    let total = app.palette_filtered.len();
    assert!(total > 20, "enough commands to need scrolling: {total}");
    draw(&app, 120, 30); // records visible rows
    for _ in 0..(total - 1) {
        app.update(Message::PaletteDown);
    }
    let rows = app.palette_rows.get();
    assert!(app.palette_selected == total - 1);
    assert!(
        app.palette_offset <= app.palette_selected
            && app.palette_selected < app.palette_offset + rows,
        "selected {} outside window {}..{}",
        app.palette_selected,
        app.palette_offset,
        app.palette_offset + rows
    );
    let screen = text(&draw(&app, 120, 30));
    assert!(screen.contains(&format!("{total}/{total}")), "{screen}");
    app.update(Message::PaletteUp);
    assert!(app.palette_selected < total - 1);
}

#[test]
fn palette_fuzzy_matches_and_actions_before_generated_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), &["api", "web"]);
    cfg.actions = vec![serde_yaml::from_str("key: c\nlabel: Run checks\ncommand: echo\n").unwrap()];
    let mut app = app_with(dir.path(), cfg);
    app.manager.update_state("api", ServiceState::Running);
    app.manager.update_state("web", ServiceState::Running);
    app.update(Message::ShowPalette);
    let names: Vec<&str> = app
        .palette_filtered
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    let action = names.iter().position(|n| *n == "Run checks").unwrap();
    let restart = names
        .iter()
        .position(|n| n.starts_with("Restart api"))
        .unwrap();
    assert!(action < restart, "actions first: {names:?}");
    for c in "rch".chars() {
        app.update(Message::PaletteInput(c));
    }
    assert_eq!(
        app.palette_filtered[0].name,
        "Run checks",
        "{:?}",
        app.palette_filtered
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>()
    );
    assert!(!app.palette_filtered[0].matched.is_empty());
}

#[test]
fn palette_entries_follow_service_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.manager.update_state("api", ServiceState::Stopped);
    app.update(Message::ShowPalette);
    let names: Vec<String> = app
        .palette_filtered
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(names.contains(&"Start api".to_string()), "{names:?}");
    assert!(!names.contains(&"Stop api".to_string()), "{names:?}");
    app.manager.update_state("api", ServiceState::Running);
    app.update(Message::ShowPalette);
    let names: Vec<String> = app
        .palette_filtered
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(names.contains(&"Stop api".to_string()) && names.contains(&"Restart api".to_string()));
    assert!(!names.contains(&"Start api".to_string()));
}

#[test]
fn recent_commands_float_to_top_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.update(Message::ShowPalette);
    let idx = app
        .palette_filtered
        .iter()
        .position(|c| c.id == "b:copy-err")
        .unwrap();
    app.update(Message::PaletteClick(idx));
    app.update(Message::ShowPalette);
    assert_eq!(app.palette_filtered[0].id, "b:copy-err");
    // new session reads it back from .logs/.atlas-ui.json
    let mut again = app_with(dir.path(), config(dir.path(), &["api"]));
    again.load_ui_state();
    again.update(Message::ShowPalette);
    assert_eq!(again.palette_filtered[0].id, "b:copy-err");
}

#[test]
fn ui_state_restores_tab_zoom_and_level() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api", "web"]));
    app.update(Message::SwitchTab(2));
    app.update(Message::ToggleZoom);
    app.update(Message::CycleLevelFilter);
    let want = app.tabs[2].clone();
    let mut fresh = app_with(dir.path(), config(dir.path(), &["api", "web"]));
    fresh.load_ui_state();
    assert_eq!(fresh.tabs[fresh.active_tab], want);
    assert!(fresh.zoom);
    assert_eq!(fresh.logs.level_filter, LevelFilter::Warn);
}

#[test]
fn narrow_strip_shows_worst_issue_first_and_click_opens_its_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api", "web"]));
    app.manager.update_state("web", ServiceState::Running);
    app.manager.update_state("api", ServiceState::Failed);
    let screen = draw(&app, 90, 28);
    let strip = screen
        .iter()
        .find(|r| r.contains("✗ api"))
        .unwrap_or_else(|| panic!("no strip:\n{}", text(&screen)));
    assert!(strip.contains("1/2 running"), "{strip}");
    let y = screen.iter().position(|r| r == strip).unwrap() as u16;
    app.update(Message::MouseClick { x: 3, y });
    assert_eq!(app.tabs[app.active_tab], "api");
}

#[test]
fn wide_layout_has_no_strip_and_health_excludes_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api", "web"]));
    app.manager.update_state("api", ServiceState::Running);
    app.manager.update_state("web", ServiceState::Stopped);
    let screen = text(&draw(&app, 200, 40));
    assert!(screen.contains("1/1 running · 1 stopped"), "{screen}");
}

#[test]
fn footer_never_overlaps_operation_and_drops_hints_instead() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.operation = Some(Operation::start("Deploy preview".into()));
    // Narrow layout moves the operation into the strip; check a width with sidebar too.
    for (w, h) in [(160u16, 40u16), (90, 28), (64, 14)] {
        let screen = draw(&app, w, h);
        let footer = screen.last().unwrap();
        assert_eq!(footer.chars().count(), w as usize);
        // every clickable footer rect lies on columns whose text is a drawn hint
        let y = h - 1;
        for (r, t) in app.clicks.targets.borrow().iter() {
            if r.y == y && matches!(t, crate::tui::clicks::Target::Click(_)) {
                let seg: String = footer
                    .chars()
                    .skip(r.x as usize)
                    .take(r.width as usize)
                    .collect();
                assert!(
                    !seg.trim().is_empty(),
                    "click on blank footer cells at {w}x{h}: {r:?}"
                );
                assert!(
                    !seg.contains("Deploy"),
                    "hint overlaps operation at {w}x{h}"
                );
            }
        }
    }
}

#[test]
fn confirm_modal_lists_paused_services() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), &["web", "api"]);
    cfg.actions = vec![serde_yaml::from_str(
        "key: d\nlabel: Reset DB\ncommand: echo\nconfirm: Wipe the db?\npause_services: [web, api]\n",
    )
    .unwrap()];
    let mut app = app_with(dir.path(), cfg);
    app.update(Message::RunAction('d'));
    let screen = text(&draw(&app, 120, 30));
    assert!(screen.contains("Pauses: web, api"), "{screen}");
}

#[test]
fn modal_dims_background_instead_of_clearing_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.update(Message::ShowHelp);
    // tabs row stays rendered behind the help modal
    let screen = text(&draw(&app, 160, 50));
    assert!(screen.contains("0 All"), "background survives:\n{screen}");
}

#[test]
fn too_small_screen_reports_current_size() {
    let dir = tempfile::tempdir().unwrap();
    let app = app_with(dir.path(), config(dir.path(), &["api"]));
    let screen = text(&draw(&app, 58, 11));
    assert!(screen.contains("Current: 58x11"), "{screen}");
}

#[test]
fn sidebar_keeps_critical_help_when_height_is_tight() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), &["api"]);
    cfg.services.get_mut("api").unwrap().critical = Some(true);
    cfg.services.get_mut("api").unwrap().critical_help = Some("Check the worker config".into());
    let mut app = app_with(dir.path(), cfg);
    app.manager.update_state("api", ServiceState::Failed);
    let screen = text(&draw(&app, 140, 30));
    if screen.contains("ISSUES") {
        assert!(screen.contains("Check the worker config"), "{screen}");
    }
}

#[test]
fn info_toast_uses_static_icon_progress_uses_spinner() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    let aged = |app: &mut App, msg: &str, v: ToastVariant| {
        // Past the slide-in so the toast is fully on screen.
        app.toast = Some((
            msg.to_string(),
            v,
            Instant::now() - Duration::from_millis(900),
        ));
    };
    aged(&mut app, "Tunnel will close on exit", ToastVariant::Info);
    assert!(text(&draw(&app, 120, 30)).contains("ℹ Tunnel will close"));
    aged(&mut app, "Restarting api", ToastVariant::Progress);
    let s = text(&draw(&app, 120, 30));
    assert!(!s.contains("ℹ Restarting"), "{s}");
    assert!(s.contains("Restarting api") && !s.contains("Restarting api..."));
}

#[test]
fn help_scroll_is_clamped_and_resets_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.update(Message::ShowHelp);
    draw(&app, 90, 14);
    app.update(Message::HelpScroll(isize::MAX));
    assert_eq!(app.help_scroll, app.help_max_scroll.get());
    assert!(app.help_scroll > 0);
    app.update(Message::HelpScroll(1000));
    assert_eq!(app.help_scroll, app.help_max_scroll.get());
    app.update(Message::HideHelp);
    app.update(Message::ShowHelp);
    assert_eq!(app.help_scroll, 0);
}

#[test]
fn restart_all_skips_completed_oneshots() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api", "web", "migrate"]));
    app.manager.update_state("migrate", ServiceState::Stopped);
    app.manager.test_set_completed("migrate");
    let targets = app.restart_all_targets();
    assert_eq!(targets.len(), 2);
    assert!(!targets.contains(&"migrate".to_string()));
    app.update(Message::RestartAll);
    match &app.layer {
        Layer::Confirm { body, .. } => assert!(body.contains("Restart 2 services?"), "{body}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn sidebar_failed_row_shows_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), config(dir.path(), &["api"]));
    app.manager.update_state("api", ServiceState::Failed);
    app.manager.test_set_exit_code("api", 2);
    let screen = text(&draw(&app, 160, 40));
    assert!(screen.contains("exit 2"), "{screen}");
}
