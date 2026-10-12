//! `atlas deploy` — local controller. Builds and uploads, then launches the
//! driver (driver.sh) on the host detached from ssh and only tails its log:
//! the host owns the transaction, so Ctrl-C / CI cancel / a dropped
//! connection never leave `current` unverified. See docs/deploy.md.

mod package;
mod plan;
mod remote;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use atlas_core::util::expand_tilde;
use atlas_tui::config::{DeployConfig, DeployKind, DeployTarget};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use super::AtlasCommand;
use plan::{Plan, render_plan};
use remote::{Remote, shq};

const DRIVER: &str = include_str!("driver.sh");
/// Give up following after this long without a successful poll.
const LOST_CONTACT: Duration = Duration::from_secs(300);

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DeployArgs {
    /// Target from `deploy.targets` (optional when there is exactly one).
    pub target: Option<String>,
    #[arg(short, long, default_value = ".", global = true)]
    pub dir: String,
    /// Build and render the plan; touch nothing remote.
    #[arg(long)]
    pub dry_run: bool,
    /// Print one JSON result object on stdout (host log goes to stderr).
    #[arg(long)]
    pub json: bool,
    /// Deploy with uncommitted changes (release id gets a `-dirty` suffix).
    #[arg(long)]
    pub allow_dirty: bool,
    /// Do not run `deploy.build`.
    #[arg(long)]
    pub skip_build: bool,
    /// Redeploy even if HEAD is already current and healthy.
    #[arg(long)]
    pub force: bool,
    /// Seconds to wait for another deploy's lock (default: fail at once).
    #[arg(long, default_value_t = 0, value_name = "SECS", global = true)]
    pub lock_wait: u64,
    /// Exit once the driver is running on the host; prints the release id.
    #[arg(long)]
    pub no_follow: bool,
    #[command(subcommand)]
    pub action: Option<DeployAction>,
}

#[derive(Debug, Subcommand)]
pub enum DeployAction {
    /// Swap back to the previous healthy release (or --to <id>) and gate every service
    Rollback {
        target: Option<String>,
        /// Release id to roll back to (default: newest healthy before current).
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show current release, releases, lock holder, last phase and service health
    Status {
        target: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print (or follow) the driver log of a release (default: newest run)
    Logs {
        target: Option<String>,
        release: Option<String>,
        #[arg(short, long)]
        follow: bool,
    },
    /// Show the deploy audit trail
    History {
        target: Option<String>,
        #[arg(short = 'n', default_value_t = 20)]
        n: usize,
        #[arg(long)]
        json: bool,
    },
}

/// Resolved project + target.
struct Ctx {
    root: PathBuf,
    cfg: DeployConfig,
    app: String,
    name: String,
    target: DeployTarget,
    dir: String,
    remote: Remote,
}

impl Ctx {
    fn load(dir: &str, target: Option<&str>) -> Result<Self> {
        let start = PathBuf::from(expand_tilde(dir));
        let root = atlas_tui::config::find_root(&start)
            .with_context(|| format!("no atlas.yaml found in {} or any parent", start.display()))?;
        let project = atlas_tui::config::load(&root)?;
        let cfg = project.deploy.context("no deploy: section in atlas.yaml")?;
        let mut known: Vec<&String> = cfg.targets.keys().collect();
        known.sort();
        let known_s = known
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let name = match target {
            Some(t) if cfg.targets.contains_key(t) => t.to_string(),
            Some(t) => bail!("unknown deploy target '{t}' (known: {known_s})"),
            None if known.len() == 1 => known[0].clone(),
            None => bail!("multiple deploy targets ({known_s}): pass one"),
        };
        let app = cfg.app.clone().unwrap_or(project.name);
        if app.is_empty()
            || !app
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            bail!("project name '{app}' is not [a-z0-9-]+: set deploy.app");
        }
        let target = cfg.targets[&name].clone();
        let dir = target.dir.clone().unwrap_or_else(|| format!("/opt/{app}"));
        let remote = Remote::new(&target.host);
        Ok(Self {
            root,
            cfg,
            app,
            name,
            target,
            dir,
            remote,
        })
    }

    fn atlas(&self) -> String {
        format!("{}/.atlas", self.dir)
    }

    fn run_dir(&self, id: &str) -> String {
        format!("{}/.atlas/run/{id}", self.dir)
    }
}

fn step(msg: &str) {
    eprintln!("\x1b[36m▸\x1b[0m {msg}");
}

fn deploy_user() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    let host = std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    format!("{user}@{host}")
}

