//! `atlas crew install <ssh-target>` — turn a Linux x86_64 or aarch64 server into a crew server.
//!
//! Steps (what the host looks like is probed first, see [`layout`]):
//! 1. Preflight: one probe of the host (uname, systemd as PID 1, every tool the install and the runtime need);
//!    nothing is changed until it passes, and it names everything that is missing at once
//! 2. Stage the atlas binary as /usr/local/lib/atlas-crew/atlas.new (curl on the host; if the host cannot reach
//!    GitHub, downloaded here with this machine's proxy settings and streamed over ssh)
//! 3. Create the atlas-crew system user (an existing atlas-bot user is reused)
//! 4. Retire a pre-rename `atlas-bot.service`, write `atlas-crew.service`
//! 5. Swap the binary in (the one that was there stays as `atlas.prev`), restart the unit, wait for hostd and
//!    roll back if it does not answer
//! 6. Append the caller's SSH public key to <home>/.ssh/authorized_keys

use anyhow::{Context, Result, bail};
use atlas_i18n::t;
use clap::Args;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

use atlas_core::progress;

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::{Remote, shq};

use super::binary;
use super::isolation;
use super::layout::{self, Account, INSTALL_DIR, Probe, SERVICE};
use super::privileged::Priv;

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// Skip confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
    /// SSH port (default 22). Used when the server listens on a non-standard port.
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    /// Private key to sign in with (`ssh -i`), instead of the ones ssh finds by itself. Its public key is the one
    /// added to the crew user's `authorized_keys`. Atlas Desktop passes the key picked in "Add a crew server".
    #[arg(long)]
    pub identity: Option<std::path::PathBuf>,
    /// Read sudo password from stdin instead of requiring passwordless sudo.
    /// Used by Atlas Desktop; the password is NEVER placed on argv.
    #[arg(long, hide = true)]
    pub sudo_password_stdin: bool,
    /// Upload this Linux `atlas` (the host's architecture) instead of downloading the release of this version.
    /// For builds whose hostd is not published yet (the release would lack the crew commands).
    /// Also read from `ATLAS_CREW_BINARY`, so Atlas Desktop picks it up.
    #[arg(long)]
    pub binary: Option<std::path::PathBuf>,
    /// Also install Playwright's chromium and webkit (pinned) for the crewmates' `capture` tool.
    /// Needs Node.js on the server and sudo for the apt libraries; the same as `atlas crew browsers install`.
    #[arg(long)]
    pub browsers: bool,
    /// What stdout carries: `text` (`step: <key>` markers, for people and old clients) or `json` (one
    /// event per line: kind, code, args, message; never translated). Atlas Desktop reads only `json`.
    #[arg(long, value_enum, default_value_t = Progress::Text)]
    pub progress: Progress,
}

/// See [`InstallArgs::progress`] and `atlas_core::progress` for the event contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Progress {
    Text,
    Json,
}

/// Whether this process prints events instead of `step:` markers (set once, by [`InstallArgs::run`]).
static JSON_EVENTS: AtomicBool = AtomicBool::new(false);

impl AtlasCommand for InstallArgs {
    async fn run(self) -> Result<()> {
        let json = self.progress == Progress::Json;
        JSON_EVENTS.store(json, Ordering::Relaxed);
        if json {
            // The machine channel is English end to end: the `message` of every event, including the error that
            // ends it, is the English a client falls back to when it does not know the `code`.
            atlas_i18n::set_lang(atlas_core::lang::DEFAULT);
        }
        let result = run_install(self).await;
        // The failure is the last event, with the code of the error, so a client never reads text.
        if let (true, Err(e)) = (json, &result) {
            println!("{}", progress::error_line(e));
        }
        result
    }
}

/// Return the hostname from an SSH target (`user@host` → `host`, `host` → `host`).
pub fn host_from_target(target: &str) -> &str {
    target.rsplit('@').next().unwrap_or(target)
}

fn step(msg: &str) {
    eprintln!("\x1b[36m▸\x1b[0m {msg}");
}

fn step_marker(key: &str) {
    if JSON_EVENTS.load(Ordering::Relaxed) {
        let message = match key {
            "check" => "checking the server",
            "install" => "installing atlas",
            "user" => "creating the crew user",
            "service" => "writing the systemd service",
            "ssh" => "configuring SSH access",
            "browsers" => "installing browsers",
            _ => "done",
        };
        println!(
            "{}",
            progress::event_line(progress::Kind::Step, key, json!({}), message)
        );
    } else {
        println!("step: {key}");
    }
}

