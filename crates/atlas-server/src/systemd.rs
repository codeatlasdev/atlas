use atlas_core::domain::server::ServiceInfo;
use atlas_core::domain::service::ServiceState;
use atlas_core::Result;
use atlas_ssh::SshClient;

pub fn parse_state(raw: &str) -> ServiceState {
    raw.trim().parse::<ServiceState>().unwrap_or(ServiceState::Unknown)
}

pub fn parse_list_units(output: &str) -> Vec<ServiceInfo> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[0].ends_with(".service") {
                let unit_name = parts[0].to_string();
                let name = unit_name.trim_end_matches(".service").to_string();
                let state = parse_state(parts.get(2).unwrap_or(&"unknown"));
                let enabled = parts.get(3).is_some_and(|s| *s == "enabled");

                Some(ServiceInfo {
                    name,
                    unit_name,
                    state,
                    enabled,
                })
            } else {
                None
            }
        })
        .collect()
}

pub async fn list_services(ssh: &SshClient) -> Result<Vec<ServiceInfo>> {
    let output = ssh
        .exec("systemctl list-units --type=service --state=running --no-pager --plain")
        .await?;
    Ok(parse_list_units(&output.stdout))
}

pub async fn service_status(ssh: &SshClient, unit: &str) -> Result<ServiceStatus> {
    let output = ssh
        .exec(&format!(
            "systemctl show {unit} --property=ActiveState,SubState,MainPID,MemoryCurrent"
        ))
        .await?;

    let mut status = ServiceStatus::default();
    for line in output.stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "ActiveState" => status.active_state = value.to_string(),
                "SubState" => status.sub_state = value.to_string(),
                "MainPID" => status.main_pid = value.parse().unwrap_or(0),
                "MemoryCurrent" => {
                    status.memory_bytes = value.parse().ok();
                }
                _ => {}
            }
        }
    }
    Ok(status)
}

pub async fn restart_service(ssh: &SshClient, unit: &str) -> Result<()> {
    let output = ssh.exec(&format!("systemctl restart {unit}")).await?;
    if output.exit_code != 0 {
        return Err(atlas_core::AtlasError::ServerManagement(format!(
            "restart failed: {}",
            output.stderr
        )));
    }
    Ok(())
}

pub async fn stop_service(ssh: &SshClient, unit: &str) -> Result<()> {
    let output = ssh.exec(&format!("systemctl stop {unit}")).await?;
    if output.exit_code != 0 {
        return Err(atlas_core::AtlasError::ServerManagement(format!(
            "stop failed: {}",
            output.stderr
        )));
    }
    Ok(())
}

pub async fn service_logs(ssh: &SshClient, unit: &str, lines: u32) -> Result<String> {
    let output = ssh
        .exec(&format!("journalctl -u {unit} -n {lines} --no-pager"))
        .await?;
    Ok(output.stdout)
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ServiceStatus {
    pub active_state: String,
    pub sub_state: String,
    pub main_pid: u32,
    pub memory_bytes: Option<u64>,
}
