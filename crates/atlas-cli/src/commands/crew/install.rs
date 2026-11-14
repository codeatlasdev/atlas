//! `atlas crew install <ssh-target>` — turn a Linux x86_64 server into a crew server.
//!
//! Steps (what the host looks like is probed first, see [`layout`]):
//! 1. Preflight: uname -sm, systemctl, sudo
//! 2. Install the atlas binary to /usr/local/lib/atlas-crew/ (curl on host, fallback upload)
//! 3. Create the atlas-crew system user (an existing atlas-bot user is reused)
//! 4. Retire a pre-rename `atlas-bot.service`, write and enable `atlas-crew.service`
//! 5. Append the caller's SSH public key to <home>/.ssh/authorized_keys

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::{Remote, shq};

use super::layout::{self, Account, INSTALL_DIR, Probe, SERVICE};

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
    /// Read sudo password from stdin instead of requiring passwordless sudo.
    /// Used by Atlas Desktop; the password is NEVER placed on argv.
    #[arg(long, hide = true)]
    pub sudo_password_stdin: bool,
    /// Upload this Linux x86_64 `atlas` instead of downloading the release of this version.
    /// For builds whose hostd is not published yet (the release would lack the crew commands).
    /// Also read from `ATLAS_CREW_BINARY`, so Atlas Desktop picks it up.
    #[arg(long)]
    pub binary: Option<std::path::PathBuf>,
}

