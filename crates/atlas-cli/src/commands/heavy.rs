//! `atlas run [--heavy] [--cache] -- <cmd…>`: run a command through the machine-wide queue of heavy
//! jobs (`atlas_core::heavy`) and/or the task cache (`run_cache`), and `atlas heavy status`: who
//! holds the slots, who waits, ETAs.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use atlas_core::heavy::{self, Dir, Entry, State, Status, Ticket};
use atlas_core::taskcache::{Chunk, Stream};
use clap::{Args, Subcommand};

use super::AtlasCommand;

/// How often a waiting job re-checks the queue (a slot frees at the speed of a process exit).
const POLL: Duration = Duration::from_millis(200);
/// How often a job behind the load or memory gate re-reads them.
const GATE_POLL: Duration = Duration::from_secs(5);
/// Exit code of a job stopped by Ctrl-C while it waited (the shell convention, 128 + SIGINT).
const EXIT_INTERRUPTED: i32 = 130;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Queue the command behind the machine's heavy jobs: at most N run at once (default
    /// `min(cores / 2, GB of memory / 8)`; `ATLAS_HEAVY_SLOTS` or `heavy.slots` in
    /// ~/.atlas/config.json), first come first served, and only while load and memory pressure
    /// allow (`ATLAS_HEAVY_MAXLOAD`, `ATLAS_HEAVY_MINMEM`).
    #[arg(long, required_unless_present = "cache")]
    pub heavy: bool,
    /// Name of the job in the queue and its ETA history (default: the program's name).
    #[arg(long)]
    pub label: Option<String>,
    /// Run the command at its own priority; by default it runs under `nice -n 10` (Unix).
    #[arg(long)]
    pub no_nice: bool,
    #[command(flatten)]
    pub cache: super::run_cache::CacheArgs,
    /// The command: one word is a shell command line, several are an argv.
    #[arg(last = true, required = true, num_args = 1..)]
    pub command: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum HeavyCommands {
    /// Who holds the heavy-job slots, who waits, with ETAs from past runs
    Status {
        #[arg(long)]
        json: bool,
    },
}

impl AtlasCommand for HeavyCommands {
    async fn run(self) -> Result<()> {
        match self {
            Self::Status { json } => {
                let st = Dir::default_dir().status(heavy::slots());
                if json {
                    println!("{}", serde_json::to_string_pretty(&st)?);
                } else {
                    print!("{}", render(&st));
                }
                Ok(())
            }
        }
    }
}

impl AtlasCommand for RunArgs {
    async fn run(self) -> Result<()> {
        let code = if self.cache.cache {
            super::run_cache::run(&self).await?
        } else {
            run_job(Some(&Dir::default_dir()), &self, None).await?
        };
        std::process::exit(code);
    }
}

/// The label a job gets when none is given: the program's file name.
fn default_label(command: &[String]) -> String {
    let first = command.first().map_or("", String::as_str);
    let word = first.split_whitespace().next().unwrap_or(first);
    Path::new(word)
        .file_stem()
        .map_or_else(|| word.to_string(), |s| s.to_string_lossy().into_owned())
}