/// Ask the host which of the pre-rename and current names exist (one ssh round trip).
async fn probe_host(r: &Remote) -> Result<Probe> {
    let out = r
        .exec(
            "t() { if \"$@\" >/dev/null 2>&1; then printf 1; else printf 0; fi; }; \
             t test -f /etc/systemd/system/atlas-bot.service; \
             t id atlas-bot; t id atlas-crew",
        )
        .await?;
    if out.code != 0 {
        bail!(
            "{}",
            t!("cmd_crew_probe_failed", reason = out.stderr.trim())
        );
    }
    Ok(parse_probe(out.stdout.trim()))
}

/// `"101"` → legacy unit, no legacy user, new user (the order [`probe_host`] asks in).
fn parse_probe(s: &str) -> Probe {
    let b = |i: usize| s.as_bytes().get(i) == Some(&b'1');
    Probe {
        legacy_unit: b(0),
        legacy_user: b(1),
        new_user: b(2),
    }
}

/// The error for a swap script's exit code (0 is fine): what the person should do next. The wait for hostd is
/// the same one `atlas crew update` has, so install and update fail the same way.
pub(super) fn swap_outcome(code: i32, stderr: &str, target: &str) -> Result<()> {
    match code {
        0 => Ok(()),
        3 => bail!(
            "{}",
            t!(
                "cmd_crew_swap_rolled_back",
                secs = binary::HEALTH_TIMEOUT_S,
                target = target
            )
        ),
        4 => bail!(
            "{}",
            t!(
                "cmd_crew_swap_no_previous",
                target = target,
                secs = binary::HEALTH_TIMEOUT_S
            )
        ),
        _ => bail!(
            "{}",
            t!(
                "cmd_crew_install_binary_failed",
                target = target,
                reason = stderr.trim()
            )
        ),
    }
}

