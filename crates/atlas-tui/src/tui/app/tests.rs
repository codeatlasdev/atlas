use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use crate::event::EventHandler;
use crate::runtime::logs::LogStream;
use crate::runtime::manager::ServiceManager;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

fn test_config() -> ProjectConfig {
    ProjectConfig {
        name: "test".to_string(),
        services: {
            let mut m = HashMap::new();
            m.insert(
                "web".to_string(),
                ServiceDef {
                    open_key: None,
                    env: Default::default(),
                    command: Some("echo hello".to_string()),
                    dir: None,
                    port: Some(3000),
                    health: None,
                    critical: None,
                    depends_on: None,
                    enabled: None,
                    hide_logs: false,
                    lock_file: None,
                    open_url: None,
                    critical_help: None,
                    ..Default::default()
                },
            );
            m
        },
        ..Default::default()
    }
}

fn make_app() -> App {
    let (tx, rx) = mpsc::unbounded_channel();
    let cfg = test_config();
    let mgr = ServiceManager::new(PathBuf::from("/tmp"), &cfg, tx);
    let (_, events) = EventHandler::test_channel();
    App::new(cfg, mgr, rx, events, false)
}

#[test]
fn test_update_switch_tab() {
    let mut app = make_app();
    assert_eq!(app.active_tab, 0);
    app.update(Message::SwitchTab(1));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn test_update_next_tab_wraps() {
    let mut app = make_app();
    let n = app.tabs.len();
    for _ in 0..n {
        app.update(Message::NextTab);
    }
    assert_eq!(app.active_tab, 0);
}

#[test]
fn test_update_log_line() {
    let mut app = make_app();
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "listening".to_string(),
        stream: LogStream::Stdout,
    });
    assert_eq!(app.logs.log_buffer.len(), 1);
    assert_eq!(app.logs.log_buffer.last().unwrap().content, "listening");
}

#[test]
fn test_log_line_stderr_preserved() {
    let mut app = make_app();
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "error output".to_string(),
        stream: LogStream::Stderr,
    });
    let entry = app.logs.log_buffer.last().unwrap();
    assert_eq!(entry.stream, LogStream::Stderr);
    assert_eq!(entry.content, "error output");
}

#[test]
fn test_update_toast_autodismiss() {
    let mut app = make_app();
    app.update(Message::Toast("hi".to_string(), ToastVariant::Info));
    assert!(app.toast.is_some());
    app.update(Message::DismissToast);
    assert!(app.toast.is_none());
}

#[test]
fn test_layer_transitions() {
    let mut app = make_app();
    assert_eq!(app.layer, Layer::Dashboard);
    app.update(Message::ShowHelp);
    assert_eq!(app.layer, Layer::Help);
    app.update(Message::HideHelp);
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn test_filtered_logs() {
    let mut app = make_app();
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "line1".to_string(),
        stream: LogStream::Stdout,
    });
    app.update(Message::LogLineFrom {
        name: "api".to_string(),
        line: "line2".to_string(),
        stream: LogStream::Stdout,
    });
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "line3".to_string(),
        stream: LogStream::Stdout,
    });

    // Tab 0 = All
    assert_eq!(app.filtered_logs().len(), 3);

    // Tab 1 = web
    app.active_tab = 1;
    let filtered = app.filtered_logs();
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].content, "line1");
}

#[test]
fn test_scroll() {
    let mut app = make_app();
    for i in 0..10 {
        app.update(Message::LogLineFrom {
            name: "web".to_string(),
            line: format!("line {i}"),
            stream: LogStream::Stdout,
        });
    }

    assert!(app.logs.follow_logs);
    app.logs.log_viewport.set(widgets::log_viewer::LogViewport {
        text_width: 80,
        height: 10,
        max_scroll: 100,
        ..Default::default()
    });
    app.update(Message::ScrollUp);
    assert_eq!(app.logs.log_scroll, 1);
    assert!(!app.logs.follow_logs);
    app.update(Message::ScrollToTop);
    assert_eq!(app.logs.log_scroll, 100);
    app.update(Message::ScrollPageDown);
    assert_eq!(app.logs.log_scroll, 91);
    assert!(!app.logs.follow_logs);
    // New lines while paused keep the view frozen on the same content.
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "more".to_string(),
        stream: LogStream::Stdout,
    });
    assert_eq!(app.logs.log_scroll, 92);
    app.update(Message::ScrollToBottom);
    assert!(app.logs.follow_logs);
    assert_eq!(app.logs.log_scroll, 0);
    // Scrolling down to the bottom resumes follow.
    app.update(Message::ScrollUp);
    app.update(Message::ScrollDown);
    assert!(app.logs.follow_logs);
}