/// `YYYYMMDDHHMMSS-<sha7>[-dirty]` — the only shape ever put into a path.
pub fn valid_release_id(s: &str) -> bool {
    let b = s.as_bytes();
    let core = b.len() >= 22
        && b[..14].iter().all(u8::is_ascii_digit)
        && b[14] == b'-'
        && b[15..22]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c));
    core && (b.len() == 22 || &s[22..] == "-dirty")
}

fn valid_run_id(s: &str) -> bool {
    valid_release_id(s)
        || s.strip_prefix("rb-")
            .is_some_and(|t| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit()))
}

fn die(code: i32, msg: &str) -> ! {
    eprintln!("\x1b[1;31merror:\x1b[0m {msg}");
    std::process::exit(code);
}

impl AtlasCommand for DeployArgs {
    async fn run(self) -> Result<()> {
        match self.action {
            None => deploy(&self).await,
            Some(DeployAction::Rollback {
                ref target,
                ref to,
                json,
            }) => {
                let ctx = Ctx::load(&self.dir, target.as_deref())?;
                if let Some(id) = to
                    && !valid_release_id(id)
                {
                    bail!("invalid release id: {id}");
                }
                let run_id = format!("rb-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"));
                let plan = Plan {
                    app: &ctx.app,
                    target_name: &ctx.name,
                    target: &ctx.target,
                    dir: &ctx.dir,
                    release: "",
                    sha: "",
                    sha256: "",
                    user: &deploy_user(),
                    rollback: true,
                    rollback_to: to.as_deref().unwrap_or(""),
                    force: false,
                    lock_wait: self.lock_wait,
                };
                let opts = RunOpts {
                    json,
                    no_follow: self.no_follow,
                    release: "",
                    sha: "",
                };
                launch_and_follow(&ctx, &run_id, &render_plan(&plan), None, &opts).await
            }
            Some(DeployAction::Status { ref target, json }) => {
                status(&Ctx::load(&self.dir, target.as_deref())?, json).await
            }
            Some(DeployAction::Logs {
                ref target,
                ref release,
                follow,
            }) => {
                logs(
                    &Ctx::load(&self.dir, target.as_deref())?,
                    release.as_deref(),
                    follow,
                )
                .await
            }
            Some(DeployAction::History {
                ref target,
                n,
                json,
            }) => history(&Ctx::load(&self.dir, target.as_deref())?, n, json).await,
        }
    }
}

async fn deploy(a: &DeployArgs) -> Result<()> {
    let ctx = Ctx::load(&a.dir, a.target.as_deref())?;
    step(&format!("package ({})", ctx.root.display()));
    let rel = package::package(&ctx.root, &ctx.cfg, a.allow_dirty, a.skip_build)?;
    let plan = render_plan(&Plan {
        app: &ctx.app,
        target_name: &ctx.name,
        target: &ctx.target,
        dir: &ctx.dir,
        release: &rel.id,
        sha: &rel.sha,
        sha256: &rel.sha256,
        user: &deploy_user(),
        rollback: false,
        rollback_to: "",
        force: a.force,
        lock_wait: a.lock_wait,
    });
    if a.dry_run {
        println!("target:  {} ({})", ctx.name, ctx.target.host);
        println!("release: {}", rel.id);
        println!("sha256:  {}", rel.sha256);
        println!("driver:  {}/driver.sh", ctx.run_dir(&rel.id));
        println!("--- plan.sh ---\n{plan}");
        return Ok(());
    }
    let opts = RunOpts {
        json: a.json,
        no_follow: a.no_follow,
        release: &rel.id,
        sha: &rel.sha,
    };
    launch_and_follow(&ctx, &rel.id, &plan, Some(&rel), &opts).await
}

struct RunOpts<'a> {
    json: bool,
    no_follow: bool,
    release: &'a str,
    sha: &'a str,
}

