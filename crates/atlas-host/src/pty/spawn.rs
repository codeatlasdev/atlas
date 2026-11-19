//! Starting a child: a real PTY, or plain pipes for line protocols.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[cfg(unix)]
use super::isolation::{IsoRef, Isolation};
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

/// The shell a terminal tab starts: the one the environment probe resolved (override, the account's login
/// shell, `$SHELL`), so a tab and a service see the same PATH. Read from the cached probe: no passwd
/// lookup or `stat` per tab.
#[cfg(unix)]
fn login_shell() -> String {
    atlas_core::login_env::shell()
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

/// Who a terminal is, set per terminal by hostd (or by the launcher in the request). Never
/// inherited: a hostd started from inside an Atlas terminal carried that terminal's pane key and
/// hook endpoint, and every terminal it opened reported its agent as that other pane.
const TERMINAL_IDENTITY: [&str; 5] = [
    "ATLAS_PANE_KEY",
    "ATLAS_HOOK_ENDPOINT",
    "ATLAS_PTY_ID",
    "ATLAS_TAB_ID",
    "ATLAS_WORKTREE_ID",
];

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
    for k in TERMINAL_IDENTITY {
        cmd.env_remove(k);
    }
    cmd.current_dir(cwd)
        .envs(
            atlas_core::login_env::vars()
                .into_iter()
                .filter(|(k, _)| !TERMINAL_IDENTITY.contains(&k.as_str())),
        )
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    // Own session, like a PTY child.
    crate::exec::new_session(&mut cmd);
    // i18n-ignore: context of an error chain (the program being started), read in logs and after the coded message
    let mut child = cmd.spawn().with_context(|| format!("spawn {}", argv[0]))?;
    // i18n-ignore: context of an error chain (a pipe of the child), read in logs
    let stdout = child.stdout.take().context("stdout")?;
    // i18n-ignore: context of an error chain (a pipe of the child), read in logs
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
        // i18n-ignore: context of an error chain (the syscall that failed), read in logs
        .context("openpty")?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.cwd(cwd);
    for k in TERMINAL_IDENTITY {
        cmd.env_remove(k);
    }
    let inherited = atlas_core::login_env::vars()
        .into_iter()
        .filter(|(k, _)| !TERMINAL_IDENTITY.contains(&k.as_str()));
    // The request's own values come last: hostd's per-terminal identity always wins.
    for (k, v) in inherited.chain(env) {
        cmd.env(k, v);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        // i18n-ignore: context of an error chain (the program being started), read in logs and after the coded message
        .with_context(|| format!("spawn {}", argv[0]))?;
    drop(pair.slave);
    let pid = child.process_id();
    // i18n-ignore: context of an error chain (the syscall that failed), read in logs
    let reader = pair.master.try_clone_reader().context("pty reader")?;
    // i18n-ignore: context of an error chain (the syscall that failed), read in logs
    let writer = pair.master.take_writer().context("pty writer")?;
    Ok(Child {
        reader,
        writer,
        master: Some(Master::Pty(pair.master)),
        pid,
        wait: Box::new(move || child.wait().ok().map(|st| st.exit_code() as i32)),
    })
}

/// Start `req` as a crewmate's own user through the spawn helper: the helper forks, drops to the
/// user and execs; hostd gets the pty master back as a bare fd (`Master::Fd`, the same type an
/// adopted session has). Every variable of the request goes through the helper's validator, so the
/// environment here is only what the agent needs: no hostd login environment is inherited (it holds
/// the crew user's secrets, and the helper refuses `PATH`/`HOME` anyway).
#[cfg(unix)]
fn spawn_isolated(
    client: &crate::crew::spawn::client::Client,
    user: &str,
    argv: &[String],
    req: &PtyCreate,
    env: Vec<(String, String)>,
) -> Result<(Child, Isolation)> {
    use crate::crew::spawn::protocol::{Mode as WireMode, SpawnReq};
    let mut env: std::collections::BTreeMap<String, String> = env.into_iter().collect();
    // The helper sets TERM itself.
    env.remove("TERM");
    let wire = SpawnReq {
        user: user.to_string(),
        argv: argv.to_vec(),
        env,
        cwd: req.cwd.clone(),
        mode: WireMode::Pty,
        cols: req.cols.max(1),
        rows: req.rows.max(1),
    };
    let (sp, fd) = client
        .spawn(&wire)
        // i18n-ignore: context of an error chain (the program and user being started), read in logs and after the coded message
        .with_context(|| format!("spawn {} as {user}", argv[0]))?;
    let reader = fd
        .try_clone()
        // i18n-ignore: context of an error chain (the syscall that failed), read in logs
        .context("pty reader")
        .map(std::fs::File::from)?;
    let writer = fd
        .try_clone()
        // i18n-ignore: context of an error chain (the syscall that failed), read in logs
        .context("pty writer")
        .map(std::fs::File::from)?;
    let iso = Isolation {
        client: client.clone(),
        iso: IsoRef {
            user: user.to_string(),
            session: sp.session,
        },
        pid: sp.pid,
    };
    Ok((
        Child {
            reader: Box::new(reader),
            writer: Box::new(writer),
            master: Some(Master::Fd(fd)),
            pid: Some(sp.pid),
            wait: iso.waiter(),
        },
        iso,
    ))
}

/// Record the terminal's root process under hostd's ownership: if hostd dies, the registry still
/// names the tree and the next sweep reaps it (best effort: a full disk must not stop a terminal).
fn register(scroll_dir: &Path, id: &str, pid: u32, req: &PtyCreate) {
    let root = atlas_core::hygiene::Root {
        session: id.to_string(),
        owner: atlas_core::hygiene::Owner::current(),
        pid,
        start: atlas_core::procs::start_time(pid).unwrap_or(0),
        kind: atlas_core::hygiene::Kind::Terminal,
        label: req.argv.first().cloned().unwrap_or_else(|| "shell".into()),
        project: Some(req.cwd.clone()),
        started_at: now_s(),
    };
    let reg = atlas_core::hygiene::Registry::in_existing(&crate::paths::hygiene_base(scroll_dir));
    let _ = reg.add_root(root);
    // The shell's first children are recorded right away, off the spawn path (a process-table
    // read must not delay the terminal: a client attaching after the spawn would miss output);
    // the daemon's periodic `track` catches what it starts later, before it can daemonize out of
    // the parent chain (macOS cannot read tags).
    std::thread::spawn(move || {
        let _ = reg.track();
    });
}

impl Session {
    /// Spawn `req` in a new PTY. `scroll_dir` holds `<id>.bin` + `<id>.json`.
    pub fn spawn(req: PtyCreate, scroll_dir: &Path) -> Result<Arc<Session>> {
        Self::spawn_as(req, scroll_dir, None)
    }

    /// [`spawn`](Self::spawn), as a crewmate's own Linux user when `as_user` names the helper and the
    /// user (`atlas-cm-<slug>`). The crew runner asks for it when isolation is on; terminals a person
    /// opens never do.
    #[cfg(unix)]
    pub fn spawn_as(
        req: PtyCreate,
        scroll_dir: &Path,
        as_user: Option<(&crate::crew::spawn::client::Client, &str)>,
    ) -> Result<Arc<Session>> {
        Self::spawn_inner(req, scroll_dir, as_user)
    }

    #[cfg(not(unix))]
    pub fn spawn_as(
        req: PtyCreate,
        scroll_dir: &Path,
        _as_user: Option<()>,
    ) -> Result<Arc<Session>> {
        Self::spawn_inner(req, scroll_dir, None)
    }

    fn spawn_inner(
        req: PtyCreate,
        scroll_dir: &Path,
        #[cfg(unix)] as_user: Option<(&crate::crew::spawn::client::Client, &str)>,
        #[cfg(not(unix))] _as_user: Option<()>,
    ) -> Result<Arc<Session>> {
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
        if req.mode != Mode::Pipe {
            // Who the terminal is (Ghostty sets `TERM_PROGRAM=ghostty` + its version): agents branch on it for
            // scroll speed, links and graphics. Set here, not inherited: hostd's own environment may carry the
            // terminal that launched it.
            env.push(("TERM_PROGRAM".into(), "Atlas".into()));
            env.push((
                "TERM_PROGRAM_VERSION".into(),
                env!("CARGO_PKG_VERSION").into(),
            ));
        }
        env.extend(req.env.iter().map(|(k, v)| (k.clone(), v.clone())));
        // Every descendant inherits the owner tag, even one that daemonizes: when the terminal
        // closes or hostd dies, `atlas_core::hygiene` reaps whatever still carries it. Last, so a
        // client's own environment can never retag a process into another session.
        env.push((atlas_core::hygiene::SESSION_ENV.into(), id.clone()));
        #[cfg(unix)]
        let mut iso = None;
        #[cfg(unix)]
        let isolated = as_user.filter(|_| req.mode != Mode::Pipe);
        #[cfg(not(unix))]
        let isolated: Option<()> = None;
        let child = if let Some(target) = isolated {
            #[cfg(unix)]
            {
                let (c, i) = spawn_isolated(target.0, target.1, &argv, &req, env)?;
                iso = Some(i);
                c
            }
            #[cfg(not(unix))]
            {
                let () = target;
                unreachable!("no helper off Unix")
            }
        } else if req.mode == Mode::Pipe {
            spawn_pipe(&argv, &req.cwd, env)?
        } else {
            #[cfg(windows)]
            let run = batch_for_shell_form(&argv, scroll_dir, &id)?;
            #[cfg(unix)]
            let run = argv.clone();
            spawn_pty(&run, &req.cwd, req.cols, req.rows, env)?
        };

        std::fs::create_dir_all(scroll_dir)?;
        // An isolated process is another user's: hostd could not reap it through the hygiene
        // registry, so the helper's `Reap` does that job.
        #[cfg(unix)]
        let register_it = iso.is_none();
        #[cfg(not(unix))]
        let register_it = true;
        if let (Some(pid), true) = (child.pid, register_it) {
            register(scroll_dir, &id, pid, &req);
        }
        let meta = Meta {
            cwd: req.cwd.clone(),
            argv,
            scope: req.scope.clone(),
            created_at: now_s(),
        };
        let _ = atlas_core::fs::write_atomic(
            &meta_path(scroll_dir, &id),
            &serde_json::to_vec(&meta).unwrap_or_default(),
            atlas_core::fs::Mode::Default,
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
                #[cfg(unix)]
                iso,
            },
            scroll_dir,
        );
        s.run(child.reader, child.wait);
        Ok(s)
    }
}