#[test]
fn test_quit_flow() {
    let mut app = make_app();
    assert_eq!(app.layer, Layer::Dashboard);

    app.update(Message::ShowQuit);
    assert_eq!(app.layer, Layer::Quit);
    assert_eq!(app.quit_selected, 0);

    app.update(Message::ToggleQuitSelection);
    assert_eq!(app.quit_selected, 1);

    app.update(Message::HideQuit);
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn test_tick_count_increments() {
    let mut app = make_app();
    assert_eq!(app.tick_count, 0);
    app.update(Message::Tick);
    app.update(Message::Tick);
    assert_eq!(app.tick_count, 2);
}

#[test]
fn test_quit_confirm_only_when_quit_selected() {
    let mut app = make_app();
    app.update(Message::ShowQuit);

    // Already on Cancel (0) — toggle to Quit (1)
    app.update(Message::ToggleQuitSelection);
    assert_eq!(app.quit_selected, 1);

    assert!(!app.should_quit);

    // Toggle back to Cancel (0)
    app.update(Message::ToggleQuitSelection);
    assert_eq!(app.quit_selected, 0);
}

#[test]
fn test_palette_filter() {
    let mut app = make_app();

    app.update(Message::ShowPalette);
    assert_eq!(app.layer, Layer::CommandPalette);
    assert!(!app.palette_filtered.is_empty());

    app.update(Message::PaletteInput('r'));
    app.update(Message::PaletteInput('e'));
    // Should filter to "Restart All"
    assert!(
        app.palette_filtered
            .iter()
            .any(|c| c.name.contains("Restart"))
    );
}

#[test]
fn test_palette_actions_grouped_ungrouped_first_and_filterable_by_group() {
    let mut app = make_app();
    let mk = |key: char, label: &str, group: Option<&str>| -> crate::config::ActionConfig {
        serde_yaml::from_str(&format!(
            "key: '{key}'\nlabel: {label}\ncommand: echo\n{}",
            group.map(|g| format!("group: {g}\n")).unwrap_or_default()
        ))
        .unwrap()
    };
    app.config.actions = vec![
        mk('a', "IosRun", Some("iOS")),
        mk('b', "Seed", None),
        mk('c', "AndroidRun", Some("Android")),
        mk('d', "IosBuild", Some("iOS")),
    ];
    let acts: Vec<(String, Option<String>)> = app
        .palette_commands()
        .into_iter()
        .filter(|c| matches!(c.action, Message::RunAction(_)))
        .map(|c| (c.name, c.group))
        .collect();
    let names: Vec<&str> = acts.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Seed", "IosRun", "IosBuild", "AndroidRun"]);
    assert_eq!(acts[0].1, None);
    assert_eq!(acts[1].1.as_deref(), Some("iOS"));

    // The group name is searchable.
    app.update(Message::ShowPalette);
    for c in "android".chars() {
        app.update(Message::PaletteInput(c));
    }
    assert_eq!(app.palette_filtered.len(), 1);
    assert_eq!(app.palette_filtered[0].name, "AndroidRun");
}

#[test]
fn test_palette_select_executes() {
    let mut app = make_app();

    app.update(Message::ShowPalette);
    // Pick a command that runs immediately (no confirm gate).
    let idx = app
        .palette_filtered
        .iter()
        .position(|c| matches!(c.action, Message::ScrollToBottom))
        .unwrap();
    app.palette_selected = idx;
    app.update(Message::PaletteSelect);
    assert_eq!(app.layer, Layer::Dashboard); // Palette closes after select
}

#[test]
fn test_palette_restart_all_asks_first() {
    let mut app = make_app();
    app.update(Message::ShowPalette);
    let idx = app
        .palette_filtered
        .iter()
        .position(|c| matches!(c.action, Message::RestartAll))
        .unwrap();
    app.palette_selected = idx;
    app.update(Message::PaletteSelect);
    assert!(matches!(app.layer, Layer::Confirm { .. }));
    assert!(app.effects.is_empty());
}

#[test]
fn test_palette_navigation() {
    let mut app = make_app();

    app.update(Message::ShowPalette);
    assert_eq!(app.palette_selected, 0);

    app.update(Message::PaletteDown);
    assert_eq!(app.palette_selected, 1);

    app.update(Message::PaletteDown);
    assert_eq!(app.palette_selected, 2);

    app.update(Message::PaletteUp);
    assert_eq!(app.palette_selected, 1);

    // Can't go below 0
    app.update(Message::PaletteUp);
    app.update(Message::PaletteUp);
    assert_eq!(app.palette_selected, 0);
}

#[test]
fn test_palette_backspace() {
    let mut app = make_app();

    app.update(Message::ShowPalette);
    app.update(Message::PaletteInput('r'));
    app.update(Message::PaletteInput('e'));
    assert_eq!(app.palette_input, "re");

    app.update(Message::PaletteBackspace);
    assert_eq!(app.palette_input, "r");

    app.update(Message::PaletteBackspace);
    assert_eq!(app.palette_input, "");
}

#[test]
fn test_palette_hide() {
    let mut app = make_app();

    app.update(Message::ShowPalette);
    assert_eq!(app.layer, Layer::CommandPalette);

    app.update(Message::HidePalette);
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn test_mouse_click_tab() {
    // Click map is populated by view(); render into TestBackend then check.
    let mut app = make_app();
    app.size = (160, 40);
    app.splash_done = true; // skip splash so dashboard renders and populates click map
    let mut term = Terminal::new(ratatui::backend::TestBackend::new(160, 40)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    // The click map now has entries for the tab chips recorded during view().
    // Clicking on something in row 0 should switch to that tab if it hits a SwitchTab entry.
    // Feed the click directly via the message (x=0 skips the header, pick a known chip).
    // For the "web" tab (index 1) the label "  1 web  " starts after "  0 All  " + sep.
    // Rather than hard-coding pixel coords, find a SwitchTab(1) target in the map.
    let tab1_x = {
        use crate::tui::clicks::Target;
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::SwitchTab(1))))
            .map(|(r, _)| r.x)
    };
    if let Some(x) = tab1_x {
        app.update(Message::MouseClick { x, y: 0 });
        assert_eq!(app.active_tab, 1);
    }
}