async fn launch_and_follow(
    ctx: &Ctx,
    run_id: &str,
    plan: &str,
    artifact: Option<&package::Release>,
    opts: &RunOpts<'_>,
) -> Result<()> {
    let started = Instant::now();
    let host = &ctx.target.host;
    let run = ctx.run_dir(run_id);

    step(&format!("preflight {host}:{}", ctx.dir));
    let pre = format!(
        "mkdir -p {r} {run} || exit 10; command -v flock >/dev/null || exit 11",
        r = shq(&format!("{}/releases", ctx.dir)),
        run = shq(&run),
    );
    let out = match ctx.remote.exec(&pre).await {
        Ok(o) => o,
        Err(e) => die(4, &format!("cannot reach {host}: {e}")),
    };
    match out.code {
        0 => {}
        11 => bail!("flock not found on {host}"),
        10 => bail!("cannot create {} on {host}: {}", ctx.dir, out.stderr.trim()),
        _ => die(4, &format!("cannot reach {host}: {}", out.stderr.trim())),
    }

    if let Some(rel) = artifact {
        step(&format!(
            "upload {} ({} bytes)",
            rel.id,
            std::fs::metadata(&rel.tarball)?.len()
        ));
        let partial = format!("{}/releases/.{}.partial", ctx.dir, rel.id);
        ctx.remote.upload(&rel.tarball, &partial).await?;
    }
    for (name, body) in [("driver.sh", DRIVER), ("plan.sh", plan)] {
        let w = format!("cat > {}", shq(&format!("{run}/{name}")));
        let out = ctx.remote.exec_stdin(&w, Some(body.as_bytes())).await?;
        if out.code != 0 {
            bail!("writing {name} on {host}: {}", out.stderr.trim());
        }
    }

    step("launch driver");
    let inner = format!(
        "exec bash {d} >>{l} 2>&1",
        d = shq(&format!("{run}/driver.sh")),
        l = shq(&format!("{run}/log"))
    );
    let unit = format!("atlas-deploy-{}-{run_id}", ctx.app);
    let launch = format!(
        "if [ -d /run/systemd/system ] && [ \"$(id -u)\" = 0 ] && command -v systemd-run >/dev/null; then \
           systemd-run --unit={unit} --collect --quiet --property=KillMode=process bash -c {inner}; \
         else \
           S=; command -v setsid >/dev/null && S=setsid; \
           $S nohup bash -c {inner} </dev/null >/dev/null 2>&1 & \
         fi",
        unit = shq(&unit),
        inner = shq(&inner),
    );
    let out = ctx.remote.exec_once(&launch).await?;
    if out.code != 0 {
        bail!("launching driver on {host} failed: {}", out.stderr.trim());
    }
    if opts.no_follow {
        println!("{run_id}");
        step(&format!(
            "driver running on {host}. Follow: atlas deploy logs -f {}",
            ctx.name
        ));
        return Ok(());
    }

    let code = tokio::select! {
        c = follow(ctx, &run, opts.json) => c?,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\ndetached; deploy continues on host. Follow: atlas deploy logs -f {}", ctx.name);
            std::process::exit(130);
        }
    };

    let st = read_state(ctx).await.unwrap_or(Value::Null);
    let s = |k: &str| st.get(k).and_then(Value::as_str).filter(|v| !v.is_empty());
    let phase = if code == 3 {
        "locked"
    } else {
        s("phase").unwrap_or("unknown")
    };
    if opts.json {
        let o = json!({
            "ok": code == 0,
            "target": ctx.name,
            "host": host,
            "release": s("release").unwrap_or(opts.release),
            "sha": opts.sha,
            "previous": s("previous"),
            "phase": phase,
            "rolled_back": phase == "rolled_back",
            "reason": if code == 3 { Some("locked") } else { s("reason") },
            "duration_s": started.elapsed().as_secs(),
        });
        println!("{}", serde_json::to_string_pretty(&o)?);
    } else {
        let msg = match code {
            0 => format!("✓ {phase} {}", s("release").unwrap_or("")),
            1 => format!("✗ {phase}: {}", s("reason").unwrap_or("")),
            3 => "✗ locked by another deploy".into(),
            _ => format!("✗ CRITICAL {phase}: {}", s("reason").unwrap_or("")),
        };
        eprintln!("{msg}");
    }
    std::process::exit(code);
}

