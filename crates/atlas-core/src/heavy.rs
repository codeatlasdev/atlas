//! Machine-wide queue for heavy jobs (builds, test suites, benches): at most N run at once on the
//! whole machine, whichever worktree, agent or user shell started them, in arrival order.
//!
//! Everything lives in one directory (`~/.atlas/heavy`, [`Dir::default_dir`]) and nothing in it
//! can go stale:
//!
//! * `slot-<i>.lock`: one per slot; the running job holds an exclusive [`sys::try_lock`] on it, so
//!   the OS frees the slot the moment the holder dies (any way it dies).
//! * `jobs/<id>.json` + `jobs/<id>.lock`: one record per queued or running job. The job holds the
//!   `.lock` for its whole life, so a record whose lock is free belongs to a dead job (no pid
//!   guessing, pids get recycled) and is ignored and swept.
//! * `history.json`: the last run times per label, the ETA source.
//!
//! Fairness is FIFO: a waiter may take a free slot only when it is among the first `free slots`
//! live queued jobs ordered by id (`<queued_at_ms>-<pid>`). Jobs never skip ahead of an older one.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::sys;

/// Slot count override (a positive integer).
pub const ENV_SLOTS: &str = "ATLAS_HEAVY_SLOTS";
/// Queue directory override (tests, sandboxes).
pub const ENV_DIR: &str = "ATLAS_HEAVY_DIR";
/// Load gate override: a job starts only while the 5-minute load is below this. `0` turns the gate off.
/// Default `0.75 * cores`: a machine at its core count is already saturated, and the load average
/// lags, so waiting for headroom is what keeps a build from freezing it.
pub const ENV_MAXLOAD: &str = "ATLAS_HEAVY_MAXLOAD";
/// Seconds a job waits for the load or memory gate before it runs anyway (default 600).
pub const ENV_LOAD_WAIT: &str = "ATLAS_HEAVY_WAIT";
/// Memory gate override: a job starts only while at least this percent of memory is free
/// (default 20). `0` turns the gate off. Also `heavy.min_mem_pct` in `~/.atlas/config.json`.
pub const ENV_MINMEM: &str = "ATLAS_HEAVY_MINMEM";

/// Default of the load gate, as a fraction of the cores.
const LOAD_PER_CORE: f64 = 0.75;
/// Default of the memory gate: percent of memory that must be free.
const DEFAULT_MIN_MEM_PCT: u8 = 20;
/// Memory one default slot is allowed to claim: a cold build of this workspace peaks around it.
const GB_PER_SLOT: u64 = 8;

/// Past runs kept per label for the ETA.
const HISTORY_KEEP: usize = 8;

/// Where a job is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Queued,
    Running,
}

/// The small JSON record of one queued or running job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// `<queued_at_ms, zero padded>-<pid>`: sorts in arrival order, unique per process.
    pub id: String,
    pub pid: u32,
    pub label: String,
    /// The command line, for humans.
    pub command: String,
    pub cwd: String,
    /// Project name (the folder the job was started in resolves to), for the status listing.
    pub project: String,
    pub state: State,
    /// Unix milliseconds.
    pub queued_at: u64,
    /// Unix milliseconds, set when the job got its slot.
    pub started_at: Option<u64>,
    /// The slot a running job holds.
    pub slot: Option<usize>,
}

/// Queue state for `atlas heavy status` and the `atlas_heavy_status` MCP tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub slots: usize,
    /// Running jobs, oldest first.
    pub running: Vec<Entry>,
    /// Waiting jobs in the order they will start.
    pub queued: Vec<Entry>,
    /// The 5-minute load average, `None` where the OS has none.
    pub load: Option<f64>,
    /// Percent of memory free (see [`sys::mem_available_pct`]), `None` when unreadable.
    pub mem_available_pct: Option<u8>,
}

/// A job plus the numbers a listing needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub job: Job,
    /// Seconds the job has been running (running) or waiting (queued).
    pub age_s: u64,
    /// Average seconds of this label's recent runs, `None` with no history.
    pub avg_s: Option<u64>,
    /// Seconds until a running job should finish, or until a queued job should start (from the
    /// history of the jobs ahead of it). `None` when history cannot tell.
    pub eta_s: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct History {
    /// Label to the seconds of its last runs, newest last.
    runs: BTreeMap<String, Vec<u64>>,
}

/// A queue directory. Every operation takes this, so tests use a temp dir and never `~/.atlas`.
#[derive(Debug, Clone)]
pub struct Dir {
    root: PathBuf,
}