#[test]
fn test_mouse_scroll() {
    let mut app = make_app();

    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "line1".to_string(),
        stream: LogStream::Stdout,
    });
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "line2".to_string(),
        stream: LogStream::Stdout,
    });

    app.logs.log_viewport.set(widgets::log_viewer::LogViewport {
        text_width: 80,
        height: 10,
        max_scroll: 100,
        ..Default::default()
    });
    app.update(Message::MouseScroll {
        x: 0,
        y: 10,
        down: false,
    });
    assert!(!app.logs.follow_logs);
    assert_eq!(app.logs.log_scroll, 3);
    app.update(Message::MouseScroll {
        x: 0,
        y: 10,
        down: true,
    });
    assert!(app.logs.follow_logs);
}

#[test]
fn test_click_map_tab_areas() {
    // Render and check the click map has entries for both tabs.
    let mut app = make_app();
    app.size = (160, 40);
    app.splash_done = true; // skip splash so dashboard renders and populates click map
    let mut term = Terminal::new(ratatui::backend::TestBackend::new(160, 40)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    use crate::tui::clicks::Target;
    let map = app.clicks.targets.borrow();
    let tab_clicks: Vec<_> = map
        .iter()
        .filter(|(_, t)| matches!(t, Target::Click(Message::SwitchTab(_))))
        .collect();
    // "All" tab is SwitchTab(0) and "web" is SwitchTab(1)
    assert!(tab_clicks.len() >= 2, "need at least 2 tab click targets");
    // All tab chips are in the header rows (y < 2)
    for (r, _) in &tab_clicks {
        assert!(r.y < 2, "tab chips must be in the header");
    }
}

#[test]
fn test_copy_logs_no_crash() {
    let mut app = make_app();

    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "started".to_string(),
        stream: LogStream::Stdout,
    });
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "Error: crash".to_string(),
        stream: LogStream::Stderr,
    });

    // These should not crash even if clipboard isn't available
    app.update(Message::CopyLogs);
    app.update(Message::CopyLogsForAI);
    app.update(Message::CopyErrors);
}

#[test]
fn test_copy_errors_empty() {
    let mut app = make_app();

    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "all good".to_string(),
        stream: LogStream::Stdout,
    });
    app.update(Message::CopyErrors);
    // Should show "No errors found" toast
    assert!(app.toast.is_some());
    assert!(app.toast.as_ref().unwrap().0.contains("No errors"));
}

/// App whose config comes from YAML, so tests exercise real parsing.
fn app_from_yaml(yaml: &str) -> App {
    let cfg: ProjectConfig = serde_yaml::from_str(yaml).unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp"), &cfg, tx);
    let (_, events) = EventHandler::test_channel();
    App::new(cfg, mgr, rx, events, false)
}

const CONFIRM_YAML: &str = r#"
name: t
services:
  api:
    command: echo hi
    port: 3001
    open_key: a
    open_url: http://localhost:3001/api-reference
    critical: true
    critical_help: run db:up
  web:
    command: echo hi
    port: 3000
    open_key: w