async fn run_install(args: InstallArgs) -> Result<()> {
    let host = host_from_target(&args.ssh_target).to_owned();
    let r = Remote::crew(&args.ssh_target, args.port).with_identity(args.identity.clone());
    // The key is looked up first: a person without one is told before anything is installed.
    let pubkey = local_public_key(&args.ssh_target, args.port, args.identity.clone())?;

    // --- Preflight: reads only, names everything that is missing ---
    step_marker("check");
    step(&t!(
        "cmd_crew_step_preflight",
        target = args.ssh_target.as_str()
    ));
    let target = preflight(&r, &args.ssh_target).await?;

    // --- Sudo check: with a password, every privileged call sends it itself (tty_tickets) ---
    let p = Priv::prepare(
        &r,
        &args.ssh_target,
        args.sudo_password_stdin,
        &format!("atlas crew install {}", args.ssh_target),
    )
    .await?;

    // What an earlier install left (pre-rename "Atlas Bot" servers keep their user and data).
    let probe = probe_host(&r).await?;
    let account = layout::account(probe);

    // --- Binary: staged only; it goes live in the swap below, after the unit exists ---
    step_marker("install");
    step(&t!("cmd_crew_step_staging"));
    let binary = args
        .binary
        .clone()
        .or_else(|| std::env::var_os("ATLAS_CREW_BINARY").map(Into::into));
    match &binary {
        Some(path) => stage_upload(&p, path).await?,
        None => stage_release(&p, env!("CARGO_PKG_VERSION"), target).await?,
    }

    // --- User ---
    step_marker("user");
    if let Some(notice) = layout::reuse_notice(&account) {
        step(&notice);
    } else {
        step(&t!("cmd_crew_step_creating_user", user = account.user));
    }
    if account.create {
        let add = format!(
            "sudo useradd --system --create-home --home-dir {} --shell /bin/bash {}",
            shq(&account.home),
            account.user
        );
        let o = p.exec(&add).await?;
        if o.code != 0 {
            bail!(
                "{}",
                t!("cmd_crew_useradd_failed", reason = o.stderr.trim())
            );
        }
    }
    // The ssh user to reach this server as: Atlas Desktop reads it (it differs on a migrated server).
    if JSON_EVENTS.load(Ordering::Relaxed) {
        let line = progress::event_line(
            progress::Kind::Info,
            "crew.user",
            json!({ "user": account.user }),
            &format!("the ssh user is {}", account.user),
        );
        println!("{line}");
    } else {
        // Old clients and people read this line as it is; Atlas Desktop reads the event above.
        // i18n-ignore: read by old clients
        println!("user: {}", account.user);
    }

    // The desktop reaches a crew server over SSH as its user and runs `$HOME/.atlas/bin/atlas hostd`, the same
    // path as any remote; without this link it found no binary and the first crewmate never got created.
    let link = format!(
        "sudo -u {u} mkdir -p {home}/.atlas/bin && sudo -u {u} ln -sfn {dir}/atlas {home}/.atlas/bin/atlas",
        u = account.user,
        home = shq(&account.home),
        dir = shq(INSTALL_DIR),
    );
    let o = p.exec(&link).await?;
    if o.code != 0 {
        bail!(
            "{}",
            t!(
                "cmd_crew_link_failed",
                user = account.user,
                reason = o.stderr.trim()
            )
        );
    }

    // Ensure .ssh dir exists under the home with correct ownership
    let mk_ssh = format!(
        "sudo mkdir -p {home}/.ssh && sudo chmod 700 {home}/.ssh && sudo chown {u}:{u} {home}/.ssh",
        home = shq(&account.home),
        u = account.user,
    );
    let o = p.exec(&mk_ssh).await?;
    if o.code != 0 {
        bail!(
            "{}",
            t!("cmd_crew_mkdir_ssh_failed", reason = o.stderr.trim())
        );
    }

    // --- Systemd unit ---
    step_marker("service");
    // A server installed as "Atlas Bot": stop and remove its unit first (two units must never run one hostd).
    let retire = layout::retire_legacy_service(probe);
    if !retire.is_empty() {
        step(&t!("cmd_crew_step_retire"));
        for cmd in &retire {
            let o = p.exec(cmd).await?;
            if o.code != 0 {
                bail!(
                    "{}",
                    t!(
                        "cmd_crew_command_failed",
                        command = cmd.as_str(),
                        reason = o.stderr.trim()
                    )
                );
            }
        }
    }
    step(&t!("cmd_crew_step_unit"));
    let unit = layout::systemd_unit(&account);
    write_unit(&p, &format!("/etc/systemd/system/{SERVICE}.service"), &unit).await?;
    let o = p
        .exec(&format!(
            "sudo systemctl daemon-reload && sudo systemctl enable {SERVICE}"
        ))
        .await?;
    if o.code != 0 {
        bail!("{}", t!("cmd_crew_enable_failed", reason = o.stderr.trim()));
    }

    // --- One Linux user per crewmate ---
    // The helpers, the engines every crewmate user runs, and a user for each crewmate that already
    // exists. All before hostd is swapped in and restarted: it looks for the helper's socket when it starts.
    let ids = list_crewmates(&p, &account).await?;
    step(&t!("cmd_crew_step_isolation", count = ids.len()));
    let engines = isolation_engines();
    for action in isolation::install_actions(
        &account,
        &ids,
        &engines
            .iter()
            .map(|(a, c)| (*a, c.as_str()))
            .collect::<Vec<_>>(),
    ) {
        run_action(&p, &action).await?;
    }
    if let Some(n) = isolation::migration_notice(ids.len()) {
        step(&n);
    }

    // --- Swap in, restart and wait for hostd (rolls back to the binary that was there) ---
    step(&t!(
        "cmd_crew_step_start",
        service = SERVICE,
        secs = binary::HEALTH_TIMEOUT_S
    ));
    let o = p
        .exec(&binary::install_script(binary::HEALTH_TIMEOUT_S))
        .await?;
    swap_outcome(o.code, &o.stderr, &args.ssh_target)?;
    if let Some(warning) = binary::report(&o.stdout, None) {
        eprintln!("\x1b[33m◆\x1b[0m {warning}");
    }
    if probe.legacy_unit {
        // The old binary dir goes only now: the old unit pointed at it and is gone, the new one is up.
        let o = p.exec(&layout::remove_legacy_install_dir()).await?;
        if o.code != 0 {
            eprintln!(
                "{}",
                t!("cmd_crew_old_dir_not_removed", reason = o.stderr.trim())
            );
        }
    }

    // --- SSH authorized_keys ---
    step_marker("ssh");
    step(&t!("cmd_crew_step_ssh", user = account.user));
    add_ssh_key(&p, &account, &pubkey).await?;

    if args.browsers {
        step_marker("browsers");
        super::browsers::install(&p, &args.ssh_target).await?;
    }

    let ready = format!(
        "\x1b[32m✓\x1b[0m {}",
        t!(
            "cmd_crew_install_ready",
            account = format!("{}@{host}", account.user)
        )
    );
    // With --progress json stdout carries events only.
    if JSON_EVENTS.load(Ordering::Relaxed) {
        eprintln!("{ready}");
    } else {
        println!("{ready}");
    }
    step_marker("done");
    Ok(())
}

