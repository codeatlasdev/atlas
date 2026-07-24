use std::fs;

use tempfile::TempDir;

use super::*;

fn write_config(dir: &TempDir, filename: &str, content: &str) {
    fs::write(dir.path().join(filename), content).unwrap();
}

#[test]
fn test_load_minimal_config() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: minimal
services:
  web:
    command: npm start
"#,
    );

    let config = load(dir.path()).unwrap();
    assert_eq!(config.name, "minimal");
    assert_eq!(config.services.len(), 1);
    assert_eq!(config.services["web"].command, "npm start");
    assert!(config.tunnel.is_none());
    assert!(config.infra.is_none());
}

#[test]
fn test_load_full_config() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: ac
tunnel:
  enabled: true
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: prod-3-codeatlas
  keepalive_interval: 15
  keepalive_max: 3
  reconnect_cooldown: 30
services:
  server:
    command: bun run dev:server
    port: 3070
    health: http://localhost:3070/health/live
  web:
    command: bun run dev:web
    port: 3071
  worker:
    command: bun run dev:worker
infra:
  compose_file: packages/infra/dev/compose.yml
"#,
    );

    let config = load(dir.path()).unwrap();
    assert_eq!(config.name, "ac");

    let tunnel = config.tunnel.unwrap();
    assert!(tunnel.is_enabled());
    assert_eq!(tunnel.local_port, 54320);
    assert_eq!(tunnel.remote_host, "localhost");
    assert_eq!(tunnel.remote_port, 5432);
    assert_eq!(tunnel.ssh_host, "prod-3-codeatlas");
    assert_eq!(tunnel.keepalive_interval, 15);
    assert_eq!(tunnel.keepalive_max, 3);
    assert_eq!(tunnel.reconnect_cooldown, 30);

    assert_eq!(config.services.len(), 3);
    assert_eq!(config.services["server"].port, Some(3070));
    assert_eq!(
        config.services["server"].health.as_deref(),
        Some("http://localhost:3070/health/live")
    );
    assert!(config.services["worker"].port.is_none());

    let infra = config.infra.unwrap();
    assert_eq!(infra.compose_file, "packages/infra/dev/compose.yml");
}

#[test]
fn test_merge_local_override() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: ac
tunnel:
  enabled: true
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: prod-3-codeatlas
services:
  web:
    command: bun run dev:web
    port: 3071
"#,
    );
    write_config(
        &dir,
        "atlas.local.yaml",
        r#"
tunnel:
  enabled: false
"#,
    );

    let config = load(dir.path()).unwrap();
    let tunnel = config.tunnel.unwrap();
    assert!(!tunnel.is_enabled());
    // Other fields preserved
    assert_eq!(tunnel.local_port, 54320);
    assert_eq!(tunnel.ssh_host, "prod-3-codeatlas");
}

#[test]
fn test_merge_local_service_override() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: ac
services:
  web:
    command: bun run dev:web
    port: 3071
  server:
    command: bun run dev:server
    port: 3070
"#,
    );
    write_config(
        &dir,
        "atlas.local.yaml",
        r#"
services:
  web:
    port: 4000
    enabled: false
"#,
    );

    let config = load(dir.path()).unwrap();
    assert_eq!(config.services["web"].port, Some(4000));
    assert!(!config.services["web"].is_enabled());
    // server unchanged
    assert_eq!(config.services["server"].port, Some(3070));
}

#[test]
fn test_service_enabled_default() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: test
services:
  app:
    command: cargo run
"#,
    );

    let config = load(dir.path()).unwrap();
    assert!(config.services["app"].is_enabled());
    assert!(config.services["app"].enabled.is_none());
}

#[test]
fn test_find_root() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested).unwrap();
    fs::write(dir.path().join("atlas.yaml"), "name: test\nservices: {}").unwrap();

    let found = find_root(&nested);
    assert_eq!(found, Some(dir.path().to_path_buf()));
}

#[test]
fn test_find_root_not_found() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a").join("b");
    fs::create_dir_all(&nested).unwrap();
    // No atlas.yaml anywhere

    let found = find_root(&nested);
    assert!(found.is_none());
}

#[test]
fn test_missing_config_returns_error() {
    let dir = TempDir::new().unwrap();
    let result = load(dir.path());
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("No atlas.yaml found"));
}

#[test]
fn test_apply_defaults() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: test
tunnel:
  local_port: 5432
  remote_host: localhost
  remote_port: 5432
  ssh_host: myhost
services:
  app:
    command: cargo run
"#,
    );

    let config = load(dir.path()).unwrap();
    let tunnel = config.tunnel.unwrap();
    // Defaults applied via serde default
    assert_eq!(tunnel.keepalive_interval, 15);
    assert_eq!(tunnel.keepalive_max, 3);
    assert_eq!(tunnel.reconnect_cooldown, 30);
}
