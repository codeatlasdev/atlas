//! Atlas Desktop: native client of the `atlas dev` runners.

mod agent;
mod browser;
mod cmds;
mod ctl;
#[cfg(debug_assertions)]
mod debug;
mod git;
mod host;
mod live;
mod menu;
mod registry;
mod remote;
mod shell_env;
mod term;
mod tray;
mod updates;
mod workspace;
mod worktree;

use tauri::{AppHandle, Manager, WindowEvent};

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

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// App IPC commands; debug builds append the bridge's commands.
macro_rules! handlers {
    ($($extra:tt)*) => {
        tauri::generate_handler![
            cmds::projects_list,
            cmds::projects_add,
            cmds::live_resync,
            cmds::projects_remove,
            cmds::projects_pin,
            cmds::projects_set_identity,
            cmds::projects_discover,
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
            cmds::open_in_editor,
            cmds::open_url,
            cmds::reveal,
            updates::system_info,
            updates::latest_release,
            updates::releases_between,
            updates::update_all,
            updates::reinstall_app,
            updates::unlink,
            updates::relaunch,
            shell_env::shell_info,
            shell_env::shell_set,
            host::host_call,
            host::host_pty_list,
            host::host_status,
            workspace::workspace_load,
            workspace::workspace_save,
            worktree::worktree_create,
            worktree::worktree_remove,
            worktree::git_refs,
            worktree::worktrees_import,
            worktree::worktree_stats,
            git::git_diff,
            git::git_status,
            git::git_run,
            git::git_files,
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
            browser::window_region_png,
            term::term_open,
            term::term_bounds,
            term::term_close,
            term::term_stats,
            term::term_stats_reset,
            term::term_key,
            term::term_paste,
            term::term_text,
            term::term_scroll,
            term::term_copy,
            term::term_select_start,
            term::term_select_update,
            term::term_select_end,
            term::term_clear_selection,
            term::term_hyperlink_at,
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

fn main() {
    shell_env::start();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&[tray::LABEL])
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(menu::Items::default());
            app.set_menu(menu::build(&handle)?)?;
            app.manage(live::LiveHub::new(&handle));
            app.manage(std::sync::Arc::new(host::HostHub::default()));
            host::start(&handle);
            app.manage(std::sync::Arc::new(agent::hooks::AgentHub::default()));
            agent::hooks::start(&handle);
            app.manage(std::sync::Arc::new(ctl::Ctl::default()));
            ctl::start(&handle);
            app.manage(std::sync::Arc::new(remote::Remotes::default()));
            remote::start(&handle);

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

            tray::start(&handle)?;
            Ok(())
        })
        .on_menu_event(|app, ev| menu::on_event(app, ev.id().as_ref()))
        .on_window_event(|win, ev| match ev {
            // Closing hides; stacks keep running in their runners. ⌘Q quits.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = win.hide();
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
    let builder = builder
        .plugin(debug::plugin())
        .invoke_handler(handlers!(debug::debug_log, debug::debug_reply,));
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(handlers!());
    builder
        .build(tauri::generate_context!())
        .expect("error while building Atlas")
        .run(|app, ev| {
            // Dock icon click with the window hidden.
            if let tauri::RunEvent::Reopen { .. } = ev {
                show_main(app);
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