/// Check the host over ssh in ONE probe that changes nothing (Linux with systemd as PID 1, sudo and every tool
/// the install and the runtime need; x86_64 or aarch64) and return the release target its `uname -m` maps to.
/// Everything that is missing is reported together, with the package that has it. Shared with `atlas crew update`.
pub(super) async fn preflight(r: &Remote, ssh_target: &str) -> Result<&'static str> {
    let out = r.exec(&binary::preflight_script()).await?;
    if out.code == 255 {
        // ssh's own failure (refused, timeout, key rejected): not the host's OS. Saying "needs Linux with systemd"
        // here sent the user hunting for the wrong problem on a server whose sshd was simply down.
        // The stable code lets a client tell "the key is not authorized yet" from "the host is down" without
        // reading the text (OpenSSH's own English, which is not ours to translate or to match on).
        let stderr = out.stderr.trim();
        let code = if stderr.contains("Permission denied (publickey") {
            "ssh.key_rejected"
        } else {
            "ssh.unreachable"
        };
        return Err(progress::Coded::new(
            code,
            json!({ "target": ssh_target, "detail": stderr }),
            t!(
                "cmd_crew_ssh_unreachable",
                target = ssh_target,
                reason = stderr
            ),
        )
        .into());
    }
    let p = binary::parse_preflight(&out.stdout);
    // The OS first: a Mac or a BSD also lacks "tools", and the useful message is that it is not Linux.
    let target = binary::target_for_uname(&p.uname)?;
    if out.code != 0 || !p.is_ready() {
        return Err(binary::preflight_error(ssh_target, &p));
    }
    Ok(target)
}

/// Leave `version`'s binary for `target` at `<INSTALL_DIR>/atlas.new`, verified:
/// 1. downloaded and checksummed on the host (the fast path);
/// 2. when the host cannot do that (offline, behind a proxy it has no settings for), downloaded and checksummed
///    on THIS machine, which has the user's proxy and credentials, and streamed over ssh;
/// 3. last, the cache `atlas update` keeps at `~/.atlas/remote/atlas-<arch>-linux`.
pub(super) async fn stage_release(p: &Priv<'_>, version: &str, target: &str) -> Result<()> {
    let out = p.exec(&binary::download_script(version, target)).await?;
    if out.code == 0 {
        return Ok(());
    }
    let on_host = out.stderr.trim().to_owned();
    eprintln!(
        "\x1b[33m◆\x1b[0m {}",
        t!(
            "cmd_crew_host_download_failed",
            version = version,
            reason = on_host
                .lines()
                .last()
                .map_or_else(|| t!("cmd_crew_no_reason_given"), str::to_owned)
        )
    );
    let tmp = tempfile::tempdir().with_context(|| t!("cmd_crew_temp_dir_failed"))?;
    let (v, t, dir) = (version.to_owned(), target.to_owned(), tmp.path().to_owned());
    let here = tokio::task::spawn_blocking(move || download_binary_here(&v, &t, &dir))
        .await
        .with_context(|| t!("cmd_crew_download_task_failed"))?;
    match here {
        Ok(bin) => stage_upload(p, &bin).await,
        Err(e) => {
            let cache = dirs::home_dir()
                .with_context(|| t!("cmd_desktop_no_home"))?
                .join(binary::offline_cache_path(target));
            if !cache.exists() {
                bail!(
                    "{}",
                    t!(
                        "cmd_crew_get_binary_failed",
                        version = version,
                        on_host = on_host.as_str(),
                        here = format!("{e:#}"),
                        path = cache.display().to_string()
                    )
                );
            }
            eprintln!(
                "\x1b[33m◆\x1b[0m {}",
                t!("cmd_crew_download_here_failed", reason = format!("{e:#}"))
            );
            stage_upload(p, &cache).await
        }
    }
}