/// A job's claim on the queue. Dropping it ends the job: the lock files close, the record goes.
#[derive(Debug)]
pub struct Ticket {
    dir: Dir,
    job: Job,
    // Held for the job's life: its release is what makes the record (and the slot) free.
    _life: File,
    slot_lock: Option<File>,
    /// Test only: end like a killed process (locks close, files stay).
    #[cfg(test)]
    killed: bool,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// `command` routed through the queue by the `atlas` CLI this machine uses (`$ATLAS_BIN`,
/// `~/.atlas/bin/atlas`, the bundled one, `PATH`); a bare `atlas` when none is found, so the shell
/// says "not found" at the point of use rather than the action silently skipping the queue.
pub fn queued(label: &str, command: &str) -> String {
    let atlas = crate::paths::resolve_atlas_cli(
        std::env::var_os("ATLAS_BIN").as_deref(),
        dirs::home_dir().as_deref(),
        crate::paths::bundled_cli(),
        None,
        std::env::var_os("PATH").as_deref(),
    );
    wrap_command(
        &atlas.map_or_else(|| "atlas".to_string(), |p| p.to_string_lossy().into_owned()),
        label,
        command,
    )
}

/// `command` (a shell command line) routed through the queue: `<atlas> run --heavy --label <label>
/// -- <command>`. The command stays ONE argument, which the CLI runs as the shell line it is, so
/// pipes, `&&` and quotes mean what the author wrote. `atlas` is the CLI to call (a path or `atlas`).
pub fn wrap_command(atlas: &str, label: &str, command: &str) -> String {
    format!(
        "{} run --heavy --label {} -- {}",
        sh_quote(atlas),
        sh_quote(label),
        sh_quote(command)
    )
}

/// One shell word: unchanged when plain; `'…'` on Unix and `"…"` for `cmd.exe` otherwise.
fn sh_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c));
    if plain {
        arg.to_string()
    } else if cfg!(windows) {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// Slot count: the env override, else `heavy.slots` in `~/.atlas/config.json`, else
/// `min(cores / 2, total memory in GB / 8)`, at least 1 (see [`slots_from`]).
pub fn slots() -> usize {
    slots_from(
        std::env::var(ENV_SLOTS).ok().as_deref(),
        config_slots(&crate::paths::atlas_home().join("config.json")),
        cores(),
        sys::mem_total_bytes(),
    )
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// [`slots`] with its inputs given (pure, for tests): a positive `env` wins, then `config`, then
/// `min(cores / 2, mem_bytes / 8 GB)` (memory ignored when unknown, 1 GiB of slack for what the
/// firmware hides), never below 1: 12 cores and 24 GB give 3, 16 GB gives 2, 8 GB gives 1. Zero, negative and garbage count as unset.
pub fn slots_from(
    env: Option<&str>,
    config: Option<usize>,
    cores: usize,
    mem_bytes: Option<u64>,
) -> usize {
    env.and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .or(config.filter(|n| *n > 0))
        .unwrap_or_else(|| {
            let by_cpu = cores / 2;
            // 1 GiB of slack: firmware and the kernel hide some of the memory a "16 GB" machine has.
            let by_mem = mem_bytes.map_or(usize::MAX, |b| {
                ((b + (1 << 30)) / (GB_PER_SLOT << 30)) as usize
            });
            by_cpu.min(by_mem).max(1)
        })
}

/// `heavy.slots` of the user config file, `None` when absent or unreadable.
fn config_slots(path: &Path) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    v.get("heavy")?.get("slots")?.as_u64().map(|n| n as usize)
}

/// `heavy.min_mem_pct` of the user config file, `None` when absent or unreadable.
fn config_min_mem(path: &Path) -> Option<u8> {
    let v: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    v.get("heavy")?
        .get("min_mem_pct")?
        .as_u64()
        .map(|n| n.min(100) as u8)
}

/// What holds a job back before it starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hold {
    /// The 5-minute load is at or above the threshold.
    Load { load: f64, max: f64 },
    /// Less memory is free than the threshold asks for.
    Memory { free_pct: u8, min_pct: u8 },
}

/// The load and memory gates every job passes before it starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gates {
    /// Start only while the 5-minute load is below this; `None`: gate off.
    pub max_load: Option<f64>,
    /// Start only while at least this percent of memory is free; `None`: gate off.
    pub min_mem_pct: Option<u8>,
    /// Seconds to wait for the gates before running anyway.
    pub wait_s: u64,
}

impl Gates {
    /// From the environment, the user config file and this machine's core count.
    pub fn from_env() -> Self {
        Self::from_inputs(
            std::env::var(ENV_MAXLOAD).ok().as_deref(),
            std::env::var(ENV_MINMEM).ok().as_deref(),
            config_min_mem(&crate::paths::atlas_home().join("config.json")),
            std::env::var(ENV_LOAD_WAIT).ok().as_deref(),
            cores(),
        )
    }

