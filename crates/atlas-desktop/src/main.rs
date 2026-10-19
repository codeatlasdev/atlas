//! Atlas Desktop: native client of the `atlas dev` runners.

mod agent;
// Pure, tested decision code driven only by the AppKit side: unused outside macOS, still built so its tests run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod agent_mode;
mod boot;
mod browser;
// Pure decision code driven only by the AppKit monitor: unused outside macOS, still built so its tests run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod browser_keys;
#[cfg(target_os = "macos")]
mod browser_keys_mac;
mod capture;
mod cmds;
mod commit_msg;
mod ctl;
#[cfg(debug_assertions)]
mod debug;
#[cfg(debug_assertions)]
mod debug_req;
mod diag;
#[cfg(target_os = "macos")]
mod display_mac;
mod git;
mod git_branch;
mod host;
mod lifecycle;
mod live;
mod live_server;
mod menu;
#[cfg(target_os = "macos")]
mod page_layers;
#[cfg(not(target_os = "macos"))]
#[path = "page_layers_stub.rs"]
mod page_layers;
// Pure, tested decision code driven only by the AppKit side: unused outside macOS, still built so its tests run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod page_stage;
mod registry;
mod remote;
mod shell_env;
#[cfg(target_os = "macos")]
mod swipe_clock;
// Pure, tested decision code driven only by the AppKit side: unused outside macOS, still built so its tests run.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod swipe_engine;
#[cfg(target_os = "macos")]
mod swipe_mac;
#[cfg(not(target_os = "macos"))]
#[path = "swipe_stub.rs"]
mod swipe_mac;
#[cfg(test)]
mod swipe_trace;
#[cfg(target_os = "macos")]
mod term;
#[cfg(not(target_os = "macos"))]
#[path = "term_stub.rs"]
mod term;
mod tray;
mod updates;
mod workspace;
mod worktree;
mod zoom;

use tauri::{AppHandle, Manager, WindowEvent};

/// The virtual display an agent instance lives on, and how its window gets there.
#[cfg(target_os = "macos")]
mod agent_display;

/// CGWindowID of a window (`screencapture -l`).
#[cfg(target_os = "macos")]
pub fn window_number<R: tauri::Runtime>(win: &tauri::Window<R>) -> Option<isize> {
    let ptr = win.ns_window().ok()?;
    // SAFETY: Tauri hands out the live NSWindow*; -windowNumber is a plain getter.
    let obj = unsafe { (ptr as *mut objc2::runtime::AnyObject).as_ref()? };
    Some(unsafe { objc2::msg_send![obj, windowNumber] })
}

#[cfg(not(target_os = "macos"))]
pub fn window_number<R: tauri::Runtime>(_: &tauri::Window<R>) -> Option<isize> {
    None
}

