use atlas_core::ports::ssh::SshPort;
use atlas_core::Result;

#[allow(dead_code)]
pub async fn reload_caddy(ssh: &dyn SshPort) -> Result<()> {
    ssh.execute("sudo systemctl reload caddy").await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn validate_config(ssh: &dyn SshPort) -> Result<String> {
    let output = ssh.execute("caddy validate --config /etc/caddy/Caddyfile").await?;
    Ok(output.stdout)
}

#[allow(dead_code)]
pub async fn get_config(ssh: &dyn SshPort) -> Result<String> {
    let output = ssh.execute("cat /etc/caddy/Caddyfile").await?;
    Ok(output.stdout)
}