    /// [`Gates::from_env`] with its inputs given (pure, for tests). Load: unset means
    /// `0.75 * cores`, a number is taken as is, `0` or garbage turn the gate off. Memory: env, else
    /// `config`, else 20 percent; `0` turns it off, above 100 counts as 100, garbage as unset.
    pub fn from_inputs(
        maxload: Option<&str>,
        minmem: Option<&str>,
        minmem_config: Option<u8>,
        wait: Option<&str>,
        cores: usize,
    ) -> Self {
        let max_load = match maxload {
            None => Some(cores.max(1) as f64 * LOAD_PER_CORE),
            Some(v) => v.trim().parse::<f64>().ok().filter(|m| *m > 0.0),
        };
        let min_mem = minmem
            .and_then(|v| v.trim().parse::<u8>().ok())
            .or(minmem_config)
            .unwrap_or(DEFAULT_MIN_MEM_PCT)
            .min(100);
        Self {
            max_load,
            min_mem_pct: (min_mem > 0).then_some(min_mem),
            wait_s: wait.and_then(|v| v.trim().parse().ok()).unwrap_or(600),
        }
    }

    /// The gate holding a job back now, `None` when it may start. `load` is the 5-minute load and
    /// `free_pct` the free memory, each `None` when the OS cannot tell (that gate then stays open:
    /// never block on a reading that does not exist). Memory is reported first: it freezes a
    /// machine, a busy CPU only slows it.
    pub fn hold(&self, load: Option<f64>, free_pct: Option<u8>) -> Option<Hold> {
        if let (Some(min_pct), Some(free_pct)) = (self.min_mem_pct, free_pct)
            && free_pct < min_pct
        {
            return Some(Hold::Memory { free_pct, min_pct });
        }
        if let (Some(max), Some(load)) = (self.max_load, load)
            && load >= max
        {
            return Some(Hold::Load { load, max });
        }
        None
    }

    /// [`Gates::hold`] with the live readings.
    pub fn hold_now(&self) -> Option<Hold> {
        self.hold(sys::load_avg().map(|l| l.1), sys::mem_available_pct())
    }
}

/// Variables that cap how much a heavy job parallelises, so `slots` jobs together stay within the
/// machine: each gets `max(2, cores / slots)` workers. `set` says which the user already set (a
/// value of theirs is never replaced). Cargo (`CARGO_BUILD_JOBS`) and the libuv threadpool of
/// node/bun tooling (`UV_THREADPOOL_SIZE`); bun has no other worker setting. Pure, for tests.
pub fn job_env(
    slots: usize,
    cores: usize,
    set: impl Fn(&str) -> bool,
) -> Vec<(&'static str, String)> {
    let n = (cores / slots.max(1)).max(2).to_string();
    ["CARGO_BUILD_JOBS", "UV_THREADPOOL_SIZE"]
        .into_iter()
        .filter(|k| !set(k))
        .map(|k| (k, n.clone()))
        .collect()
}

/// [`job_env`] for this machine and process environment.
pub fn job_env_now(slots: usize) -> Vec<(&'static str, String)> {
    job_env(slots, cores(), |k| {
        std::env::var_os(k).is_some_and(|v| !v.is_empty())
    })
}