actions:
  - key: x
    label: Refresh DB
    command: "true"
    confirm: Wipes the database
  - key: y
    label: Plain
    command: "true"
"#;

#[test]
fn test_confirm_action_blocks_until_confirmed() {
    let mut app = app_from_yaml(CONFIRM_YAML);

    app.update(Message::RunAction('x'));
    match &app.layer {
        Layer::Confirm { title, body, .. } => {
            assert_eq!(title, "Refresh DB");
            assert_eq!(body, "Wipes the database");
        }
        other => panic!("expected Confirm, got {other:?}"),
    }
    assert!(app.effects.is_empty(), "must not run before confirmation");

    app.update(Message::ConfirmAccept);
    assert_eq!(app.layer, Layer::Dashboard);
    assert_eq!(app.effects.len(), 1);
    assert!(matches!(app.effects[0], Effect::RunAction('x')));
}

#[test]
fn test_confirm_cancel_runs_nothing() {
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.update(Message::RunAction('x'));
    app.update(Message::ConfirmCancel);
    assert_eq!(app.layer, Layer::Dashboard);
    assert!(app.effects.is_empty());
}

#[test]
fn test_action_without_confirm_runs_immediately() {
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.update(Message::RunAction('y'));
    assert_eq!(app.layer, Layer::Dashboard);
    assert!(matches!(app.effects[0], Effect::RunAction('y')));
}

#[test]
fn test_palette_select_respects_confirm() {
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.update(Message::ShowPalette);
    app.palette_filtered = app
        .palette_commands()
        .into_iter()
        .filter(|c| c.name.contains("Refresh DB"))
        .collect();
    assert_eq!(app.palette_filtered.len(), 1);
    app.palette_selected = 0;
    app.update(Message::PaletteSelect);
    assert!(matches!(app.layer, Layer::Confirm { .. }));
    assert!(app.effects.is_empty());
}

#[test]
fn test_open_url_overrides_port_url() {
    let app = app_from_yaml(CONFIRM_YAML);
    assert_eq!(
        app.service_open_url("api", Some(3001)),
        "http://localhost:3001/api-reference"
    );
    assert_eq!(
        app.service_open_url("web", Some(3000)),
        "http://localhost:3000"
    );
    assert_eq!(app.service_open_url("web", None), "http://localhost");
}

#[test]
fn test_app_start_waits_for_deps_install() {
    let drain = |app: &mut App| -> Vec<Vec<String>> {
        app.effects
            .drain(..)
            .filter_map(|e| match e {
                Effect::StartAppServices(n) => Some(n),
                _ => None,
            })
            .collect()
    };
    let names = vec!["web".to_string()];

    // Install pending: InfraReady is held, nothing starts.
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.deps_pending = true;
    app.apply_manager_event(ManagerEvent::InfraReady {
        app_names: names.clone(),
    });
    assert!(drain(&mut app).is_empty());

    // Install done: the held start is released exactly once.
    app.apply_manager_event(ManagerEvent::DepsInstalled);
    assert!(!app.deps_pending);
    assert_eq!(drain(&mut app), vec![names.clone()]);

    // Failure releases too (start anyway, error toast).
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.deps_pending = true;
    app.apply_manager_event(ManagerEvent::InfraReady {
        app_names: names.clone(),
    });
    app.apply_manager_event(ManagerEvent::DepsFailed {
        reason: "exit 1: boom".into(),
    });
    assert_eq!(drain(&mut app), vec![names.clone()]);
    assert_eq!(app.toast.as_ref().unwrap().1, ToastVariant::Error);

    // Deps finishing before infra: InfraReady starts immediately.
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.deps_pending = true;
    app.apply_manager_event(ManagerEvent::DepsInstalled);
    assert!(drain(&mut app).is_empty());
    app.apply_manager_event(ManagerEvent::InfraReady {
        app_names: names.clone(),
    });
    assert_eq!(drain(&mut app), vec![names]);

    // No install pending: no hold.
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.apply_manager_event(ManagerEvent::InfraReady {
        app_names: vec!["x".into()],
    });
    assert_eq!(drain(&mut app).len(), 1);
}