/// Tail `<run>/log` until `<run>/exit` appears; returns the driver's exit code.
async fn follow(ctx: &Ctx, run: &str, json: bool) -> Result<i32> {
    use std::io::Write;
    let host = &ctx.target.host;
    let mut offset: u64 = 0;
    let mut pending = String::new();
    let mut last_ok = Instant::now();
    let script = |off: u64| {
        format!(
            "tail -c +{} {l} 2>/dev/null; printf '\\0'; cat {e} 2>/dev/null; true",
            off + 1,
            l = shq(&format!("{run}/log")),
            e = shq(&format!("{run}/exit")),
        )
    };
    loop {
        match ctx.remote.exec(&script(offset)).await {
            Ok(out) if out.code == 0 => {
                last_ok = Instant::now();
                let (chunk, exit) = out.stdout.split_once('\0').unwrap_or((&out.stdout, ""));
                offset += chunk.len() as u64;
                pending.push_str(chunk);
                let mut sink: Box<dyn Write> = if json {
                    Box::new(std::io::stderr())
                } else {
                    Box::new(std::io::stdout())
                };
                while let Some(i) = pending.find('\n') {
                    let line: String = pending.drain(..=i).collect();
                    let _ = write!(sink, "[{host}] {line}");
                }
                if let Ok(code) = exit.trim().parse::<i32>() {
                    if !pending.is_empty() {
                        let _ = writeln!(sink, "[{host}] {pending}");
                    }
                    return Ok(code);
                }
            }
            _ if last_ok.elapsed() > LOST_CONTACT => die(
                4,
                &format!(
                    "lost contact; deploy continues on host — atlas deploy status {}",
                    ctx.name
                ),
            ),
            _ => {}
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn read_state(ctx: &Ctx) -> Result<Value> {
    let out = ctx
        .remote
        .exec(&format!("cat {}/state.json", shq(&ctx.atlas())))
        .await?;
    Ok(serde_json::from_str(out.stdout.trim())?)
}

const TERMINAL: [&str; 4] = ["healthy", "noop", "rolled_back", "failed"];

async fn status(ctx: &Ctx, json_out: bool) -> Result<()> {
    let mut svc = String::new();
    for s in &ctx.target.services {
        let kind = match s.kind {
            DeployKind::Systemd => "systemd",
            DeployKind::Compose => "compose",
            DeployKind::Command => "command",
        };
        svc.push_str(&format!(
            "svc {} {} {} {}\n",
            shq(&s.name),
            kind,
            shq(s.health.as_deref().unwrap_or("")),
            shq(s.compose_file.as_deref().unwrap_or("compose.yml")),
        ));
    }
    let script = format!(
        r#"D={dir}; A={app}
cd "$D" 2>/dev/null || {{ echo "NODIR"; exit 0; }}
[ -L current ] && echo "CURRENT $(basename "$(readlink current)")"
for r in $(ls -1 releases 2>/dev/null | sort -r); do
  m=releases/$r/.atlas-release
  sha=$(sed -n 's/.*"sha":"\([^"]*\)".*/\1/p' "$m" 2>/dev/null); st=$(sed -n 's/.*"status":"\([^"]*\)".*/\1/p' "$m" 2>/dev/null)
  echo "REL $r ${{sha:--}} ${{st:-unknown}}"
done
if [ -e .atlas/lock ] && command -v flock >/dev/null && ! flock -n .atlas/lock true; then echo "LOCK $(cat .atlas/lock.owner 2>/dev/null)"; fi
[ -f .atlas/state.json ] && echo "STATE $(cat .atlas/state.json)"
svc() {{
  n=$1 k=$2 h=$3 c=$4 a=0 ok=0
  case $k in
    systemd) systemctl is-active --quiet "$n" && a=1 ;;
    compose) [ -n "$(docker compose -p "$A" -f "$D/current/$c" ps -q --status running "$n" 2>/dev/null)" ] && a=1 ;;
    command) a=1 ;;
  esac
  if [ -n "$h" ]; then curl -fsS -o /dev/null --max-time 5 "$h" 2>/dev/null && ok=1; else ok=$a; fi
  echo "SVC $n $a $ok"
}}
{svc}"#,
        dir = shq(&ctx.dir),
        app = shq(&ctx.app),
    );
    let out = ctx.remote.exec(&script).await?;
    if out.code == 255 {
        die(
            4,
            &format!("cannot reach {}: {}", ctx.target.host, out.stderr.trim()),
        );
    }
    let (mut current, mut lock, mut state) = (None, None, Value::Null);
    let (mut releases, mut services) = (vec![], vec![]);
    for line in out.stdout.lines() {
        let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
        let f: Vec<&str> = rest.split(' ').collect();
        match tag {
            "CURRENT" => current = Some(rest.to_string()),
            "REL" if f.len() == 3 => {
                releases.push(json!({"id": f[0], "sha": f[1], "status": f[2]}))
            }
            "LOCK" => lock = Some(rest.to_string()),
            "STATE" => state = serde_json::from_str(rest).unwrap_or(Value::Null),
            "SVC" if f.len() == 3 => {
                services.push(json!({"name": f[0], "active": f[1] == "1", "healthy": f[2] == "1"}))
            }
            _ => {}
        }
    }
    let phase = state.get("phase").and_then(Value::as_str).unwrap_or("");
    let terminal = phase.is_empty() || TERMINAL.contains(&phase);
    let healthy = services.iter().all(|s| s["healthy"] == true);
    let ok = current.is_some() && terminal && healthy && lock.is_none();
    if json_out {
        let o = json!({"ok": ok, "current": current, "releases": releases, "lock": lock,
            "phase": phase, "services": services});
        println!("{}", serde_json::to_string_pretty(&o)?);
    } else {
        println!("target   {} ({}:{})", ctx.name, ctx.target.host, ctx.dir);
        println!("current  {}", current.as_deref().unwrap_or("-"));
        let mut p = format!("phase    {}", if phase.is_empty() { "-" } else { phase });
        if let Some(r) = state
            .get("reason")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty())
        {
            p.push_str(&format!(" ({r})"));
        }
        if !terminal && lock.is_none() {
            p.push_str("  \x1b[1;31mINTERRUPTED\x1b[0m — next deploy will recover");
        }
        println!("{p}");
        println!("lock     {}", lock.as_deref().unwrap_or("free"));
        println!("releases");
        for r in &releases {
            let mark = if Some(r["id"].as_str().unwrap_or("")) == current.as_deref() {
                "*"
            } else {
                " "
            };
            println!(
                "  {mark} {}  {:.7}  {}",
                r["id"].as_str().unwrap_or(""),
                r["sha"].as_str().unwrap_or(""),
                r["status"].as_str().unwrap_or("")
            );
        }
        println!("services");
        for s in &services {
            let icon = if s["healthy"] == true {
                "\x1b[32m✓\x1b[0m"
            } else {
                "\x1b[31m✗\x1b[0m"
            };
            println!(
                "  {icon} {} (active: {})",
                s["name"].as_str().unwrap_or(""),
                s["active"]
            );
        }
    }
    std::process::exit(if ok { 0 } else { 1 });
}