/// Show the main window. An agent instance (`agent_mode`) puts it on its private virtual display and shows it
/// WITHOUT activating the app: never on the user's monitors, never focus.
// The early `return` is the shape `agent_display`'s guard test checks; off macOS the agent branch is empty.
#[cfg_attr(not(target_os = "macos"), allow(clippy::needless_return))]
fn show_main(app: &AppHandle) {
    let Some(w) = app.get_window("main") else {
        return;
    };
    if agent_mode::Mode::current().focus_allowed() {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    // Agent instances need the private virtual display (macOS); elsewhere they stay hidden.
    #[cfg(target_os = "macos")]
    {
        // Watching first: whatever the placement below does or fails to do, the window cannot stay on his screen.
        agent_display::guard_user_displays(app);
        agent_display::park_and_show(app, &w);
    }
}

/// App IPC commands; debug builds append the bridge's commands.
macro_rules! handlers {
    ($($extra:tt)*) => {
        tauri::generate_handler![
            cmds::projects_list,
            cmds::projects_add,
            cmds::system_ui_sounds,
            cmds::live_resync,
            cmds::projects_remove,
            cmds::projects_pin,
            cmds::projects_set_identity,
            cmds::projects_discover,
            cmds::projects_discover_cancel,
            cmds::projects_relink,
            cmds::projects_remove_missing,
            cmds::projects_find_moved,
            cmds::project_init,
            cmds::project_probe,
            cmds::project_init_preview,
            cmds::stack_start,
            cmds::stack_stop,
            cmds::service_cmd,
            cmds::action_run,
            cmds::project_config,
            cmds::setup_status,
            cmds::setup_run,
            cmds::config_read,
            cmds::config_schema,
            cmds::config_validate,
            cmds::config_write,
            boot::boot_mark,
            boot::app_ready,
            boot::theme_cache_write,
            boot::themes_user_read,
            cmds::open_in_editor,
            cmds::open_url,
            cmds::reveal,
            cmds::theme_unpack,
            cmds::theme_unpack_cleanup,
            cmds::theme_read_packed,
            updates::system_info,
            updates::latest_release,
            updates::update_channels,
            updates::update_channels_offline,
            updates::update_channel_get,
            updates::update_channel_set,
            updates::releases_between,
            updates::update_size,
            updates::update_all,
            updates::reinstall_app,
            updates::unlink,
            updates::relaunch,
            shell_env::shell_info,
            shell_env::shell_set,
            host::host_call,
            host::host_pty_list,
            host::host_status,
            host::host_take_prompt,
            host::host_restart,
            host::host_postpone,
            host::host_restart_when_idle,
            host::host_restart_force,
            host::host_diagnostics,
            workspace::workspace_load,
            workspace::workspace_save,
            worktree::worktree_create,
            worktree::worktree_remove,
            worktree::worktree_auto_remove,
            worktree::git_refs,
            worktree::worktrees_import,
            worktree::worktree_stats,
            git::git_diff,
            git::git_status,
            git::git_run,
            git::git_files,
            commit_msg::commit_msg_provider,
            commit_msg::commit_msg_generate,
            commit_msg::commit_msg_cancel,
            git_branch::git_branches,
            git_branch::git_switch,
            ctl::ctl_reply,
            remote::ssh_config_hosts,
            remote::ssh_status,
            remote::ssh_add,
            remote::ssh_remove,
            remote::ssh_test,
            remote::ssh_forward,
            remote::ssh_forwards,
            remote::projects_add_remote,
            menu::menu_sync,
            zoom::zoom_set,
            live_server::live_open,
            live_server::live_touch,
            live_server::live_stop,
            browser::browser_open,
            browser::browser_bounds,
            browser::browser_park,
            browser::browser_list,
            browser::browser_hide_all,
            browser::browser_navigate,
            browser::browser_nav,
            browser::browser_close,
            browser::browser_eval,
            browser::browser_pick,
            browser::browser_pick_cancel,
            browser_keys::browser_set_keys,
            browser::window_region_png,
            capture::capture_cmd,
            term::term_open,
            term::term_bounds,
            term::term_close,
            term::term_stats,
            term::term_stats_reset,
            swipe_mac::swipe_zones,
            swipe_mac::swipe_native,
            swipe_mac::swipe_go,
            page_layers::pager_pages,
            page_layers::pager_snapshot,
            page_layers::pager_invalidate,
            page_layers::pager_cover,
            page_layers::pager_uncover,
            page_layers::pager_ready,
            page_layers::pager_state,
            term::term_key,
            term::term_paste,
            term::term_paste_clipboard,
            term::term_text,
            term::term_scroll,
            term::term_copy,
            term::term_select_start,
            term::term_select_update,
            term::term_select_end,
            term::term_clear_selection,
            term::term_hyperlink_at,
            term::term_path_at,
            term::term_select_output,
            term::term_mouse,
            term::term_mouse_captured,
            term::term_hover,
            term::term_search,
            term::term_search_clear,
            term::term_recolor,
            term::term_set_prefs,
            term::term_focus,
            term::term_jump_prompt,
            term::term_scroll_to,
            term::term_select_all,
            term::term_clear,
            term::term_click_move,
            term::term_font,
            shell_env::settings_get,
            agent::registry::agent_list,
            agent::launch::agent_command,
            agent::hooks::agent_status_all,
            agent::hooks::agent_pane_exited,
            agent::hooks::agent_bind,
            agent::sessions::agent_sessions,
            agent::sessions::listening_ports,
            agent::notify::agent_notify,
            agent::notify::set_badge,
            tray::tray_publish,
            tray::tray_snapshot,
            tray::tray_hide,
            tray::tray_resize,
            tray::tray_action,
            shell_env::settings_patch,
            $($extra)*
        ]
    };
}

/// window-state restores the last position, which can belong to a monitor that is
/// no longer connected; the window then "opens" invisibly. Center it instead.
fn recenter_if_offscreen(win: &tauri::Window) {
    // An agent instance is parked on its virtual display by `show_main`; centering would move it onto the user's.
    if !agent_mode::Mode::current().focus_allowed() {
        return;
    }
    let (Ok(pos), Ok(size), Ok(monitors)) = (
        win.outer_position(),
        win.outer_size(),
        win.available_monitors(),
    ) else {
        return;
    };
    let rect = (pos.x, pos.y, size.width as i32, size.height as i32);
    let screens: Vec<_> = monitors
        .iter()
        .map(|m| {
            let (p, s) = (m.position(), m.size());
            (p.x, p.y, s.width as i32, s.height as i32)
        })
        .collect();
    if !visible_on_any(rect, &screens) {
        let _ = win.center();
    }
}

/// True when at least half of `win` (x, y, w, h), in each axis, lies on one of
/// `screens`. A thin strip on screen still reads as "the app did not open".
fn visible_on_any(win: (i32, i32, i32, i32), screens: &[(i32, i32, i32, i32)]) -> bool {
    screens.iter().any(|&(sx, sy, sw, sh)| {
        let w = (win.0 + win.2).min(sx + sw) - win.0.max(sx);
        let h = (win.1 + win.3).min(sy + sh) - win.1.max(sy);
        w * 2 >= win.2 && h * 2 >= win.3
    })
}

/// The main window from `tauri.conf.json` (`create: false`), built here so it can start hidden
/// with the cached theme already on `<html>` and in the native appearance. `boot::app_ready`
/// (first real frame) or `boot`'s deadline shows it; window-state restores size/position/maximized
/// while it is still hidden, so it never visibly moves or resizes.
fn create_main_window(app: &tauri::App) -> tauri::Result<()> {
    let cache = boot::load_cache();
    let cfg = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let (theme, script) = boot::window_boot(cache.as_ref());
    let win = tauri::WebviewWindowBuilder::from_config(app.handle(), cfg)?
        .visible(false)
        .theme(theme)
        .initialization_script(script)
        .build()?;
    boot::mark("window:created");
    boot::arm_deadline(app.handle());
    drop(win);
    Ok(())
}

fn main() {
    diag::install_panic_hook();
    boot::start();
    boot::mark("main");
    shell_env::start();
    boot::mark("shell_env:started");
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&[tray::LABEL])
                // Showing is `boot`'s call (first frame), not window-state's at creation.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .setup(|app| {
            boot::mark("setup:begin");
            // An agent instance is invisible to the user: no Dock icon, never frontmost (agent_mode.rs).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(match agent_mode::Mode::current().policy() {
                agent_mode::Policy::Regular => tauri::ActivationPolicy::Regular,
                agent_mode::Policy::Prohibited => tauri::ActivationPolicy::Prohibited,
            });
            // First: WebKit's WebContent process spawns while the rest of setup runs. Nothing can
            // reach managed state before `setup` returns (the page's scripts need the event loop).
            create_main_window(app)?;
            let handle = app.handle().clone();
            app.manage(menu::Items::default());
            app.set_menu(menu::build(&handle)?)?;
            app.manage(live::LiveHub::new(&handle));
            app.manage(cmds::Discovery::default());
            let servers = std::sync::Arc::new(live_server::LiveServers::default());
            servers.start_reaper();
            app.manage(servers);
            app.manage(std::sync::Arc::new(host::HostHub::default()));
            host::start(&handle);
            app.manage(std::sync::Arc::new(agent::hooks::AgentHub::default()));
            app.manage(commit_msg::Runs::default());
            agent::hooks::start(&handle);
            app.manage(std::sync::Arc::new(capture::Capture::default()));
            app.manage(std::sync::Arc::new(ctl::Ctl::default()));
            ctl::start(&handle);
            app.manage(std::sync::Arc::new(remote::Remotes::default()));
            remote::start(&handle);

            #[cfg(target_os = "macos")]
            swipe_mac::install(&handle);
            #[cfg(target_os = "macos")]
            browser_keys_mac::install(&handle);
            #[cfg(target_os = "macos")]
            if let Some(win) = app.get_window("main") {
                use window_vibrancy::{
                    NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy,
                };
                let _ = apply_vibrancy(
                    &win,
                    NSVisualEffectMaterial::Sidebar,
                    Some(NSVisualEffectState::FollowsWindowActiveState),
                    Some(12.0),
                );
            }

            if agent_mode::Mode::current().tray_allowed() {
                tray::start(&handle)?;
            }
            boot::mark("setup:end");
            Ok(())
        })
        .on_page_load(|wv, p| {
            if wv.label() == "main" {
                boot::mark(&format!("page:{:?}", p.event()));
            }
        })
        .on_menu_event(|app, ev| menu::on_event(app, ev.id().as_ref()))
        .on_window_event(|win, ev| match ev {
            // Closing hides while the tray can bring the window back; stacks keep running in their runners.
            // ⌘Q / the tray's Quit exit. With no tray host (Linux without an AppIndicator host) there is no way
            // back to a hidden window, so closing the main window quits (lifecycle.rs).
            WindowEvent::CloseRequested { api, .. } => {
                if win.label() != "main"
                    || lifecycle::close(tray::reachable(win.app_handle())) == lifecycle::Close::Hide
                {
                    api.prevent_close();
                    let _ = win.hide();
                }
            }
            // window-state's restored position lands as the first move, after setup
            // and Ready; check it once there.
            WindowEvent::Moved(_) if win.label() == "main" => {
                static CHECKED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !CHECKED.swap(true, std::sync::atomic::Ordering::Relaxed)
                    && let Some(w) = win.app_handle().get_window(win.label())
                {
                    recenter_if_offscreen(&w);
                }
            }
            _ => {}
        });
    // Debug builds (`desktop:dev`, `atlas link --desktop`) get the agent bridge;
    // release builds never contain it.
    #[cfg(debug_assertions)]
    let builder = builder.plugin(debug::plugin()).invoke_handler(handlers!(
        debug::debug_log,
        debug::debug_reply,
        debug::gesture_replay_samples,
    ));
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(handlers!());
    boot::mark("builder:configured");
    let ctx = tauri::generate_context!();
    boot::mark("context:generated");
    builder
        .build(ctx)
        .expect("error while building Atlas")
        .run(|app, ev| {
            if matches!(ev, tauri::RunEvent::Ready) {
                boot::mark("ready");
            }
            // Dock icon click with the window hidden.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = ev {
                show_main(app);
            }
            // An exit the user did not ask for (the last window going away) is held while the tray can reopen the
            // app; an explicit `exit(code)` (tray Quit, ⌘Q, the updater's restart) always goes through.
            if let tauri::RunEvent::ExitRequested { code, api, .. } = &ev
                && lifecycle::exit(*code, tray::reachable(app)) == lifecycle::Exit::Prevent
            {
                api.prevent_exit();
            }
            // Quitting with "restart when idle" armed and no terminal left: end the old host now.
            if matches!(ev, tauri::RunEvent::Exit) {
                host::on_exit(app);
            }
        });
}
#[cfg(test)]
mod window_tests {
    use super::visible_on_any;