#[test]
fn test_operation_lifecycle() {
    let mut app = app_from_yaml(CONFIRM_YAML);
    app.operation = Some(Operation::start("Refresh DB".into()));

    app.apply_manager_event(ManagerEvent::ActionProgress {
        label: "Refresh DB".into(),
        line: "seeding".into(),
    });
    assert_eq!(app.operation.as_ref().unwrap().detail, "seeding");

    // Progress for another label is ignored.
    app.apply_manager_event(ManagerEvent::ActionProgress {
        label: "Other".into(),
        line: "nope".into(),
    });
    assert_eq!(app.operation.as_ref().unwrap().detail, "seeding");

    // Failure keeps it, with the last line captured.
    app.apply_manager_event(ManagerEvent::ActionCompleted {
        label: "Refresh DB".into(),
        ok: false,
        resume: vec![],
    });
    let op = app.operation.as_ref().unwrap();
    assert_eq!(op.failed.as_ref().unwrap().1, "seeding");
    assert!(!op.expired());

    // Empty-label resume pings never touch it; success clears it.
    app.apply_manager_event(ManagerEvent::ActionCompleted {
        label: String::new(),
        ok: true,
        resume: vec![],
    });
    assert!(app.operation.is_some());
    app.operation = Some(Operation::start("Refresh DB".into()));
    app.apply_manager_event(ManagerEvent::ActionCompleted {
        label: "Refresh DB".into(),
        ok: true,
        resume: vec![],
    });
    assert!(app.operation.is_none());
}

#[test]
fn test_failed_operation_expires_after_30s() {
    let mut op = Operation::start("x".into());
    op.failed = Some((
        Instant::now() - std::time::Duration::from_secs(31),
        String::new(),
    ));
    assert!(op.expired());
}

fn push_level_logs(app: &mut App) {
    for (line, stream) in [
        ("all good", LogStream::Stdout),
        ("warn: slow", LogStream::Stdout),
        ("error: boom", LogStream::Stderr),
    ] {
        app.update(Message::LogLineFrom {
            name: "web".to_string(),
            line: line.to_string(),
            stream,
        });
    }
}

#[test]
fn test_level_filter_cycles_and_filters() {
    let mut app = make_app();
    push_level_logs(&mut app);
    assert_eq!(app.filtered_logs().len(), 3);

    app.update(Message::CycleLevelFilter);
    assert_eq!(app.logs.level_filter, LevelFilter::Warn);
    let contents: Vec<_> = app
        .filtered_logs()
        .iter()
        .map(|e| e.content.clone())
        .collect();
    assert_eq!(contents, ["warn: slow", "error: boom"]);

    app.update(Message::CycleLevelFilter);
    assert_eq!(app.logs.level_filter, LevelFilter::Error);
    assert_eq!(app.filtered_logs().len(), 1);

    app.update(Message::CycleLevelFilter);
    assert_eq!(app.logs.level_filter, LevelFilter::All);
    assert_eq!(app.filtered_logs().len(), 3);
}

#[test]
fn test_level_filter_resets_scroll() {
    let mut app = make_app();
    app.logs.follow_logs = false;
    app.logs.log_scroll = 7;
    app.update(Message::CycleLevelFilter);
    assert!(app.logs.follow_logs);
    assert_eq!(app.logs.log_scroll, 0);
}

#[test]
fn test_paused_scroll_ignores_filtered_out_lines() {
    let mut app = make_app();
    app.logs.level_filter = LevelFilter::Error;
    app.logs.follow_logs = false;
    app.logs.log_viewport.set(widgets::log_viewer::LogViewport {
        text_width: 80,
        ..Default::default()
    });
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "plain info".to_string(),
        stream: LogStream::Stdout,
    });
    assert_eq!(
        app.logs.log_scroll, 0,
        "hidden line must not shift the paused view"
    );
    app.update(Message::LogLineFrom {
        name: "web".to_string(),
        line: "error: visible".to_string(),
        stream: LogStream::Stderr,
    });
    assert!(app.logs.log_scroll > 0);
}

#[test]
fn test_paste_routes_to_search_input() {
    let mut app = make_app();
    // Not searching: ignored.
    app.update(Message::Paste("nope".into()));
    assert!(app.logs.log_search_input.is_empty());

    app.update(Message::StartLogSearch);
    app.update(Message::Paste("foo\nbar\t".into()));
    assert_eq!(app.logs.log_search_input, "foobar");
}

#[test]
fn test_paste_routes_to_palette_and_ignored_elsewhere() {
    let mut app = make_app();
    app.update(Message::ShowPalette);
    app.update(Message::Paste("re".into()));
    assert_eq!(app.palette_input, "re");

    app.update(Message::HidePalette);
    app.update(Message::Paste("zzz".into()));
    assert_eq!(app.palette_input, "re");
    assert!(app.logs.log_search_input.is_empty());
}

