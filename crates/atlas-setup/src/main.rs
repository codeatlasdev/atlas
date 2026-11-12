//! Atlas-Setup.exe: Atlas Desktop's one-click installer and uninstaller for Windows.
//!
//! A small borderless window (drawn by us: no WebView2, which may not exist yet) that unpacks the
//! embedded Tauri NSIS installer, runs it silently, shows real progress and opens Atlas when it is
//! done. `--uninstall` is the same window for removal. See `docs/release.md` ("Windows installer").

// No console window, ever: this is a double-clicked installer.
#![cfg_attr(windows, windows_subsystem = "windows")]

// The engine, the renderer and their tests build on every OS (so CI on Linux and macOS runs them);
// only the Windows binary uses them, hence nothing but the tests compiles elsewhere.
#[cfg(any(windows, test))]
mod cli;
#[cfg(any(windows, test))]
mod fsutil;
#[cfg(any(windows, test))]
mod install;
#[cfg(any(windows, test))]
mod paths;
#[cfg(any(windows, test))]
mod payload;
#[cfg(any(windows, test))]
mod progress;
#[cfg(any(windows, test))]
mod repair;
#[cfg(any(windows, test))]
mod state;
#[cfg(any(windows, test))]
mod ui;
#[cfg(any(windows, test))]
mod uninstall;
#[cfg(windows)]
mod win;

#[cfg(not(windows))]
fn main() {
    // The real installer is Windows only; the crate builds elsewhere so the workspace does.
    eprintln!("Atlas-Setup is the Windows installer; there is nothing to do on this OS.");
}

#[cfg(windows)]
fn main() {
    std::process::exit(windows_main());
}

#[cfg(windows)]
fn windows_main() -> i32 {
    use cli::Mode;
    use state::Shared;

    let args = match cli::parse(std::env::args_os().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            win::window::message_box("Atlas Setup", &e);
            return 2;
        }
    };
    let env = paths::Env::real();
    let log = std::sync::Arc::new(fsutil::Log::create(env.log_file()));
    log.line(format!(
        "atlas-setup {} {:?}",
        env!("CARGO_PKG_VERSION"),
        args
    ));
    let shared = Shared::new(args.mode);

    let worker = {
        let (env, log, shared, args) = (env.clone(), log.clone(), shared.clone(), args.clone());
        std::thread::spawn(move || {
            // A panic in the worker must reach the window as an error, never a frozen "Installing…".
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match args.mode {
                Mode::Install => install::run(
                    &env,
                    &win::host::Real,
                    &shared,
                    &log,
                    &install::Options {
                        launch: args.launch,
                        tick: std::time::Duration::from_millis(120),
                    },
                ),
                Mode::Uninstall => uninstall::run(
                    &env,
                    &win::host::Real,
                    &shared,
                    &log,
                    &uninstall::Options {
                        data: args.data,
                        tick: std::time::Duration::from_millis(60),
                    },
                ),
            }));
            match r {
                Ok(res) => res.is_ok(),
                Err(_) => {
                    log.line("PANIC in the worker");
                    shared.fail(
                        "Something went wrong",
                        format!(
                            "The setup stopped unexpectedly.\n\nLog: {}",
                            log.path().display()
                        ),
                    );
                    false
                }
            }
        })
    };

    if args.silent {
        // No window: same steps, the exit code is the answer.
        return i32::from(!worker.join().unwrap_or(false));
    }
    let code = win::window::run(shared.clone(), true);
    // Closing the window mid-run does not abort the work: the install or uninstall finishes
    // (a half-removed app is worse than a slow close). A failure the window showed stays the exit code.
    let ok = worker.join().unwrap_or(false);
    if code == 0 || !ok {
        code.max(i32::from(!ok))
    } else {
        code
    }
}
