//! `atlas deploy` — local controller. Builds and uploads, then launches the
//! driver (driver.sh) on the host detached from ssh and only tails its log:
//! the host owns the transaction, so Ctrl-C / CI cancel / a dropped
//! connection never leave `current` unverified. See docs/deploy.md.
//!
//! * `ctx`     — resolved project/target + shared helpers
//! * `package` — local build and tarball
//! * `plan`    — the `plan.sh` handed to the host driver
//! * `remote`  — ssh transport
//! * `launch`  — upload, start the driver, follow and report
//! * `inspect` — `status` / `logs` / `history`

mod ctx;
mod inspect;
mod launch;
mod package;
mod plan;
pub(crate) mod remote;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};

use super::AtlasCommand;
use ctx::{Ctx, deploy_user, step, valid_release_id};
use launch::{RunOpts, launch_and_follow};
use plan::{Plan, render_plan};

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
                rollback(&ctx, to.as_deref(), json, self.no_follow, self.lock_wait).await
            }
            Some(DeployAction::Status { ref target, json }) => {
                inspect::status(&Ctx::load(&self.dir, target.as_deref())?, json).await
            }
            Some(DeployAction::Logs {
                ref target,
                ref release,
                follow,
            }) => {
                inspect::logs(
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
            }) => inspect::history(&Ctx::load(&self.dir, target.as_deref())?, n, json).await,
        }
    }
}

async fn rollback(
    ctx: &Ctx,
    to: Option<&str>,
    json: bool,
    no_follow: bool,
    lock_wait: u64,
) -> Result<()> {
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
        rollback_to: to.unwrap_or(""),
        force: false,
        lock_wait,
    };
    let opts = RunOpts {
        json,
        no_follow,
        release: "",
        sha: "",
    };
    launch_and_follow(ctx, &run_id, &render_plan(&plan), None, &opts).await
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