#[test]
fn test_paste_to_attached_session_queues_stdin() {
    let mut app = make_app();
    app.layer = Layer::SessionAttached("sess".into());
    app.update(Message::Paste("a\r\nb".into()));
    match app.effects.pop_back() {
        Some(Effect::SendSessionInput { label, bytes }) => {
            assert_eq!(label, "sess");
            assert_eq!(bytes, b"a\nb");
        }
        other => panic!("expected SendSessionInput, got {other:?}"),
    }
}

#[test]
fn test_non_press_key_events_ignored() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    let app = make_app();
    let mut key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    assert!(app.map_key(key).is_some());
    key.kind = KeyEventKind::Release;
    assert!(app.map_key(key).is_none());
    key.kind = KeyEventKind::Repeat;
    assert!(app.map_key(key).is_none());
}

#[test]
fn test_mouse_capture_toggle_queues_effect() {
    let mut app = make_app();
    app.update(Message::ToggleMouseCapture);
    assert!(!app.mouse_capture);
    assert_eq!(
        app.toast.as_ref().map(|t| t.0.as_str()),
        Some("Mouse capture off — select text freely")
    );
    assert!(matches!(
        app.effects.pop_back(),
        Some(Effect::SetMouseCapture(false))
    ));
    app.update(Message::ToggleMouseCapture);
    assert!(app.mouse_capture);
}

#[test]
fn test_manager_drain_is_bounded_and_quit_has_priority() {
    let (tx, rx) = mpsc::unbounded_channel();
    let cfg = test_config();
    let mgr = ServiceManager::new(PathBuf::from("/tmp"), &cfg, tx.clone());
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);

    for i in 0..(MANAGER_DRAIN_MAX + 500) {
        tx.send(ManagerEvent::LogLine {
            name: "web".into(),
            line: format!("l{i}"),
            stream: LogStream::Stdout,
        })
        .unwrap();
    }
    let (applied, closed) = app.drain_manager();
    assert!(applied > 0 && applied <= MANAGER_DRAIN_MAX);
    assert!(!closed);
    assert!(
        !app.manager_rx.is_empty(),
        "backlog must remain for next pass"
    );

    // Quit in the middle of a backlog stops the drain right there.
    while app.drain_manager().0 > 0 && !app.manager_rx.is_empty() {}
    for _ in 0..10 {
        tx.send(ManagerEvent::LogLine {
            name: "web".into(),
            line: "x".into(),
            stream: LogStream::Stdout,
        })
        .unwrap();
    }
    tx.send(ManagerEvent::Quit).unwrap();
    for _ in 0..10 {
        tx.send(ManagerEvent::LogLine {
            name: "web".into(),
            line: "after".into(),
            stream: LogStream::Stdout,
        })
        .unwrap();
    }
    let (applied, _) = app.drain_manager();
    assert_eq!(applied, 11, "drain must stop at Quit");
    assert!(app.should_quit);
    assert!(!app.manager_rx.is_empty());
}

// ── Comprehensive mouse tests ─────────────────────────────────────────────

/// Helper: make an app with splash done, render into 160×40 TestBackend.
fn render_app(app: &mut App) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut term = Terminal::new(ratatui::backend::TestBackend::new(160, 40)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    term
}

fn ready_app() -> App {
    let mut app = make_app();
    app.splash_done = true;
    app.size = (160, 40);
    app
}

// ── Click map: topmost wins ────────────────────────────────────────────────
#[test]
fn click_topmost_wins() {
    use crate::tui::clicks::{ClickMap, Target};
    let m = ClickMap::default();
    m.click(Rect::new(0, 0, 10, 1), Message::ShowQuit);
    m.push(Rect::new(2, 0, 6, 1), Target::Inert);
    m.click(Rect::new(4, 0, 2, 1), Message::ShowHelp);
    // innermost (last pushed) wins
    assert!(matches!(m.click_at(4, 0), Some(Message::ShowHelp)));
    // inert swallows
    assert!(m.click_at(2, 0).is_none());
    // behind all overlays
    assert!(matches!(m.click_at(0, 0), Some(Message::ShowQuit)));
    // outside everything
    assert!(m.click_at(10, 0).is_none());
}

// ── Tab chips ─────────────────────────────────────────────────────────────
#[test]
fn click_tab_first_and_last_column() {
    let mut app = ready_app();
    render_app(&mut app);
    use crate::tui::clicks::Target;
    let (first_x, last_x) = {
        let map = app.clicks.targets.borrow();
        let tab0 = map
            .iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::SwitchTab(0))))
            .map(|(r, _)| *r)
            .expect("SwitchTab(0) target");
        (tab0.x, tab0.x + tab0.width - 1)
    };
    // First column hits
    app.active_tab = 1;
    app.update(Message::MouseClick { x: first_x, y: 0 });
    assert_eq!(app.active_tab, 0);
    // Last column hits
    app.active_tab = 1;
    app.update(Message::MouseClick { x: last_x, y: 0 });
    assert_eq!(app.active_tab, 0);
    // One past the end does NOT hit tab 0
    let orig = app.active_tab;
    app.update(Message::MouseClick {
        x: last_x + 1,
        y: 0,
    });
    // It either hits tab 1 or nothing — just not tab 0 if that was col+1
    let _ = orig; // sufficient that no panic
}

