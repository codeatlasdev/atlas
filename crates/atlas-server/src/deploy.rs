use atlas_core::ports::ssh::SshPort;
use atlas_core::Result;

pub async fn run_deploy(ssh: &dyn SshPort, service_name: &str) -> Result<String> {
    let steps = [
        format!("cd /opt/{service_name} && git pull"),
        format!("cd /opt/{service_name} && make build"),
        format!("sudo systemctl restart {service_name}"),
    ];

    let mut log = String::new();
    for step in &steps {
        tracing::info!(command = step.as_str(), "deploy step");
        let output = ssh.execute(step).await?;
        log.push_str(&format!("$ {step}\n{}\n", output.stdout));

        if output.exit_code != 0 {
            log.push_str(&format!("STDERR: {}\n", output.stderr));
            log.push_str(&format!("EXIT: {}\n", output.exit_code));
            return Ok(log);
        }
    }

    log.push_str("deploy completed successfully\n");
    Ok(log)
}