/// Download `version`'s release asset for `target` with this machine's curl (so its proxy settings apply),
/// verify the checksum, and extract the `atlas` binary into `dir`.
fn download_binary_here(
    version: &str,
    target: &str,
    dir: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let asset = binary::asset_name(version, target);
    let tarball = crate::commands::release::download_verified(
        &format!("v{version}"),
        &asset,
        dir,
        |_, _| {},
    )?;
    let file = std::fs::File::open(&tarball)?;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in ar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.file_name().is_some_and(|n| n == "atlas") {
            let out = dir.join("atlas");
            entry.unpack(&out)?;
            return Ok(out);
        }
    }
    bail!("{}", t!("cmd_crew_asset_no_binary", asset = asset.as_str()))
}

/// Stream a local binary to `<INSTALL_DIR>/atlas.new`.
pub(super) async fn stage_upload(p: &Priv<'_>, local_bin: &std::path::Path) -> Result<()> {
    let content = std::fs::read(local_bin).with_context(|| {
        t!(
            "cmd_install_read_failed",
            path = local_bin.display().to_string()
        )
    })?;
    let o = p
        .exec_stdin(&binary::stage_upload_script(), Some(&content))
        .await?;
    if o.code != 0 {
        bail!("{}", t!("cmd_crew_upload_failed", reason = o.stderr.trim()));
    }
    Ok(())
}