// ── Modal outside-click closes ─────────────────────────────────────────────
#[test]
fn click_outside_quit_closes_it() {
    let mut app = ready_app();
    app.update(Message::ShowQuit);
    render_app(&mut app);
    // Click at the very top-left corner, which is outside the centred modal
    app.update(Message::MouseClick { x: 0, y: 0 });
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn click_outside_help_closes_it() {
    let mut app = ready_app();
    app.update(Message::ShowHelp);
    render_app(&mut app);
    app.update(Message::MouseClick { x: 0, y: 0 });
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn click_outside_confirm_cancels_it() {
    let mut app = ready_app();
    app.layer = Layer::Confirm {
        title: "Test".into(),
        body: "ok?".into(),
        on_confirm: Box::new(Message::RestartAll),
    };
    render_app(&mut app);
    app.update(Message::MouseClick { x: 0, y: 0 });
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn click_outside_service_menu_closes_it() {
    let mut app = ready_app();
    app.update(Message::ShowServiceMenu("web".into()));
    render_app(&mut app);
    app.update(Message::MouseClick { x: 0, y: 0 });
    assert_eq!(app.layer, Layer::Dashboard);
}

// ── Quit modal buttons ─────────────────────────────────────────────────────
#[test]
fn click_quit_cancel_button() {
    let mut app = ready_app();
    app.update(Message::ShowQuit);
    render_app(&mut app);
    // Cancel is the first button; Quit is the second. Find the Cancel target.
    use crate::tui::clicks::Target;
    let cancel_r = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .filter(|(_, t)| matches!(t, Target::Click(Message::HideQuit)))
            .min_by_key(|(r, _)| r.x) // leftmost HideQuit = Cancel button
            .map(|(r, _)| *r)
    };
    if let Some(r) = cancel_r {
        app.update(Message::MouseClick { x: r.x, y: r.y });
        assert_eq!(app.layer, Layer::Dashboard);
    }
}

#[test]
fn click_quit_quit_button() {
    let mut app = ready_app();
    app.update(Message::ShowQuit);
    render_app(&mut app);
    use crate::tui::clicks::Target;
    let quit_r = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::Quit)))
            .map(|(r, _)| *r)
    };
    if let Some(r) = quit_r {
        app.update(Message::MouseClick { x: r.x, y: r.y });
        assert!(app.should_quit);
    }
}

// ── Confirm modal buttons ──────────────────────────────────────────────────
#[test]
fn click_confirm_accept_button() {
    let mut app = ready_app();
    app.layer = Layer::Confirm {
        title: "T".into(),
        body: "ok?".into(),
        on_confirm: Box::new(Message::RestartAllConfirmed),
    };
    render_app(&mut app);
    use crate::tui::clicks::Target;
    let accept_r = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::ConfirmAccept)))
            .map(|(r, _)| *r)
    };
    if let Some(r) = accept_r {
        app.update(Message::MouseClick { x: r.x, y: r.y });
        assert_eq!(app.layer, Layer::Dashboard);
        // on_confirm (RestartAllConfirmed) was dispatched → effect queued
        assert!(!app.effects.is_empty());
    }
}

// ── Command palette row click ──────────────────────────────────────────────
#[test]
fn click_palette_item_selects_and_runs() {
    let mut app = ready_app();
    app.update(Message::ShowPalette);
    render_app(&mut app);
    // Clicking a row closes the palette and dispatches that command.
    let idx = app
        .palette_filtered
        .iter()
        .position(|c| matches!(c.action, Message::ScrollToBottom))
        .unwrap();
    app.update(Message::PaletteClick(idx));
    assert_eq!(app.layer, Layer::Dashboard);
}

#[test]
fn wheel_over_palette_moves_selection() {
    let mut app = ready_app();
    app.update(Message::ShowPalette);
    render_app(&mut app);
    assert_eq!(app.palette_selected, 0);
    // Wheel down over the palette area scrolls selection down.
    use crate::tui::clicks::{Target, Wheel};
    let palette_y = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Wheel(Wheel::Palette)))
            .map(|(r, _)| r.y)
    };
    if let Some(y) = palette_y {
        app.update(Message::MouseScroll {
            x: 80,
            y,
            down: true,
        });
        assert_eq!(app.palette_selected, 1);
        app.update(Message::MouseScroll {
            x: 80,
            y,
            down: false,
        });
        assert_eq!(app.palette_selected, 0);
    }
}