    const LAPTOP: (i32, i32, i32, i32) = (0, 0, 3024, 1964);

    #[test]
    fn window_on_disconnected_external_monitor_is_offscreen() {
        assert!(!visible_on_any((-2592, -208, 2294, 1374), &[LAPTOP]));
    }

    #[test]
    fn window_mostly_on_screen_is_kept_and_a_strip_is_not() {
        assert!(visible_on_any((0, 60, 2560, 1640), &[LAPTOP]));
        assert!(
            visible_on_any((2300, 1000, 1352, 848), &[LAPTOP]),
            "half on screen"
        );
        assert!(
            !visible_on_any((-2480, 60, 2704, 1696), &[LAPTOP]),
            "224px strip: what the app showed after unplugging the monitor"
        );
    }

    #[test]
    fn window_on_a_connected_second_monitor_is_kept() {
        let external = (-2560, -200, 2560, 1440);
        assert!(visible_on_any(
            (-2592, -208, 2294, 1374),
            &[LAPTOP, external]
        ));
    }
}

/// Every `term::` command registered here must exist in `term_stub.rs` too, or only the Linux build
/// fails (CI broke this way on the 1.5.0-alpha.6 commit). Reads both sources, so it runs on macOS.
#[cfg(test)]
mod term_stub_tests {
    #[test]
    fn every_registered_term_command_has_a_linux_stub() {
        let main = include_str!("main.rs");
        let stub = include_str!("term_stub.rs");
        let missing: Vec<&str> = main
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
            .filter_map(|w| w.strip_prefix("term::"))
            .filter(|n| n.starts_with("term_"))
            .filter(|n| !stub.contains(&format!("    {n},")))
            .collect();
        assert!(missing.is_empty(), "add to term_stub.rs: {missing:?}");
    }
}