impl Dir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$ATLAS_HEAVY_DIR`, else `~/.atlas/heavy`.
    pub fn default_dir() -> Self {
        Self::new(
            std::env::var_os(ENV_DIR)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| crate::paths::atlas_home().join("heavy")),
        )
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn jobs(&self) -> PathBuf {
        self.root.join("jobs")
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("history.json")
    }

    fn open_lock(path: &Path) -> io::Result<File> {
        let mut o = OpenOptions::new();
        o.create(true).read(true).write(true).truncate(false);
        sys::owner_only(&mut o).open(path)
    }

    /// Join the queue: write the record (state `queued`) and take the job's life lock. The caller
    /// then loops [`Ticket::try_start`] until it returns `true`.
    pub fn enqueue(
        &self,
        label: &str,
        command: &str,
        cwd: &Path,
        project: &str,
    ) -> io::Result<Ticket> {
        fs::create_dir_all(self.jobs())?;
        let (pid, queued_at) = (std::process::id(), now_ms());
        // The pid is unique among live processes; a sequence number keeps ids of one process apart.
        let id = format!("{queued_at:013}-{pid}-{:04}", next_seq());
        let life = Self::open_lock(&self.jobs().join(format!("{id}.lock")))?;
        if !sys::try_lock(&life)? {
            return Err(io::Error::other("job id collision"));
        }
        let job = Job {
            id,
            pid,
            label: label.to_string(),
            command: command.to_string(),
            cwd: cwd.to_string_lossy().into_owned(),
            project: project.to_string(),
            state: State::Queued,
            queued_at,
            started_at: None,
            slot: None,
        };
        let t = Ticket {
            dir: self.clone(),
            job,
            _life: life,
            slot_lock: None,
            #[cfg(test)]
            killed: false,
        };
        t.write()?;
        Ok(t)
    }

    /// Live jobs, queued and running, with dead jobs' files swept. Oldest id first.
    pub fn jobs_live(&self) -> Vec<Job> {
        let Ok(rd) = fs::read_dir(self.jobs()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let lock = p.with_extension("lock");
            // The life lock is free exactly when the job is dead.
            // Never create it: a job finishing right now removes its files, and a sweep that
            // re-created the lock would leave an orphan file nobody cleans. A missing lock is a
            // dead (or just finished) job.
            let dead = match OpenOptions::new().read(true).write(true).open(&lock) {
                Ok(f) => sys::try_lock(&f).unwrap_or(false),
                Err(e) => e.kind() == io::ErrorKind::NotFound,
            };
            if dead {
                let _ = fs::remove_file(&p);
                let _ = fs::remove_file(&lock);
                continue;
            }
            // A record being rewritten is read as it was a moment ago or skipped for this pass.
            if let Some(j) = fs::read(&p)
                .ok()
                .and_then(|b| serde_json::from_slice::<Job>(&b).ok())
            {
                out.push(j);
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// The queue as `atlas heavy status` shows it.
    pub fn status(&self, slots: usize) -> Status {
        let jobs = self.jobs_live();
        let hist = self.history();
        let now = now_ms();
        let avg = |label: &str| hist.runs.get(label).and_then(|r| average(r));
        let secs = |from: u64| now.saturating_sub(from) / 1000;

        let mut running: Vec<Job> = jobs
            .iter()
            .filter(|j| j.state == State::Running)
            .cloned()
            .collect();
        running.sort_by_key(|j| j.started_at.unwrap_or(j.queued_at));
        let queued: Vec<Job> = jobs
            .into_iter()
            .filter(|j| j.state == State::Queued)
            .collect();

        // When each slot frees up: a running job at its start + average (never in the past),
        // unknown when its label has no history. Free slots are free now.
        let mut frees: Vec<Option<u64>> = running
            .iter()
            .map(|j| {
                avg(&j.label).map(|a| {
                    let end = secs(j.started_at.unwrap_or(j.queued_at));
                    a.saturating_sub(end)
                })
            })
            .collect();
        frees.resize(slots.max(running.len()), Some(0));

        let running_e = running
            .iter()
            .zip(frees.iter())
            .map(|(j, f)| Entry {
                age_s: secs(j.started_at.unwrap_or(j.queued_at)),
                avg_s: avg(&j.label),
                eta_s: *f,
                job: j.clone(),
            })
            .collect();

        let mut queued_e = Vec::new();
        for j in queued {
            // This job takes the soonest-freeing slot; unknown frees sort last and poison the ETA.
            frees.sort_by_key(|f| f.unwrap_or(u64::MAX));
            let start = frees.first().copied().flatten();
            let a = avg(&j.label);
            if let Some(slot) = frees.first_mut() {
                *slot = start.zip(a).map(|(s, a)| s + a);
            }
            queued_e.push(Entry {
                age_s: secs(j.queued_at),
                avg_s: a,
                eta_s: start,
                job: j,
            });
        }
        Status {
            slots,
            running: running_e,
            queued: queued_e,
            load: sys::load_avg().map(|l| l.1),
            mem_available_pct: sys::mem_available_pct(),
        }
    }

    fn history(&self) -> History {
        fs::read(self.history_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Remember that a run of `label` took `secs`. Racing writers may lose a sample, never
    /// corrupt the file (write to a temp name, then rename).
    pub fn record_run(&self, label: &str, secs: u64) {
        let mut h = self.history();
        let runs = h.runs.entry(label.to_string()).or_default();
        runs.push(secs);
        let excess = runs.len().saturating_sub(HISTORY_KEEP);
        runs.drain(..excess);
        let tmp = self
            .root
            .join(format!("history.{}.tmp", std::process::id()));
        if let Ok(b) = serde_json::to_vec(&h)
            && fs::write(&tmp, b).is_ok()
        {
            let _ = fs::rename(&tmp, self.history_path());
        }
        let _ = fs::remove_file(&tmp);
    }
}

fn average(runs: &[u64]) -> Option<u64> {
    (!runs.is_empty()).then(|| runs.iter().sum::<u64>() / runs.len() as u64)
}

fn next_seq() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed) % 10_000
}

impl Ticket {
    pub fn job(&self) -> &Job {
        &self.job
    }

    fn write(&self) -> io::Result<()> {
        let path = self.dir.jobs().join(format!("{}.json", self.job.id));
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(&self.job)?)?;
        fs::rename(&tmp, &path)
    }

    /// One attempt to get a slot. `true` once this job runs (its record says so), `false` while it
    /// must keep waiting: all slots are held, or older queued jobs go first.
    pub fn try_start(&mut self, slots: usize) -> io::Result<bool> {
        if self.slot_lock.is_some() {
            return Ok(true);
        }
        let live = self.dir.jobs_live();
        let held = live.iter().filter(|j| j.state == State::Running).count();
        let free = slots.saturating_sub(held);
        let ahead = live
            .iter()
            .filter(|j| j.state == State::Queued && j.id < self.job.id)
            .count();
        // Only the first `free` queued jobs may start: nobody overtakes an older one.
        if ahead >= free {
            return Ok(false);
        }
        for i in 0..slots {
            let f = Dir::open_lock(&self.dir.root.join(format!("slot-{i}.lock")))?;
            if sys::try_lock(&f)? {
                self.job.state = State::Running;
                self.job.started_at = Some(now_ms());
                self.job.slot = Some(i);
                self.slot_lock = Some(f);
                self.write()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// End the job after a run of `secs` seconds that was worth remembering (`Some`) or not.
    pub fn finish(self, ran_secs: Option<u64>) {
        if let Some(s) = ran_secs {
            self.dir.record_run(&self.job.label, s);
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        #[cfg(test)]
        if self.killed {
            return;
        }
        let j = self.dir.jobs();
        let _ = fs::remove_file(j.join(format!("{}.json", self.job.id)));
        let _ = fs::remove_file(j.join(format!("{}.lock", self.job.id)));
        // The slot lock and the life lock close with the struct.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> (tempfile::TempDir, Dir) {
        let t = tempfile::tempdir().unwrap();
        let d = Dir::new(t.path().join("heavy"));
        (t, d)
    }

    fn job(d: &Dir, label: &str) -> Ticket {
        d.enqueue(label, "cmd", Path::new("/tmp"), "proj").unwrap()
    }

    const GIB: u64 = 1 << 30;

    #[test]
    fn slots_precedence_env_then_config_then_the_machine_never_zero() {
        let big = Some(64 * GIB);
        assert_eq!(
            slots_from(Some("3"), Some(5), 8, big),
            3,
            "env beats config"
        );
        assert_eq!(
            slots_from(None, Some(5), 8, big),
            5,
            "config beats the machine"
        );
        assert_eq!(slots_from(None, None, 8, big), 4, "half the cores");
        assert_eq!(
            slots_from(None, None, 1, big),
            1,
            "one core still gets a slot"
        );
        assert_eq!(slots_from(None, None, 0, big), 1);
        for bad in ["0", "-2", "x", ""] {
            assert_eq!(
                slots_from(Some(bad), None, 8, big),
                4,
                "{bad:?} counts as unset"
            );
        }
        assert_eq!(slots_from(Some("2"), Some(0), 8, big), 2);
        assert_eq!(
            slots_from(None, Some(0), 8, big),
            4,
            "a zero config is unset"
        );
        // An explicit choice is the user's: memory does not cap it.
        assert_eq!(slots_from(Some("6"), None, 12, Some(8 * GIB)), 6);
        assert_eq!(slots_from(None, Some(6), 12, Some(8 * GIB)), 6);
    }

    #[test]
    fn default_slots_follow_the_memory_too_one_per_8_gb() {
        // The machine that froze: 12 cores, 24 GB.
        assert_eq!(slots_from(None, None, 12, Some(24 * GIB)), 3);
        assert_eq!(slots_from(None, None, 12, Some(16 * GIB)), 2);
        assert_eq!(slots_from(None, None, 12, Some(8 * GIB)), 1);
        assert_eq!(
            slots_from(None, None, 12, Some(4 * GIB)),
            1,
            "never below one"
        );
        assert_eq!(
            slots_from(None, None, 12, Some(128 * GIB)),
            6,
            "the cores cap it then"
        );
        assert_eq!(
            slots_from(None, None, 4, Some(64 * GIB)),
            2,
            "few cores cap it"
        );
        assert_eq!(
            slots_from(None, None, 12, None),
            6,
            "unknown memory is ignored"
        );
        // Firmware hides some memory: a "16 GB" Linux box reports ~15.6 GiB and still gets 2.
        assert_eq!(slots_from(None, None, 12, Some(15 * GIB + GIB / 2)), 2);
        assert_eq!(slots_from(None, None, 12, Some(23 * GIB + GIB / 2)), 3);
        // 7 GiB is not an 8 GB machine's worth of anything more.
        assert_eq!(slots_from(None, None, 12, Some(7 * GIB)), 1);
    }

    #[test]
    fn the_load_gate_defaults_to_three_quarters_of_the_cores() {
        let g = |v: Option<&str>, c| Gates::from_inputs(v, None, None, None, c).max_load;
        assert_eq!(g(None, 12), Some(9.0));
        assert_eq!(g(None, 8), Some(6.0));
        assert_eq!(g(None, 1), Some(0.75));
        assert_eq!(g(None, 0), Some(0.75), "no cores reported counts as one");
        assert_eq!(
            g(Some("5.5"), 12),
            Some(5.5),
            "an explicit number is taken as is"
        );
        assert_eq!(g(Some(" 3 "), 12), Some(3.0));
        assert_eq!(g(Some("0"), 12), None, "0 turns it off");
        assert_eq!(g(Some("off"), 12), None, "garbage turns it off");
        assert_eq!(g(Some("-1"), 12), None);
    }

    #[test]
    fn the_memory_gate_defaults_to_20_percent_and_takes_env_then_config() {
        let m = |env: Option<&str>, cfg: Option<u8>| {
            Gates::from_inputs(None, env, cfg, None, 8).min_mem_pct
        };
        assert_eq!(m(None, None), Some(20));
        assert_eq!(m(Some("35"), None), Some(35));
        assert_eq!(m(None, Some(30)), Some(30), "config beats the default");
        assert_eq!(m(Some("35"), Some(30)), Some(35), "env beats config");
        assert_eq!(
            m(Some("0"), Some(30)),
            None,
            "0 turns it off even over a config"
        );
        assert_eq!(m(None, Some(0)), None);
        assert_eq!(m(Some("250"), None), Some(100), "above 100 counts as 100");
        assert_eq!(m(Some("x"), Some(30)), Some(30), "garbage is unset");
        assert_eq!(m(Some("-5"), None), Some(20), "garbage is unset");
        assert_eq!(m(None, Some(200)), Some(100));
    }

    #[test]
    fn the_wait_is_600_seconds_unless_set() {
        let w = |v: Option<&str>| Gates::from_inputs(None, None, None, v, 8).wait_s;
        assert_eq!(
            (w(None), w(Some("30")), w(Some("0")), w(Some("x"))),
            (600, 30, 0, 600)
        );
    }

    #[test]
    fn a_job_is_held_by_memory_pressure_or_load_and_only_by_readings_that_exist() {
        let g = Gates::from_inputs(None, None, None, None, 12); // load < 9.0, memory >= 20%
        assert_eq!(g.hold(Some(2.0), Some(60)), None, "all clear");
        assert_eq!(g.hold(Some(8.99), Some(20)), None, "both exactly inside");
        assert_eq!(
            g.hold(Some(2.0), Some(19)),
            Some(Hold::Memory {
                free_pct: 19,
                min_pct: 20
            }),
            "the freeze: memory low, CPU idle"
        );
        assert_eq!(
            g.hold(Some(9.0), Some(60)),
            Some(Hold::Load {
                load: 9.0,
                max: 9.0
            }),
            "load at the threshold holds"
        );
        assert_eq!(
            g.hold(Some(20.0), Some(5)),
            Some(Hold::Memory {
                free_pct: 5,
                min_pct: 20
            }),
            "memory is reported before load"
        );
        // No reading, no gate: Windows has no load average, an odd kernel may hide memory.
        assert_eq!(g.hold(None, Some(60)), None);
        assert_eq!(g.hold(Some(2.0), None), None);
        assert_eq!(g.hold(None, None), None);
        // Gates that are off never hold.
        let off = Gates::from_inputs(Some("0"), Some("0"), None, None, 12);
        assert_eq!(off.hold(Some(100.0), Some(0)), None);
        let mem_off = Gates::from_inputs(None, Some("0"), None, None, 12);
        assert_eq!(mem_off.hold(Some(2.0), Some(0)), None);
        assert!(matches!(
            mem_off.hold(Some(50.0), Some(0)),
            Some(Hold::Load { .. })
        ));
    }

    #[test]
    fn a_job_gets_a_share_of_the_cores_for_its_builds_unless_the_user_set_it() {
        let none = |_: &str| false;
        let env = |s, c| job_env(s, c, none);
        let v = |s, c| env(s, c).into_iter().map(|(_, v)| v).collect::<Vec<_>>();
        // 12 cores / 3 slots = 4 workers each.
        assert_eq!(
            env(3, 12),
            [
                ("CARGO_BUILD_JOBS", "4".to_string()),
                ("UV_THREADPOOL_SIZE", "4".to_string())
            ]
        );
        assert_eq!(v(1, 12), ["12", "12"], "a lone job may use everything");
        assert_eq!(v(6, 12), ["2", "2"]);
        assert_eq!(v(12, 12), ["2", "2"], "never below 2");
        assert_eq!(v(2, 2), ["2", "2"]);
        assert_eq!(v(0, 8), ["8", "8"], "zero slots counts as one");
        assert_eq!(v(5, 12), ["2", "2"], "integer division, floor of 2");
        // What the user set stays theirs; the other one is still filled in.
        let only_cargo = job_env(3, 12, |k| k == "CARGO_BUILD_JOBS");
        assert_eq!(only_cargo, [("UV_THREADPOOL_SIZE", "4".to_string())]);
        assert!(job_env(3, 12, |_| true).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_wrapped_command_stays_one_shell_word_whatever_it_contains() {
        // The wrapper is run by `sh -c`; the argument the CLI receives must be the original line.
        for cmd in ["bun run check", "a && b | c", "echo 'it'\"s\" $HOME", "x;y"] {
            let line = wrap_command("/bin/echo", "my label", cmd);
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&line)
                .output()
                .unwrap();
            let got = String::from_utf8(out.stdout).unwrap();
            assert_eq!(
                got.trim_end(),
                format!("run --heavy --label my label -- {cmd}"),
                "{line}"
            );
        }
    }

    #[test]
    fn config_file_slots_are_read_from_heavy_slots() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("config.json");
        assert_eq!(config_slots(&p), None);
        fs::write(&p, r#"{"heavy":{"slots":3},"other":1}"#).unwrap();
        assert_eq!(config_slots(&p), Some(3));
        fs::write(&p, "not json").unwrap();
        assert_eq!(config_slots(&p), None);
    }

    #[test]
    fn at_most_n_jobs_run_and_the_rest_wait_in_arrival_order() {
        let (_t, d) = dir();
        let mut jobs: Vec<Ticket> = (0..5).map(|i| job(&d, &format!("j{i}"))).collect();
        let started: Vec<bool> = jobs.iter_mut().map(|j| j.try_start(2).unwrap()).collect();
        assert_eq!(
            started,
            [true, true, false, false, false],
            "first two, in order"
        );
        let st = d.status(2);
        assert_eq!((st.running.len(), st.queued.len()), (2, 3));
        assert_eq!(
            st.queued
                .iter()
                .map(|e| e.job.label.as_str())
                .collect::<Vec<_>>(),
            ["j2", "j3", "j4"]
        );
        // A finishing job frees exactly one place, and it goes to the OLDEST waiter even when a
        // younger one asks first.
        let first = jobs.remove(0);
        first.finish(None);
        assert!(
            !jobs[3].try_start(2).unwrap(),
            "j4 must not overtake j2 and j3"
        );
        assert!(!jobs[2].try_start(2).unwrap(), "j3 must not overtake j2");
        assert!(jobs[1].try_start(2).unwrap(), "j2 is next");
        assert!(!jobs[2].try_start(2).unwrap(), "no slot left for j3");
    }

    #[test]
    fn a_dead_holder_frees_its_slot_at_once_with_no_stale_state() {
        // The holder is simulated by dropping its process-wide handles without running `Drop`'s
        // cleanup: the files stay behind exactly as after a SIGKILL, only the locks are released.
        let (_t, d) = dir();
        let mut a = job(&d, "a");
        assert!(a.try_start(1).unwrap());
        let mut b = job(&d, "b");
        assert!(!b.try_start(1).unwrap(), "the only slot is taken");
        let dead = a.job.clone();
        a.killed = true;
        drop(a);
        assert!(
            d.jobs().join(format!("{}.json", dead.id)).exists(),
            "record left behind"
        );
        assert!(b.try_start(1).unwrap(), "b gets the slot immediately");
        let live = d.jobs_live();
        assert_eq!(live.len(), 1, "the dead record was swept: {live:?}");
        assert_eq!(live[0].label, "b");
        assert!(!d.jobs().join(format!("{}.json", dead.id)).exists());
    }

    #[test]
    fn a_dead_queued_job_does_not_block_the_jobs_behind_it() {
        let (_t, d) = dir();
        let mut front = job(&d, "front");
        let mut behind = job(&d, "behind");
        assert!(front.try_start(1).unwrap());
        // `queued_dead` is older than `behind` and never starts: it dies waiting.
        let queued_dead = job(&d, "queued-dead");
        let id = queued_dead.job.id.clone();
        let mut queued_dead = queued_dead;
        queued_dead.killed = true;
        drop(queued_dead);
        front.finish(None);
        assert!(
            behind.try_start(1).unwrap(),
            "{id} is dead and must not hold the line"
        );
    }

    // A job ends by removing its record and then its lock; a status call in between must not
    // bring the lock file back (it would stay forever, one orphan per racing status call).
    #[test]
    fn sweeping_a_record_without_a_lock_leaves_no_lock_file_behind() {
        let (_t, d) = dir();
        let t = job(&d, "a");
        let (json, lock) = (
            d.jobs().join(format!("{}.json", t.job.id)),
            d.jobs().join(format!("{}.lock", t.job.id)),
        );
        // The lock file vanished while the record is still there (a finishing job, half way).
        fs::remove_file(&lock).unwrap();
        assert!(
            d.jobs_live().is_empty(),
            "a record without a lock is a dead job"
        );
        assert!(!json.exists(), "its record is swept");
        assert!(
            !lock.exists(),
            "and the lock file is not re-created by the sweep"
        );
    }

    #[test]
    fn dropping_a_ticket_removes_its_record_and_frees_the_slot() {
        let (_t, d) = dir();
        let mut a = job(&d, "a");
        assert!(a.try_start(1).unwrap());
        assert_eq!(d.jobs_live().len(), 1);
        drop(a);
        assert!(d.jobs_live().is_empty());
        let mut b = job(&d, "b");
        assert!(b.try_start(1).unwrap());
    }

    #[test]
    fn records_carry_what_status_needs_and_serialize_with_the_keys_the_tools_read() {
        let (_t, d) = dir();
        let mut a = d
            .enqueue("gate", "bun run check", Path::new("/work/x"), "atlas")
            .unwrap();
        let mut b = job(&d, "other");
        assert!(a.try_start(1).unwrap());
        assert!(!b.try_start(1).unwrap());
        let v = serde_json::to_value(d.status(1)).unwrap();
        let r = &v["running"][0];
        assert_eq!(r["label"], "gate");
        assert_eq!(r["state"], "running");
        assert_eq!(r["cwd"], "/work/x");
        assert_eq!(r["project"], "atlas");
        assert_eq!(r["command"], "bun run check");
        assert_eq!(r["pid"], std::process::id());
        assert!(r["queued_at"].as_u64().unwrap() <= r["started_at"].as_u64().unwrap());
        assert_eq!(r["slot"], 0);
        assert_eq!(v["queued"][0]["state"], "queued");
        assert!(v["queued"][0]["started_at"].is_null());
        assert_eq!(v["slots"], 1);
    }

    #[test]
    fn eta_comes_from_the_rolling_average_of_the_label_and_chains_through_the_queue() {
        let (_t, d) = dir();
        fs::create_dir_all(d.root()).unwrap();
        for s in [100, 200, 300] {
            d.record_run("gate", s);
        }
        let mut a = job(&d, "gate");
        assert!(a.try_start(1).unwrap());
        let _b = job(&d, "gate");
        let _c = job(&d, "gate");
        let _unknown = job(&d, "never-seen");
        let st = d.status(1);
        assert_eq!(st.running[0].avg_s, Some(200));
        let eta = st.running[0].eta_s.unwrap();
        assert!(
            (198..=200).contains(&eta),
            "running job ends ~200 s after its start: {eta}"
        );
        let q: Vec<_> = st.queued.iter().map(|e| e.eta_s).collect();
        assert!(
            (198..=200).contains(&q[0].unwrap()),
            "first waiter starts when the holder ends"
        );
        assert!(
            (398..=400).contains(&q[1].unwrap()),
            "second waits for the first as well: {q:?}"
        );
        assert_eq!(
            q[2],
            Some(q[1].unwrap() + 200),
            "third waits for the second"
        );
        // Nothing is known about the unseen label: its own average is None, and it adds nothing.
        assert_eq!(st.queued[2].avg_s, None);
    }

    #[test]
    fn history_keeps_only_the_last_runs_per_label() {
        let (_t, d) = dir();
        fs::create_dir_all(d.root()).unwrap();
        for s in 1..=20 {
            d.record_run("a", s);
        }
        d.record_run("b", 5);
        let h = d.history();
        assert_eq!(
            h.runs["a"],
            (13..=20).collect::<Vec<u64>>(),
            "rolling window of {HISTORY_KEEP}"
        );
        assert_eq!(h.runs["b"], [5]);
        assert_eq!(average(&h.runs["a"]), Some(16));
        assert_eq!(average(&[]), None);
    }

    #[test]
    fn a_free_directory_with_nothing_in_it_is_an_empty_status() {
        let (_t, d) = dir();
        let st = d.status(3);
        assert_eq!((st.slots, st.running.len(), st.queued.len()), (3, 0, 0));
    }
}