/// The project a job belongs to: the folder the project root resolves to (worktrees differ).
pub(super) fn project_of(cwd: &Path) -> String {
    atlas_core::project_root::resolve(cwd)
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// The job's name: `--label`, else the program's name.
pub(super) fn label_of(args: &RunArgs) -> String {
    args.label
        .clone()
        .filter(|l| !l.trim().is_empty())
        .unwrap_or_else(|| default_label(&args.command))
}

/// Run the command, behind the heavy queue when `queue` is given, and report: returns its exit
/// code. With `capture`, its stdout and stderr are also collected (still passed through live).
pub(super) async fn run_job(
    queue: Option<&Dir>,
    args: &RunArgs,
    capture: Option<&mut Vec<Chunk>>,
) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let command_line = super::open::command_line(&args.command);
    let label = label_of(args);
    let slots = heavy::slots();
    let mut interrupt = Interrupt::new()?;
    let mut ticket = None;
    if let Some(dir) = queue {
        let mut t = dir
            .enqueue(&label, &command_line, &cwd, &project_of(&cwd))
            .with_context(|| format!("cannot join the heavy queue in {}", dir.root().display()))?;
        let mut said = false;
        let queued_since = Instant::now();
        while !t.try_start(slots)? {
            if !said {
                said = true;
                eprintln!("{}", waiting_line(&dir.status(slots), &t, &label));
            }
            if interrupt.fired_within(POLL).await {
                eprintln!("heavy: interrupted while queued ({label})");
                return Ok(EXIT_INTERRUPTED);
            }
        }
        if said {
            eprintln!(
                "heavy: {label} got a slot after {}",
                human(queued_since.elapsed().as_secs())
            );
        }
        if !wait_for_gates(&heavy::Gates::from_env(), &label, &mut interrupt).await {
            return Ok(EXIT_INTERRUPTED);
        }
        ticket = Some(t);
    }

    let started = Instant::now();
    let code = spawn_and_wait(&args.command, args.no_nice, slots, &mut interrupt, capture).await?;
    // Only a command that ran to its end teaches the ETA: a killed or failing-fast run would drag
    // the average down.
    let ran = (code == 0).then(|| started.elapsed().as_secs());
    if let Some(t) = ticket {
        Ticket::finish(t, ran);
    }
    Ok(code)
}

/// Hold the job while memory is short or the 5-minute load is high ([`heavy::Gates`]), at most
/// `wait_s` seconds; then run anyway, warned. False when Ctrl-C ended the wait.
async fn wait_for_gates(gates: &heavy::Gates, label: &str, interrupt: &mut Interrupt) -> bool {
    let start = Instant::now();
    let mut said: Option<String> = None;
    while let Some(hold) = gates.hold_now() {
        let why = describe_hold(&hold);
        if start.elapsed().as_secs() >= gates.wait_s {
            eprintln!(
                "heavy: {why} after {}s; running {label} anyway, timing-sensitive tests may flake",
                gates.wait_s
            );
            break;
        }
        // Say it once per reason: a job that waited for memory and then for load tells both.
        if said.as_deref() != Some(kind(&hold)) {
            said = Some(kind(&hold).to_string());
            eprintln!("heavy: {why}, waiting up to {}s ({label})", gates.wait_s);
        }
        if interrupt.fired_within(GATE_POLL).await {
            eprintln!("heavy: interrupted while waiting for the machine ({label})");
            return false;
        }
    }
    true
}

fn kind(h: &heavy::Hold) -> &'static str {
    match h {
        heavy::Hold::Load { .. } => "load",
        heavy::Hold::Memory { .. } => "memory",
    }
}

fn describe_hold(h: &heavy::Hold) -> String {
    match h {
        heavy::Hold::Load { load, max } => format!("load {load:.1} >= {max:.1}"),
        heavy::Hold::Memory { free_pct, min_pct } => {
            format!("only {free_pct}% of memory free (< {min_pct}%)")
        }
    }
}

