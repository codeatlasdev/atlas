use atlas_core::Result;
use atlas_ssh::SshClient;

#[allow(dead_code)]
pub async fn reload_caddy(ssh: &SshClient) -> Result<()> {
    let output = ssh.exec("systemctl reload caddy").await?;
    if output.exit_code != 0 {
        return Err(atlas_core::AtlasError::ServerManagement(format!(
            "caddy reload failed: {}",
            output.stderr
        )));
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn validate_config(ssh: &SshClient) -> Result<String> {
    let output = ssh.exec("caddy validate --config /etc/caddy/Caddyfile").await?;
    Ok(output.stdout)
}

#[allow(dead_code)]
pub async fn get_config(ssh: &SshClient) -> Result<String> {
    let output = ssh.exec("cat /etc/caddy/Caddyfile").await?;
    Ok(output.stdout)
}