async fn logs(ctx: &Ctx, release: Option<&str>, follow: bool) -> Result<()> {
    let pick = match release {
        Some(r) if valid_run_id(r) => shq(r),
        Some(r) => bail!("invalid release id: {r}"),
        None => "\"$(ls -1t \"$R\" 2>/dev/null | head -n1)\"".into(),
    };
    let cmd = if follow { "tail -n +1 -F" } else { "cat" };
    let script = format!("R={}/run; {cmd} \"$R\"/{pick}/log", shq(&ctx.atlas()));
    let code = ctx.remote.stream(&script).await?;
    std::process::exit(code);
}

async fn history(ctx: &Ctx, n: usize, json_out: bool) -> Result<()> {
    let out = ctx
        .remote
        .exec(&format!(
            "tail -n {n} {}/deploys.jsonl 2>/dev/null; true",
            shq(&ctx.atlas())
        ))
        .await?;
    if json_out {
        print!("{}", out.stdout);
        return Ok(());
    }
    println!(
        "{:<20}  {:<24}  {:<8}  {:<11}  {:>5}  REASON",
        "FINISHED", "RELEASE", "MODE", "RESULT", "SECS"
    );
    for line in out.stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let g = |k: &str| {
            v.get(k)
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| x.to_string())
                })
                .unwrap_or_default()
        };
        println!(
            "{:<20}  {:<24}  {:<8}  {:<11}  {:>5}  {}",
            g("finished"),
            g("release"),
            g("mode"),
            g("result"),
            g("duration_s"),
            g("reason")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_id_shape() {
        assert!(valid_release_id("20250101120000-abcdef0"));
        assert!(valid_release_id("20250101120000-abcdef0-dirty"));
        for bad in [
            "../x",
            "20250101120000-abcdef",
            "20250101120000-ABCDEF0",
            "20250101120000-abcdef0/..",
            "20250101120000-abcdef0-dirtyx",
            "",
        ] {
            assert!(!valid_release_id(bad), "{bad}");
        }
        assert!(valid_run_id("rb-20250101120000"));
        assert!(!valid_run_id("rb-../x"));
    }
}