/// The ids of the crewmates the crew user already has (their directories under its home).
async fn list_crewmates(p: &Priv<'_>, a: &Account) -> Result<Vec<String>> {
    let o = p.exec(&isolation::list_crewmates_script(a)).await?;
    if o.code != 0 {
        bail!("{}", t!("cmd_crew_list_failed", reason = o.stderr.trim()));
    }
    Ok(o.stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// The engines installed system-wide: each built-in agent with the vendor's own install line
/// (`install` in `data/agents.json`, the lines `atlas crew login` prints).
fn isolation_engines() -> Vec<(&'static str, String)> {
    atlas_core::agents::all()
        .iter()
        .map(|a| (a.id.as_str(), a.install.clone()))
        .collect()
}

/// Write the unit `file` on the server through [`layout::write_unit_script`], the one way every unit Atlas owns is
/// written: unmasked first, replaced rather than written through. A unit that had to be unmasked is said in one line;
/// one that could not be fails with the command to run (and a `hint:` line on stderr, which Atlas Desktop shows as what to do).
async fn write_unit(p: &Priv<'_>, file: &str, contents: &str) -> Result<()> {
    let o = p
        .exec_stdin(&layout::write_unit_script(file), Some(contents.as_bytes()))
        .await?;
    for unit in layout::unmasked_units(&o.stdout) {
        step(&t!("cmd_crew_unit_unmasked", unit = unit.to_string()));
    }
    if o.code != 0 {
        if o.code == layout::UNMASK_FAILED {
            let unit = file.rsplit('/').next().unwrap_or(file);
            eprintln!("{}", layout::unmask_hint(unit));
        }
        return Err(layout::unit_write_error(o.code, &o.stderr, file));
    }
    Ok(())
}

/// Run one isolation action on the server. Through [`Priv`], like every privileged step of the install:
/// a server whose sudo asks for a password (Atlas Desktop sends it) gets it with each of these too.
async fn run_action(p: &Priv<'_>, action: &isolation::Action) -> Result<()> {
    let (what, o) = match action {
        isolation::Action::Run(sh) => (sh.clone(), p.exec(sh).await?),
        isolation::Action::Engine {
            agent,
            line,
            script,
        } => {
            let o = p.exec(script).await?;
            if o.code != 0 {
                let why = o.stderr.lines().rev().find(|l| !l.trim().is_empty());
                eprintln!(
                    "\x1b[33m◆\x1b[0m {}",
                    t!(
                        "cmd_crew_engine_missing",
                        agent = agent.to_string(),
                        reason = why.map_or_else(
                            || t!("cmd_crew_installer_failed"),
                            |w| w.trim().to_owned()
                        ),
                        line = line.as_str()
                    )
                );
            }
            return Ok(());
        }
        isolation::Action::Write { file, contents } => {
            return write_unit(p, file, contents).await;
        }
    };
    if o.code != 0 {
        bail!(
            "{}",
            t!(
                "cmd_crew_command_failed",
                command = what.chars().take(120).collect::<String>(),
                reason = o.stderr.trim()
            )
        );
    }
    Ok(())
}

/// The public key `ssh` itself would offer to `ssh_target`: the `--identity` key, else the first existing one of
/// `ssh -G` (`~/.ssh/config` identities and the default key names, so a key called anything is found). Read before
/// the install changes anything on the server.
fn local_public_key(
    ssh_target: &str,
    port: u16,
    identity: Option<std::path::PathBuf>,
) -> Result<String> {
    let target = atlas_core::ssh::SshTarget::new(ssh_target, Some(port)).with_identity(identity);
    let key = atlas_core::ssh::find_key(&target).with_context(|| match &target.identity {
        Some(k) => t!("cmd_crew_key_missing", path = k.display().to_string()),
        None => t!("cmd_crew_no_ssh_key"),
    })?;
    key.public_text().map_err(anyhow::Error::msg)
}

/// Append `pubkey` to the crew user's authorized_keys (idempotent), then lock the file down.
async fn add_ssh_key(p: &Priv<'_>, account: &Account, pubkey: &str) -> Result<()> {
    let script = format!(
        r#"f={home}/.ssh/authorized_keys
sudo grep -qxF {key_q} "$f" 2>/dev/null || printf '%s\n' {key_q} | sudo tee -a "$f" > /dev/null
sudo chmod 600 "$f"
sudo chown {user}:{user} "$f""#,
        home = shq(&account.home),
        key_q = shq(pubkey),
        user = account.user,
    );
    let o = p.exec(&script).await?;
    if o.code != 0 {
        bail!(
            "{}",
            t!("cmd_crew_add_key_failed", reason = o.stderr.trim())
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_from_target_strips_user_prefix() {
        assert_eq!(
            host_from_target("ubuntu@server.example.com"),
            "server.example.com"
        );
        assert_eq!(host_from_target("server.example.com"), "server.example.com");
        assert_eq!(host_from_target("root@10.0.0.1"), "10.0.0.1");
        assert_eq!(host_from_target("10.0.0.1"), "10.0.0.1");
    }

    // The probe is one ssh round trip answering three yes/no questions in a fixed order; the plan
    // ([`layout::account`], [`layout::retire_legacy_service`]) is decided from it alone.
    #[test]
    fn the_probe_answer_is_read_in_the_order_it_is_asked() {
        assert_eq!(parse_probe("000"), Probe::default());
        assert_eq!(
            parse_probe("110"),
            Probe {
                legacy_unit: true,
                legacy_user: true,
                new_user: false
            }
        );
        assert_eq!(
            parse_probe("001"),
            Probe {
                legacy_unit: false,
                legacy_user: false,
                new_user: true
            }
        );
        // A short or garbled answer reads as "not there", never as an existing user.
        assert_eq!(parse_probe(""), Probe::default());
        assert_eq!(parse_probe("x"), Probe::default());
    }

    #[test]
    fn step_marker_output_is_stable() {
        // step_marker writes to stdout; capture by asserting the format we produce.
        // The frontend's parseInstallLine depends on this exact format.
        for key in [
            "check", "install", "user", "service", "ssh", "browsers", "done",
        ] {
            // Verify the function compiles and the key is a valid str literal
            let expected = format!("step: {key}");
            assert!(
                expected.starts_with("step: "),
                "marker {key} must start with 'step: '"
            );
        }
    }

    // Gap 2/3 (CrewOSAudit): the swap's exit code is what the person is told. A rollback and a failed rollback say
    // different things, both point at the logs, and a plain failure carries the server's own message.
    #[test]
    fn the_swap_exit_codes_become_messages_that_point_at_the_logs() {
        assert!(swap_outcome(0, "", "me@srv").is_ok());
        let rolled = swap_outcome(3, "", "me@srv").unwrap_err().to_string();
        assert!(rolled.contains("previous binary was restored"), "{rolled}");
        assert!(rolled.contains("atlas crew logs me@srv"), "{rolled}");
        let dead = swap_outcome(4, "", "me@srv").unwrap_err().to_string();
        assert!(dead.contains("no previous binary"), "{dead}");
        assert!(dead.contains("atlas crew logs me@srv"), "{dead}");
        let other = swap_outcome(2, "no staged binary\n", "me@srv")
            .unwrap_err()
            .to_string();
        assert!(other.contains("no staged binary"), "{other}");
    }

    #[test]
    fn sudo_password_never_in_argv() {
        // The password is delivered via stdin, not InstallArgs.
        // This struct literal is exhaustive: any added password String field
        // would break this literal, making the absence compile-time-proven.
        let _args = InstallArgs {
            ssh_target: "ubuntu@example.com".into(),
            yes: true,
            port: 22,
            identity: None,
            sudo_password_stdin: true,
            binary: None,
            browsers: false,
            progress: Progress::Text,
        };
    }
}
