//! Starting a child: a real PTY, or plain pipes for line protocols.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use super::master::{Master, Waiter};
use super::{Buf, Init, Io, Meta, Session, bin_path, meta_path, now_s, open_append};
use crate::protocol::{Mode, PtyCreate};

/// The shell a terminal tab starts: the one the environment probe resolved (override, `$SHELL`, the
/// account's login shell), so a tab and a service see the same PATH.
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
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(cwd)
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

fn spawn_pty(
    argv: &[String],
    cwd: &str,
    cols: u16,
    rows: u16,
    env: Vec<(String, String)>,
) -> Result<Child> {
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
            vec![login_shell(), "-l".into()]
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
            spawn_pty(&argv, &req.cwd, req.cols, req.rows, env)?
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