/// Run the command as our child, with our stdio, and return its exit code (128 + signal when a
/// signal ended it). A SIGINT/SIGTERM/SIGHUP that reaches us is passed on to the child (its exact
/// pid, never a process group), and we wait for it to end. With `capture` the child's stdout and
/// stderr are pipes that [`tee`] copies to ours while collecting them.
async fn spawn_and_wait(
    command: &[String],
    no_nice: bool,
    slots: usize,
    interrupt: &mut Interrupt,
    capture: Option<&mut Vec<Chunk>>,
) -> Result<i32> {
    let mut cmd = job_command(command, no_nice);
    // `slots` jobs share the machine: cap each one's build parallelism unless the user did.
    cmd.envs(heavy::job_env_now(slots));
    if capture.is_some() {
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn: {}", command.join(" ")))?;
    let pid = child.id();
    let pumps = capture.map(|sink| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let out = tokio::spawn(tee(child.stdout.take(), Stream::Stdout, tx.clone()));
        let err = tokio::spawn(tee(child.stderr.take(), Stream::Stderr, tx));
        (sink, rx, out, err)
    });
    let code = loop {
        tokio::select! {
            status = child.wait() => break exit_code(&status?),
            () = interrupt.wait() => {
                if let Some(pid) = pid {
                    atlas_core::sys::terminate(pid);
                }
            }
        }
    };
    if let Some((sink, mut rx, out, err)) = pumps {
        // The pipes close with the child (or its last holder): drain what is left, in order.
        let _ = tokio::join!(out, err);
        while let Ok(c) = rx.try_recv() {
            sink.push(c);
        }
    }
    Ok(code)
}

/// Copy a child's pipe to our own stdout/stderr as it arrives, and hand each chunk over for the
/// log. Ordering between the two streams is the order chunks reached the channel.
async fn tee(
    pipe: Option<impl tokio::io::AsyncRead + Unpin>,
    stream: Stream,
    tx: tokio::sync::mpsc::UnboundedSender<Chunk>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Some(mut pipe) = pipe else { return };
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let bytes = buf[..n].to_vec();
        match stream {
            Stream::Stdout => {
                let mut o = tokio::io::stdout();
                let _ = o.write_all(&bytes).await;
                let _ = o.flush().await;
            }
            Stream::Stderr => {
                let mut e = tokio::io::stderr();
                let _ = e.write_all(&bytes).await;
                let _ = e.flush().await;
            }
        }
        if tx.send(Chunk { stream, bytes }).is_err() {
            return;
        }
    }
}

/// The command as a tokio process, under `nice -n 10` on Unix unless `no_nice`. Several words are
/// an argv run as is: our child IS the program, so a forwarded signal reaches it. (Through
/// `sh -c` it reached dash, which does not exec its last command, and dash died of the signal
/// while the program's own handler never ran: CI, exit 143.) One word is a command line for the
/// platform shell, parsed as its author wrote it.
fn job_command(command: &[String], no_nice: bool) -> tokio::process::Command {
    let argv: Vec<String> = match command {
        [line] => {
            #[cfg(unix)]
            {
                vec!["sh".into(), "-c".into(), line.clone()]
            }
            #[cfg(not(unix))]
            {
                let _ = no_nice;
                return atlas_core::sys::shell_tokio(line);
            }
        }
        argv => argv.to_vec(),
    };
    // A terminal command: the child shares our console (no `no_window`, which is for GUI parents).
    if cfg!(unix) && !no_nice {
        let mut c = tokio::process::Command::new("nice");
        c.args(["-n", "10"]).args(&argv);
        c
    } else {
        let mut c = tokio::process::Command::new(&argv[0]);
        c.args(&argv[1..]);
        c
    }
}

fn exit_code(s: &std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = s.signal() {
            return 128 + sig;
        }
    }
    s.code().unwrap_or(1)
}

/// Ctrl-C / SIGTERM / SIGHUP as an async source, so a queued job leaves cleanly and a running one
/// passes the signal on to its child instead of dying first and orphaning it.
struct Interrupt {
    #[cfg(unix)]
    sigs: [tokio::signal::unix::Signal; 3],
}

impl Interrupt {
    fn new() -> Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Ok(Self {
                sigs: [
                    signal(SignalKind::interrupt())?,
                    signal(SignalKind::terminate())?,
                    signal(SignalKind::hangup())?,
                ],
            })
        }
        #[cfg(not(unix))]
        Ok(Self {})
    }

    /// Resolves when a signal arrives.
    async fn wait(&mut self) {
        #[cfg(unix)]
        {
            let [a, b, c] = &mut self.sigs;
            tokio::select! { _ = a.recv() => {}, _ = b.recv() => {}, _ = c.recv() => {} }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    }

    /// True when a signal arrived within `d`; sleeps the whole `d` otherwise.
    async fn fired_within(&mut self, d: Duration) -> bool {
        tokio::select! {
            () = self.wait() => true,
            () = tokio::time::sleep(d) => false,
        }
    }
}

