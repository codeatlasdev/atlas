//! `atlas bot install <ssh-target>` — turn a Linux x86_64 server into a bot computer.
//!
//! Steps:
//! 1. Preflight: uname -sm, systemctl, sudo
//! 2. Install the atlas binary to /usr/local/lib/atlas-bot/ (curl on host, fallback upload)
//! 3. Create the atlas-bot system user
//! 4. Write and enable the systemd service (ATLAS_HOSTD_IDLE_SECS=never)
//! 5. Append the caller's SSH public key to /var/lib/atlas-bot/.ssh/authorized_keys

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::{Remote, shq};

const INSTALL_DIR: &str = "/usr/local/lib/atlas-bot";
const BOT_USER: &str = "atlas-bot";
const BOT_HOME: &str = "/var/lib/atlas-bot";
const SERVICE: &str = "atlas-bot";

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// Skip confirmation prompts
    #[arg(short, long)]
    pub yes: bool,
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

/// The systemd unit file content (pure — testable without SSH).
pub fn systemd_unit() -> &'static str {
    "[Unit]\n\
Description=Atlas Crew (atlas hostd)\n\
After=network-online.target\n\
Wants=network-online.target\n\
[Service]\n\
User=atlas-bot\n\
Environment=ATLAS_HOSTD_IDLE_SECS=never\n\
Environment=ATLAS_BOT_HOST=1\n\
ExecStart=/usr/local/lib/atlas-bot/atlas hostd\n\
Restart=always\n\
RestartSec=5\n\
NoNewPrivileges=true\n\
ProtectSystem=strict\n\
ReadWritePaths=/var/lib/atlas-bot\n\
PrivateTmp=true\n\
[Install]\n\
WantedBy=multi-user.target\n"
}

fn step(msg: &str) {
    eprintln!("\x1b[36m▸\x1b[0m {msg}");
}

async fn run_install(args: InstallArgs) -> Result<()> {
    let host = host_from_target(&args.ssh_target).to_owned();
    let r = Remote::new(&args.ssh_target);

    // --- Preflight ---
    step(&format!("preflight {}", args.ssh_target));
    let out = r
        .exec(
            "uname -sm && command -v systemctl >/dev/null 2>&1 && command -v sudo >/dev/null 2>&1",
        )
        .await?;
    if out.code != 0 {
        bail!("atlas bot install needs a Linux x86_64 host with systemd and sudo");
    }
    let uname = out.stdout.trim().to_string();
    if !uname.starts_with("Linux x86_64") {
        bail!("atlas bot install needs a Linux x86_64 host with systemd and sudo (got: {uname})");
    }

    // --- Binary ---
    step("installing atlas binary");
    get_binary(&r).await?;

    // --- User ---
    step("creating atlas-bot user");
    let user_exists = r
        .exec("id atlas-bot >/dev/null 2>&1 && echo yes || echo no")
        .await?;
    if user_exists.stdout.trim() == "no" {
        let add = format!(
            "sudo useradd --system --create-home --home-dir {} --shell /bin/bash {}",
            shq(BOT_HOME),
            BOT_USER
        );
        let o = r.exec(&add).await?;
        if o.code != 0 {
            bail!("useradd failed: {}", o.stderr.trim());
        }
    }

    // Ensure .ssh dir exists under the bot home with correct ownership
    let mk_ssh = format!(
        "sudo mkdir -p {home}/.ssh && sudo chmod 700 {home}/.ssh && sudo chown {u}:{u} {home}/.ssh",
        home = shq(BOT_HOME),
        u = BOT_USER,
    );
    let o = r.exec(&mk_ssh).await?;
    if o.code != 0 {
        bail!("creating .ssh dir failed: {}", o.stderr.trim());
    }

    // --- Systemd unit ---
    step("writing systemd service");
    let unit = systemd_unit();
    let write_unit = format!("sudo tee /etc/systemd/system/{SERVICE}.service > /dev/null");
    let o = r.exec_stdin(&write_unit, Some(unit.as_bytes())).await?;
    if o.code != 0 {
        bail!("writing service file failed: {}", o.stderr.trim());
    }

    // --- Enable and start ---
    step("enabling and starting atlas-bot");
    let o = r
        .exec("sudo systemctl daemon-reload && sudo systemctl enable --now atlas-bot")
        .await?;
    if o.code != 0 {
        bail!("systemctl enable --now failed: {}", o.stderr.trim());
    }

    // --- SSH authorized_keys ---
    step("configuring SSH access for atlas-bot");
    add_ssh_key(&r).await?;

    println!(
        "\x1b[32m✓\x1b[0m Bot computer ready: atlas-bot@{host}. \
Add it in Atlas Desktop → Crew, then sign the agents in (atlas bot login <agent>)."
    );
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
tmpf=$(mktemp /tmp/atlas-bot-XXXXXX.tar.gz) &&
curl -fsSL {url_q} -o "$tmpf" &&
WANT=$(curl -fsSL {sha_q} | awk '{{print $1}}') &&
GOT=$(sha256sum "$tmpf" | awk '{{print $1}}') &&
[ "$WANT" = "$GOT" ] || {{ echo 'checksum mismatch' >&2; exit 1; }} &&
sudo tar -xzf "$tmpf" --strip-components=1 -C {dir_q} &&
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
    let content =
        std::fs::read(&local_bin).with_context(|| format!("reading {}", local_bin.display()))?;
    let upload = format!(
        "sudo mkdir -p {dir_q} && sudo tee {dir_q}/atlas > /dev/null && sudo chmod +x {dir_q}/atlas",
        dir_q = shq(INSTALL_DIR),
    );
    let o = r.exec_stdin(&upload, Some(&content)).await?;
    if o.code != 0 {
        bail!("binary upload failed: {}", o.stderr.trim());
    }
    Ok(())
}

/// Append the caller's local SSH public key to atlas-bot's authorized_keys.
async fn add_ssh_key(r: &Remote) -> Result<()> {
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
        home = shq(BOT_HOME),
        key_q = shq(pubkey),
        user = BOT_USER,
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

    #[test]
    fn systemd_unit_contains_required_fields() {
        let unit = systemd_unit();
        assert!(unit.contains("User=atlas-bot"), "missing User=");
        assert!(
            unit.contains("ATLAS_HOSTD_IDLE_SECS=never"),
            "missing ATLAS_HOSTD_IDLE_SECS=never"
        );
        assert!(
            unit.contains("ExecStart=/usr/local/lib/atlas-bot/atlas hostd"),
            "missing ExecStart="
        );
        assert!(
            unit.contains("NoNewPrivileges=true"),
            "missing NoNewPrivileges="
        );
        assert!(
            unit.contains("ProtectSystem=strict"),
            "missing ProtectSystem="
        );
        assert!(
            unit.contains("ReadWritePaths=/var/lib/atlas-bot"),
            "missing ReadWritePaths="
        );
        assert!(unit.contains("PrivateTmp=true"), "missing PrivateTmp=");
        assert!(
            unit.contains("WantedBy=multi-user.target"),
            "missing WantedBy="
        );
        assert!(
            unit.contains("ATLAS_BOT_HOST=1"),
            "missing ATLAS_BOT_HOST=1"
        );
    }

    #[test]
    fn systemd_unit_is_valid_ini_sections() {
        let unit = systemd_unit();
        assert!(unit.contains("[Unit]"), "missing [Unit]");
        assert!(unit.contains("[Service]"), "missing [Service]");
        assert!(unit.contains("[Install]"), "missing [Install]");
    }
}