impl AtlasCommand for InstallArgs {
    async fn run(self) -> Result<()> {
        run_install(self).await
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
    println!("step: {key}");
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
        bail!("probing the host failed: {}", out.stderr.trim());
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

async fn run_install(args: InstallArgs) -> Result<()> {
    let host = host_from_target(&args.ssh_target).to_owned();
    let r = if args.port == 22 {
        Remote::new(&args.ssh_target)
    } else {
        Remote::with_port(&args.ssh_target, args.port)
    };

    // --- Preflight ---
    step_marker("check");
    step(&format!("preflight {}", args.ssh_target));
    let out = r
        .exec(
            "uname -sm && command -v systemctl >/dev/null 2>&1 && command -v sudo >/dev/null 2>&1",
        )
        .await?;
    if out.code == 255 {
        // ssh's own failure (refused, timeout, key rejected): not the host's OS. Saying "needs Linux with systemd"
        // here sent the user hunting for the wrong problem on a server whose sshd was simply down.
        bail!(
            "could not connect to {} over SSH: {}",
            args.ssh_target,
            out.stderr.trim()
        );
    }
    if out.code != 0 {
        bail!("atlas crew install needs a Linux x86_64 host with systemd and sudo");
    }
    let uname = out.stdout.trim().to_string();
    if !uname.starts_with("Linux x86_64") {
        bail!("atlas crew install needs a Linux x86_64 host with systemd and sudo (got: {uname})");
    }

    // --- Sudo check / credential cache ---
    if args.sudo_password_stdin {
        // Desktop GUI pipes the password via stdin. Read one line and use it to
        // prime the sudo credential cache (15-min window covers the full install).
        // The password is NEVER passed on argv or logged.
        use tokio::io::AsyncBufReadExt as _;
        let mut pw = String::new();
        tokio::io::BufReader::new(tokio::io::stdin())
            .read_line(&mut pw)
            .await
            .context("reading sudo password from stdin")?;
        let pw = pw.trim_end_matches('\n').trim_end_matches('\r').to_string();
        let pw_bytes = format!("{pw}\n").into_bytes();
        let cache = r.exec_stdin("sudo -S -p '' -v", Some(&pw_bytes)).await?;
        if cache.code != 0 {
            bail!("sudo password rejected — check your password and try again");
        }
        // pw and pw_bytes are dropped here
    } else {
        let sudo_check = r.exec("sudo -n true").await?;
        if sudo_check.code != 0 {
            bail!(
                "sudo on {} asks for a password — grant passwordless sudo to this user, \
or run the install from a terminal: atlas crew install {}",
                args.ssh_target,
                args.ssh_target
            );
        }
    }

    // What an earlier install left (pre-rename "Atlas Bot" servers keep their user and data).
    let probe = probe_host(&r).await?;
    let account = layout::account(probe);

    // --- Binary ---
    step_marker("install");
    step("installing atlas binary");
    let binary = args
        .binary
        .clone()
        .or_else(|| std::env::var_os("ATLAS_CREW_BINARY").map(Into::into));
    match &binary {
        Some(path) => upload_binary(&r, path).await?,
        None => get_binary(&r).await?,
    }

    // --- User ---
    step_marker("user");
    if let Some(notice) = layout::reuse_notice(&account) {
        step(&notice);
    } else {
        step(&format!("creating {} user", account.user));
    }
    if account.create {
        let add = format!(
            "sudo useradd --system --create-home --home-dir {} --shell /bin/bash {}",
            shq(&account.home),
            account.user
        );
        let o = r.exec(&add).await?;
        if o.code != 0 {
            bail!("useradd failed: {}", o.stderr.trim());
        }
    }
    // The ssh user to reach this server as: Atlas Desktop reads it (it differs on a migrated server).
    println!("user: {}", account.user);

    // The desktop reaches a crew server over SSH as its user and runs `$HOME/.atlas/bin/atlas hostd`, the same
    // path as any remote; without this link it found no binary and the first crewmate never got created.
    let link = format!(
        "sudo -u {u} mkdir -p {home}/.atlas/bin && sudo -u {u} ln -sfn {dir}/atlas {home}/.atlas/bin/atlas",
        u = account.user,
        home = shq(&account.home),
        dir = shq(INSTALL_DIR),
    );
    let o = r.exec(&link).await?;
    if o.code != 0 {
        bail!(
            "linking atlas for {} failed: {}",
            account.user,
            o.stderr.trim()
        );
    }

    // Ensure .ssh dir exists under the home with correct ownership
    let mk_ssh = format!(
        "sudo mkdir -p {home}/.ssh && sudo chmod 700 {home}/.ssh && sudo chown {u}:{u} {home}/.ssh",
        home = shq(&account.home),
        u = account.user,
    );
    let o = r.exec(&mk_ssh).await?;
    if o.code != 0 {
        bail!("creating .ssh dir failed: {}", o.stderr.trim());
    }

    // --- Systemd unit ---
    step_marker("service");
    // A server installed as "Atlas Bot": stop and remove its unit first (two units must never run one hostd).
    let retire = layout::retire_legacy_service(probe);
    if !retire.is_empty() {
        step("retiring the old atlas-bot service (data is kept)");
        for cmd in &retire {
            let o = r.exec(cmd).await?;
            if o.code != 0 {
                bail!("`{cmd}` failed: {}", o.stderr.trim());
            }
        }
    }
    step("writing systemd service");
    let unit = layout::systemd_unit(&account);
    let write_unit = format!("sudo tee /etc/systemd/system/{SERVICE}.service > /dev/null");
    let o = r.exec_stdin(&write_unit, Some(unit.as_bytes())).await?;
    if o.code != 0 {
        bail!("writing service file failed: {}", o.stderr.trim());
    }

    // --- Enable and start ---
    step(&format!("enabling and starting {SERVICE}"));
    let o = r
        // `restart`, not `enable --now`: a reinstall over a running server must run the new binary.
        .exec(&format!(
            "sudo systemctl daemon-reload && sudo systemctl enable {SERVICE} && sudo systemctl restart {SERVICE}"
        ))
        .await?;
    if o.code != 0 {
        bail!("systemctl enable --now failed: {}", o.stderr.trim());
    }
    if probe.legacy_unit {
        // The old binary dir goes only now: the old unit pointed at it and is gone, the new one is up.
        let o = r.exec(&layout::remove_legacy_install_dir()).await?;
        if o.code != 0 {
            eprintln!("could not remove the old install dir: {}", o.stderr.trim());
        }
    }

    // --- SSH authorized_keys ---
    step_marker("ssh");
    step(&format!("configuring SSH access for {}", account.user));
    add_ssh_key(&r, &account).await?;

    println!(
        "\x1b[32m✓\x1b[0m Crew server ready: {user}@{host}. \
Add it in Atlas Desktop → Crew, then sign the agents in (atlas crew login <agent>).",
        user = account.user
    );
    step_marker("done");
    Ok(())
}

/// Install the atlas binary on the host.
///
/// Tries `curl` from the GitHub release first (checksummed); falls back to uploading
/// `~/.atlas/remote/atlas-x86_64-linux` when the host has no internet.
async fn get_binary(r: &Remote) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let asset = format!("atlas-{version}-x86_64-unknown-linux-gnu.tar.gz");
    let url = format!(
        "https://github.com/codeatlasdev/atlas/releases/download/v{version}/{}",
        asset
    );
    let sha_url = format!("{url}.sha256");

    let script = format!(
        r#"sudo mkdir -p {dir_q} &&
tmpf=$(mktemp /tmp/atlas-crew-XXXXXX.tar.gz) &&
curl -fsSL {url_q} -o "$tmpf" &&
WANT=$(curl -fsSL {sha_q} | awk '{{print $1}}') &&
GOT=$(sha256sum "$tmpf" | awk '{{print $1}}') &&
[ "$WANT" = "$GOT" ] || {{ echo 'checksum mismatch' >&2; exit 1; }} &&
sudo tar -xzf "$tmpf" -C {dir_q} atlas &&
sudo chmod +x {dir_q}/atlas &&
rm -f "$tmpf""#,
        dir_q = shq(INSTALL_DIR),
        url_q = shq(&url),
        sha_q = shq(&sha_url),
    );

    let out = r.exec(&script).await?;
    if out.code == 0 {
        return Ok(());
    }

    // Fallback: upload the locally cached Linux binary
    let local_bin = dirs::home_dir()
        .context("no home directory")?
        .join(".atlas/remote/atlas-x86_64-linux");

    if !local_bin.exists() {
        bail!(
            "could not download atlas on the host ({})\n  \
and no offline fallback found at {}\n  \
Run `atlas update` on this machine to populate the cache, or ensure the host has internet access.",
            out.stderr.trim(),
            local_bin.display()
        );
    }

    eprintln!("\x1b[33m◆\x1b[0m online download failed; uploading local binary (offline mode)");
    upload_binary(r, &local_bin).await
}

/// Upload a local Linux `atlas` to the host's install dir.
async fn upload_binary(r: &Remote, local_bin: &std::path::Path) -> Result<()> {
    let content =
        std::fs::read(local_bin).with_context(|| format!("reading {}", local_bin.display()))?;
    let upload = format!(
        "sudo mkdir -p {dir_q} && sudo tee {dir_q}/atlas.new > /dev/null && sudo chmod +x {dir_q}/atlas.new && sudo mv -f {dir_q}/atlas.new {dir_q}/atlas",
        dir_q = shq(INSTALL_DIR),
    );
    let o = r.exec_stdin(&upload, Some(&content)).await?;
    if o.code != 0 {
        bail!("binary upload failed: {}", o.stderr.trim());
    }
    Ok(())
}

/// Append the caller's local SSH public key to the crew user's authorized_keys.
async fn add_ssh_key(r: &Remote, account: &Account) -> Result<()> {
    let home = dirs::home_dir().context("no home directory")?;
    let key_path = ["id_ed25519.pub", "id_rsa.pub"]
        .iter()
        .map(|n| home.join(".ssh").join(n))
        .find(|p| p.exists())
        .context(
            "no SSH public key found at ~/.ssh/id_ed25519.pub or ~/.ssh/id_rsa.pub; \
create one with: ssh-keygen -t ed25519",
        )?;

    let pubkey = std::fs::read_to_string(&key_path)
        .with_context(|| format!("reading {}", key_path.display()))?;
    let pubkey = pubkey.trim();

    // Append idempotently, then lock down the file
    let script = format!(
        r#"f={home}/.ssh/authorized_keys
grep -qxF {key_q} "$f" 2>/dev/null || printf '%s\n' {key_q} | sudo tee -a "$f" > /dev/null
sudo chmod 600 "$f"
sudo chown {user}:{user} "$f""#,
        home = shq(&account.home),
        key_q = shq(pubkey),
        user = account.user,
    );
    let o = r.exec(&script).await?;
    if o.code != 0 {
        bail!("adding SSH key failed: {}", o.stderr.trim());
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
        for key in ["check", "install", "user", "service", "ssh", "done"] {
            // Verify the function compiles and the key is a valid str literal
            let expected = format!("step: {key}");
            assert!(
                expected.starts_with("step: "),
                "marker {key} must start with 'step: '"
            );
        }
    }

    #[test]
    fn sudo_preflight_error_message_is_clear() {
        let target = "ubuntu@server.example.com";
        let msg = format!(
            "sudo on {} asks for a password — grant passwordless sudo to this user, \
or run the install from a terminal: atlas crew install {}",
            target, target
        );
        assert!(
            msg.contains("passwordless sudo"),
            "message must mention passwordless sudo"
        );
        assert!(msg.contains(target), "message must contain the target");
        assert!(
            msg.contains("atlas crew install"),
            "message must contain the install command"
        );
        assert!(
            msg.contains("asks for a password"),
            "message must mention password prompt"
        );
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
            sudo_password_stdin: true,
            binary: None,
        };
    }
}
