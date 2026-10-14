//! Starting a child: a real PTY, or plain pipes for line protocols.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use super::master::{Master, Waiter};
use super::{Buf, Init, Io, Meta, Session, bin_path, meta_path, now_s, open_append};
use crate::protocol::{Mode, PtyCreate};

/// The default terminal command: the account's login shell, started as a login shell.
#[cfg(unix)]
fn default_argv() -> Vec<String> {
    vec![login_shell(), "-l".into()]
}

/// The default terminal command on Windows: PowerShell 7 (`pwsh`) when installed, else Windows
/// PowerShell, else `cmd.exe` (`%ComSpec%`).
#[cfg(windows)]
fn default_argv() -> Vec<String> {
    for ps in ["pwsh", "powershell"] {
        if let Some(p) = atlas_core::paths::which(ps) {
            return vec![p.to_string_lossy().into_owned(), "-NoLogo".into()];
        }
    }
    vec![std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())]
}

/// The shell a terminal tab starts: the one the environment probe resolved (override, `$SHELL`, the
/// account's login shell), so a tab and a service see the same PATH.
#[cfg(unix)]
fn login_shell() -> String {
    atlas_core::login_env::resolve_shell().0
}

/// A started child, before it becomes a [`Session`].
struct Child {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    /// `None` in pipe mode.
    master: Option<Master>,
    pid: Option<u32>,
    wait: Waiter,
}

/// Line protocols (omp --mode rpc-ui): stdin/stdout pipes, stderr dropped so it
/// can't corrupt the JSONL stream.
fn spawn_pipe(argv: &[String], cwd: &str, env: Vec<(String, String)>) -> Result<Child> {
    let mut cmd = match atlas_core::sys::shell_script(argv) {
        Some(script) => atlas_core::sys::shell(script),
        None => {
            let mut c = std::process::Command::new(&argv[0]);
            c.args(&argv[1..]);
            c
        }
    };
    cmd.current_dir(cwd)
        .envs(atlas_core::login_env::vars())
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    // Own session, like a PTY child.
    crate::exec::new_session(&mut cmd);
    let mut child = cmd.spawn().with_context(|| format!("spawn {}", argv[0]))?;
    let stdout = child.stdout.take().context("stdout")?;
    let stdin = child.stdin.take().context("stdin")?;
    let pid = Some(child.id());
    Ok(Child {
        reader: Box::new(stdout),
        writer: Box::new(stdin),
        master: None,
        pid,
        wait: Box::new(move || child.wait().ok().and_then(|s| s.code())),
    })
}

/// Windows: ConPTY takes one command line, quoted by MSVC rules that cmd does not read. A
/// shell-form argv ([`atlas_core::sys::shell_argv`]) runs its script from a batch file next to the
/// scrollback instead (removed with it).
#[cfg(windows)]
fn batch_for_shell_form(argv: &[String], dir: &Path, id: &str) -> Result<Vec<String>> {
    let Some(script) = atlas_core::sys::shell_script(argv) else {
        return Ok(argv.to_vec());
    };
    std::fs::create_dir_all(dir)?;
    let file = super::batch_path(dir, id);
    std::fs::write(&file, format!("@echo off\r\n{script}\r\n"))?;
    Ok(vec![
        argv[0].clone(),
        "/d".into(),
        "/c".into(),
        file.to_string_lossy().into_owned(),
    ])
}

/// Windows: the program's file, found on `PATH` with its executable extension (`claude` →
/// `claude.cmd`). ConPTY would otherwise take the first file named exactly `claude`, which for an
/// npm package is the POSIX shell script beside the `.cmd`.
#[cfg(windows)]
fn resolve_program(argv: &mut [String], env: &[(String, String)]) {
    let Some(first) = argv.first_mut() else {
        return;
    };
    if first.contains(['/', '\\']) {
        return;
    }
    let path = env
        .iter()
        .rev()
        .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
        .map(|(_, v)| std::ffi::OsString::from(v))
        .or_else(|| std::env::var_os("PATH"));
    if let Some(p) = path.and_then(|p| atlas_core::paths::which_in(first, &p)) {
        *first = p.to_string_lossy().into_owned();
    }
}

fn spawn_pty(
    argv: &[String],
    cwd: &str,
    cols: u16,
    rows: u16,
    env: Vec<(String, String)>,
) -> Result<Child> {
    #[cfg(windows)]
    let argv = &{
        let mut a = argv.to_vec();
        resolve_program(&mut a, &env);
        a
    };
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("openpty")?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.cwd(cwd);
    for (k, v) in atlas_core::login_env::vars().into_iter().chain(env) {
        cmd.env(k, v);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("spawn {}", argv[0]))?;
    drop(pair.slave);
    let pid = child.process_id();
    let reader = pair.master.try_clone_reader().context("pty reader")?;
    let writer = pair.master.take_writer().context("pty writer")?;
    Ok(Child {
        reader,
        writer,
        master: Some(Master::Pty(pair.master)),
        pid,
        wait: Box::new(move || child.wait().ok().map(|st| st.exit_code() as i32)),
    })
}

impl Session {
    /// Spawn `req` in a new PTY. `scroll_dir` holds `<id>.bin` + `<id>.json`.
    pub fn spawn(req: PtyCreate, scroll_dir: &Path) -> Result<Arc<Session>> {
        let id = crate::paths::new_id();
        let argv = if req.argv.is_empty() {
            default_argv()
        } else {
            req.argv.clone()
        };
        let mut env: Vec<(String, String)> = vec![
            ("TERM".into(), "xterm-256color".into()),
            ("COLORTERM".into(), "truecolor".into()),
            ("ATLAS_HOSTD".into(), "1".into()),
            ("ATLAS_PTY_ID".into(), id.clone()),
        ];
        env.extend(req.env.iter().map(|(k, v)| (k.clone(), v.clone())));
        let child = if req.mode == Mode::Pipe {
            spawn_pipe(&argv, &req.cwd, env)?
        } else {
            #[cfg(windows)]
            let run = batch_for_shell_form(&argv, scroll_dir, &id)?;
            #[cfg(unix)]
            let run = argv.clone();
            spawn_pty(&run, &req.cwd, req.cols, req.rows, env)?
        };

        std::fs::create_dir_all(scroll_dir)?;
        let meta = Meta {
            cwd: req.cwd.clone(),
            argv,
            scope: req.scope.clone(),
            created_at: now_s(),
        };
        let _ = std::fs::write(
            meta_path(scroll_dir, &id),
            serde_json::to_vec(&meta).unwrap_or_default(),
        );
        let file_path = bin_path(scroll_dir, &id);
        let _ = std::fs::remove_file(&file_path);
        let s = Session::new(
            Init {
                id,
                meta,
                pid: child.pid,
                title: req.title.clone().unwrap_or_default(),
                exited: None,
                size: (req.cols, req.rows),
                io: Some(Io {
                    master: child.master,
                    writer: child.writer,
                }),
                buf: Buf::new(Default::default(), open_append(&file_path), 0),
            },
            scroll_dir,
        );
        s.run(child.reader, child.wait);
        Ok(s)
    }
}
