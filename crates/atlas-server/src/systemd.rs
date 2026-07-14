use atlas_core::domain::server::ServiceInfo;
use atlas_core::domain::service::ServiceState;

pub fn parse_state(output: &str) -> ServiceState {
    output.trim().parse::<ServiceState>().unwrap_or(ServiceState::Unknown)
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
