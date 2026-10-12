//! The "update when you quit" modal: queuing behind other modals, once per version,
//! accept/decline, and the brand-row flag.

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use crate::event::EventHandler;
use crate::runtime::manager::ServiceManager;

fn app(dir: &std::path::Path) -> App {
    let mut services = HashMap::new();
    services.insert(
        "api".to_string(),
        ServiceDef {
            command: Some("sleep 30".into()),
            port: Some(4000),
            ..Default::default()
        },
    );
    let cfg = ProjectConfig {
        name: "demo".into(),
        services,
        ..Default::default()
    };
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from(dir), &cfg, tx);
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);
    app.splash_done = true;
    app
}

fn is_update_modal(app: &App, version: &str) -> bool {
    match &app.layer {
        Layer::Confirm {
            title,
            body,
            on_confirm,
        } => {
            title == "Update available"
                && body.contains(&format!("v{version}"))
                && body.contains("update when you quit")
                && matches!(&**on_confirm, Message::UpdateOnExit(v) if v == version)
        }
        _ => false,
    }
}

fn brand_row(app: &App) -> String {
    let mut term = Terminal::new(TestBackend::new(200, 40)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn modal_opens_after_splash_with_the_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.offer_update("9.9.9".into());
    assert!(is_update_modal(&app, "9.9.9"), "{:?}", app.layer);
    assert!(app.toast.is_none(), "the toast is gone");
}

#[test]
fn modal_waits_for_the_splash() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.splash_done = false;
    app.offer_update("9.9.9".into());
    assert_eq!(app.layer, Layer::Dashboard);
    assert_eq!(app.update_announced, None, "not consumed: offered later");
    app.splash_done = true;
    app.offer_update("9.9.9".into());
    assert!(is_update_modal(&app, "9.9.9"));
}

#[test]
fn modal_is_queued_behind_any_open_modal() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.layer = Layer::Help;
    app.offer_update("9.9.9".into());
    assert_eq!(app.layer, Layer::Help, "must not replace the open modal");
    assert_eq!(app.update_announced, None, "still pending");

    // A confirm already on screen is not clobbered either.
    app.layer = Layer::Confirm {
        title: "Restart all".into(),
        body: "x".into(),
        on_confirm: Box::new(Message::RestartAllConfirmed),
    };
    app.offer_update("9.9.9".into());
    assert!(matches!(&app.layer, Layer::Confirm { title, .. } if title == "Restart all"));

    // Once the user closes it, the next offer (next tick) shows the update modal.
    app.layer = Layer::Dashboard;
    app.offer_update("9.9.9".into());
    assert!(is_update_modal(&app, "9.9.9"));
}

#[test]
fn modal_shows_once_per_version_per_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.offer_update("9.9.9".into());
    app.update(Message::ConfirmCancel); // "Not now"
    assert_eq!(app.layer, Layer::Dashboard);
    assert!(!app.update_on_exit);

    app.offer_update("9.9.9".into());
    assert_eq!(
        app.layer,
        Layer::Dashboard,
        "same version is not re-offered"
    );

    app.offer_update("10.0.0".into());
    assert!(
        is_update_modal(&app, "10.0.0"),
        "a newer version is offered"
    );
}

#[test]
fn accepting_sets_the_update_on_exit_flag_and_brand_shows_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.offer_update("9.9.9".into());
    app.update(Message::ConfirmAccept); // Enter = Yes, update on exit
    assert_eq!(app.layer, Layer::Dashboard);
    assert!(app.update_on_exit);
    assert!(brand_row(&app).contains("⬆ on exit"));
}

#[test]
fn declining_leaves_the_flag_unset_and_brand_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.offer_update("9.9.9".into());
    app.update(Message::ConfirmCancel); // Esc = Not now
    assert!(!app.update_on_exit);
    assert!(!brand_row(&app).contains("⬆ on exit"));
}

#[test]
fn no_second_modal_after_accepting() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.offer_update("9.9.9".into());
    app.update(Message::ConfirmAccept);
    app.offer_update("10.0.0".into());
    assert_eq!(app.layer, Layer::Dashboard, "already updating on exit");
}

#[test]
fn attached_modal_mentions_the_runner_keeps_the_old_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.remote = Some(Remote::new(dir.path().to_path_buf(), 1));
    app.offer_update("9.9.9".into());
    match &app.layer {
        Layer::Confirm { body, .. } => {
            assert!(body.contains("atlas down && atlas dev --detach"), "{body}")
        }
        other => panic!("{other:?}"),
    }
}