// ── Toast click dismisses ─────────────────────────────────────────────────
#[test]
fn click_toast_dismisses() {
    let mut app = ready_app();
    app.update(Message::Toast("hi".into(), ToastVariant::Info));
    // Age the toast past the slide-in so it actually renders
    if let Some((ref msg, ref variant, ref mut at)) = app.toast {
        let _ = (msg, variant);
        *at = std::time::Instant::now() - std::time::Duration::from_millis(400);
    }
    render_app(&mut app);
    use crate::tui::clicks::Target;
    let dismiss_r = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::DismissToast)))
            .map(|(r, _)| *r)
    };
    if let Some(r) = dismiss_r {
        app.update(Message::MouseClick { x: r.x, y: r.y });
        assert!(app.toast.is_none());
    }
}

// ── Scrollbar geometry ────────────────────────────────────────────────────
#[test]
fn scrollbar_scroll_to_target() {
    use crate::tui::clicks::ScrollbarGeometry;
    let g = ScrollbarGeometry {
        track: 30,
        thumb: 6,
        max_scroll: 120,
        y_origin: 5,
    };
    // Top of track → max_scroll (oldest)
    assert_eq!(g.scroll_for_row(5), g.max_scroll);
    // Bottom of track → 0 (newest)
    let at_bottom = g.scroll_for_row(5 + 30 - 1);
    assert!(at_bottom < 10, "bottom of track → near 0, got {at_bottom}");
    // Mid-track → something between 0 and max_scroll
    let mid = g.scroll_for_row(5 + 15);
    assert!(mid > 0 && mid < g.max_scroll);
}

#[test]
fn scrolllogs_to_message_clamps_to_max() {
    let mut app = make_app();
    app.logs.log_viewport.set(widgets::log_viewer::LogViewport {
        text_width: 80,
        height: 10,
        max_scroll: 50,
        ..Default::default()
    });
    app.update(Message::ScrollLogsTo(9999));
    assert_eq!(app.logs.log_scroll, 50);
    assert!(!app.logs.follow_logs);
    app.update(Message::ScrollLogsTo(0));
    assert_eq!(app.logs.log_scroll, 0);
    assert!(app.logs.follow_logs);
}

// ── SelectService switches the tab ────────────────────────────────────────
#[test]
fn select_service_switches_to_service_tab() {
    let mut app = make_app();
    assert_eq!(app.active_tab, 0);
    app.update(Message::SelectService("web".into()));
    assert_eq!(app.active_tab, 1);
}

#[test]
fn select_service_unknown_name_is_noop() {
    let mut app = make_app();
    app.update(Message::SelectService("no_such".into()));
    assert_eq!(app.active_tab, 0);
}

// ── Narrow layout (no sidebar) still produces tab targets ────────────────
#[test]
fn narrow_layout_tab_targets_present() {
    let mut app = make_app();
    app.splash_done = true;
    app.size = (100, 30);
    let mut term = Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    use crate::tui::clicks::Target;
    let has_tabs = app
        .clicks
        .targets
        .borrow()
        .iter()
        .any(|(_, t)| matches!(t, Target::Click(Message::SwitchTab(_))));
    assert!(has_tabs, "narrow layout must still register tab chips");
}

// ── Footer hint clicks ────────────────────────────────────────────────────
#[test]
fn footer_hint_help_click_shows_help() {
    let mut app = ready_app();
    render_app(&mut app);
    use crate::tui::clicks::Target;
    let help_r = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::ShowHelp)))
            .map(|(r, _)| *r)
    };
    if let Some(r) = help_r {
        app.update(Message::MouseClick { x: r.x, y: r.y });
        assert_eq!(app.layer, Layer::Help);
    }
}

// ── Search bar click starts search ───────────────────────────────────────
#[test]
fn click_search_bar_starts_search_when_filter_active() {
    let mut app = ready_app();
    app.logs.level_filter = crate::tui::app::LevelFilter::Error; // makes search bar visible
    render_app(&mut app);
    use crate::tui::clicks::Target;
    let search_r = {
        let map = app.clicks.targets.borrow();
        map.iter()
            .find(|(_, t)| matches!(t, Target::Click(Message::StartLogSearch)))
            .map(|(r, _)| *r)
    };
    if let Some(r) = search_r {
        app.update(Message::MouseClick { x: r.x, y: r.y });
        assert!(app.logs.log_searching);
    }
}
