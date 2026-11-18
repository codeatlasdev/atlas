//! Execution of one deploy/rollback run: preflight, upload, start the detached
//! driver on the host, then follow its log and report the result.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use atlas_i18n::t;
use serde_json::{Value, json};

use super::ctx::{Ctx, die, step};
use super::package;
use super::remote::shq;
use super::words::phase_label;

const DRIVER: &str = include_str!("driver.sh");
/// Give up following after this long without a successful poll.
const LOST_CONTACT: Duration = Duration::from_secs(300);

pub(super) struct RunOpts<'a> {
    pub json: bool,
    pub no_follow: bool,
    pub release: &'a str,
    pub sha: &'a str,
}

pub(super) async fn launch_and_follow(
    ctx: &Ctx,
    run_id: &str,
    plan: &str,
    artifact: Option<&package::Release>,
    opts: &RunOpts<'_>,
) -> Result<()> {
    let started = Instant::now();
    let host = &ctx.target.host;
    let run = ctx.run_dir(run_id);

    preflight(ctx, &run).await?;
    if let Some(rel) = artifact {
        step(&t!(
            "cmd_deploy_step_upload",
            id = &rel.id,
            bytes = std::fs::metadata(&rel.tarball)?.len().to_string()
        ));
        let partial = format!("{}/releases/.{}.partial", ctx.dir, rel.id);
        ctx.remote.upload(&rel.tarball, &partial).await?;
    }
    for (name, body) in [("driver.sh", DRIVER), ("plan.sh", plan)] {
        let w = format!("cat > {}", shq(&format!("{run}/{name}")));
        let out = ctx.remote.exec_stdin(&w, Some(body.as_bytes())).await?;
        if out.code != 0 {
            bail!(t!(
                "cmd_deploy_launch_write_failed",
                name = name,
                host = host,
                error = out.stderr.trim()
            ));
        }
    }
    launch_driver(ctx, run_id, &run).await?;
    if opts.no_follow {
        println!("{run_id}");
        step(&t!(
            "cmd_deploy_step_driver_running",
            host = host,
            target = &ctx.name
        ));
        return Ok(());
    }

    let code = tokio::select! {
        c = follow(ctx, &run, opts.json) => c?,
        _ = tokio::signal::ctrl_c() => {
            eprintln!(
                "\n{}",
                t!("cmd_deploy_launch_detached", target = &ctx.name)
            );
            std::process::exit(130);
        }
    };
    report(ctx, code, started, opts).await?;
    std::process::exit(code);
}

/// Reach the host, create the release/run dirs and check `flock` exists.
async fn preflight(ctx: &Ctx, run: &str) -> Result<()> {
    let host = &ctx.target.host;
    step(&t!(
        "cmd_deploy_step_preflight",
        host = host,
        dir = &ctx.dir
    ));
    let pre = format!(
        "mkdir -p {r} {run} || exit 10; command -v flock >/dev/null || exit 11",
        r = shq(&format!("{}/releases", ctx.dir)),
        run = shq(run),
    );
    let out = match ctx.remote.exec(&pre).await {
        Ok(o) => o,
        Err(e) => die(
            4,
            &t!(
                "cmd_deploy_launch_unreachable",
                host = host,
                error = e.to_string()
            ),
        ),
    };
    match out.code {
        0 => Ok(()),
        11 => bail!(t!("cmd_deploy_launch_no_flock", host = host)),
        10 => bail!(t!(
            "cmd_deploy_launch_mkdir_failed",
            dir = &ctx.dir,
            host = host,
            error = out.stderr.trim()
        )),
        _ => die(
            4,
            &t!(
                "cmd_deploy_launch_unreachable",
                host = host,
                error = out.stderr.trim()
            ),
        ),
    }
}

/// Start `driver.sh` detached from the ssh session (systemd-run as root, else setsid+nohup).
async fn launch_driver(ctx: &Ctx, run_id: &str, run: &str) -> Result<()> {
    step(&t!("cmd_deploy_step_launch"));
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
        bail!(t!(
            "cmd_deploy_launch_driver_failed",
            host = &ctx.target.host,
            error = out.stderr.trim()
        ));
    }
    Ok(())
}

/// Print the final result (JSON on stdout or a one-line verdict on stderr).
async fn report(ctx: &Ctx, code: i32, started: Instant, opts: &RunOpts<'_>) -> Result<()> {
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
            "host": ctx.target.host,
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
            0 => format!(
                "✓ {}",
                t!(
                    "cmd_deploy_report_ok",
                    phase = phase_label(phase),
                    release = s("release").unwrap_or("")
                )
            ),
            1 => format!(
                "✗ {}",
                t!(
                    "cmd_deploy_report_failed",
                    phase = phase_label(phase),
                    reason = s("reason").unwrap_or("")
                )
            ),
            3 => format!("✗ {}", t!("cmd_deploy_report_locked")),
            _ => format!(
                "✗ {}",
                t!(
                    "cmd_deploy_report_critical",
                    phase = phase_label(phase),
                    reason = s("reason").unwrap_or("")
                )
            ),
        };
        eprintln!("{msg}");
    }
    Ok(())
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
                &t!(
                    "cmd_deploy_launch_lost_contact",
                    host = host,
                    target = &ctx.name
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