fn human(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, secs % 3600 / 60),
    }
}

/// What a job that must wait tells its user.
fn waiting_line(st: &Status, ticket: &Ticket, label: &str) -> String {
    let me = &ticket.job().id;
    let ahead = st.queued.iter().take_while(|e| &e.job.id != me).count();
    let mut s = format!("heavy: {label} is queued ({ahead} ahead");
    if let Some(eta) = st
        .queued
        .iter()
        .find(|e| &e.job.id == me)
        .and_then(|e| e.eta_s)
    {
        s.push_str(&format!(", ~{} to start", human(eta)));
    }
    s.push(')');
    for e in &st.running {
        s.push_str(&format!(
            "\n  running: {} (pid {}, {}, {})",
            e.job.label,
            e.job.pid,
            e.job.project,
            human(e.age_s)
        ));
    }
    s
}

/// The `atlas heavy status` table.
pub fn render(st: &Status) -> String {
    let mut out = format!(
        "{} of {} slot{} busy, {} waiting{}{}\n",
        st.running.len(),
        st.slots,
        if st.slots == 1 { "" } else { "s" },
        st.queued.len(),
        st.load
            .map_or_else(String::new, |l| format!(", load {l:.1}")),
        st.mem_available_pct
            .map_or_else(String::new, |m| format!(", {m}% memory free")),
    );
    let row = |e: &Entry| {
        let eta = e.eta_s.map_or_else(|| "?".to_string(), human);
        let what = match e.job.state {
            State::Running => format!("running {} (eta {eta})", human(e.age_s)),
            State::Queued => format!("waiting {} (starts in ~{eta})", human(e.age_s)),
        };
        format!(
            "  {:<18} {:<16} pid {:<7} {}\n    {}\n",
            e.job.label, e.job.project, e.job.pid, what, e.job.cwd
        )
    };
    st.running
        .iter()
        .chain(&st.queued)
        .for_each(|e| out.push_str(&row(e)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_label_is_the_programs_name_not_its_path_or_arguments() {
        let l = |w: &[&str]| default_label(&w.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(l(&["cargo", "build"]), "cargo");
        assert_eq!(l(&["/usr/bin/cargo", "build"]), "cargo");
        assert_eq!(
            l(&["bun run check"]),
            "bun",
            "a one-word command line is split"
        );
        assert_eq!(l(&["scripts/heavy.sh"]), "heavy");
    }

    #[test]
    fn the_status_header_shows_load_and_free_memory_when_known() {
        let st = |load, mem| Status {
            slots: 3,
            running: vec![],
            queued: vec![],
            load,
            mem_available_pct: mem,
        };
        assert_eq!(
            render(&st(Some(2.5), Some(42))),
            "0 of 3 slots busy, 0 waiting, load 2.5, 42% memory free\n"
        );
        assert_eq!(render(&st(None, None)), "0 of 3 slots busy, 0 waiting\n");
    }

    #[test]
    fn a_hold_is_described_by_what_holds_the_job() {
        let mem = heavy::Hold::Memory {
            free_pct: 7,
            min_pct: 20,
        };
        assert_eq!(describe_hold(&mem), "only 7% of memory free (< 20%)");
        let load = heavy::Hold::Load {
            load: 10.04,
            max: 9.0,
        };
        assert_eq!(describe_hold(&load), "load 10.0 >= 9.0");
        assert_ne!(kind(&mem), kind(&load));
    }

    #[test]
    fn human_durations_are_short() {
        assert_eq!(human(5), "5s");
        assert_eq!(human(65), "1m05s");
        assert_eq!(human(3 * 3600 + 120), "3h02m");
    }
}
