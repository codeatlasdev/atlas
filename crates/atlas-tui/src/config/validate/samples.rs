//! One real `atlas.yaml` per problem code, for the tests that run the real validators.
//!
//! Every YAML here goes through `load_checked` and `warnings`, never a hand-built [`ConfigError`]: a
//! sample proves the producer emits the code, and the catalog tests render what it produced.

const BASE: &str = "name: t\nservices:\n  web:\n    command: x\n";

const DEPLOY_OK: &str = r#"
name: shop
services:
  web:
    command: bun dev
deploy:
  build: bun run build
  artifact: [dist, package.json]
  targets:
    prod:
      host: prod
      dir: /opt/shop
      keep: 3
      link: { ".env": /opt/shop/shared/.env }
      install: bun install
      hooks: { post_swap: ./sync.sh, on_failure: ./page.sh }
      pause_marker: /opt/shop/shared/.deploy-lock
      services:
        - { name: shop-api, kind: systemd, health: "http://127.0.0.1:3000/health" }
        - { name: db, kind: compose, compose_file: infra/compose.yml }
        - { name: cron, kind: command, restart: "pkill -HUP cron" }
"#;

const AFFECTED_BASE: &str = "name: p\nservices:\n  api: { command: run }\nactions:\n  - { key: c, label: Run checks, command: x, cache: { inputs: ['crates/**'] } }\n";

/// `(code, atlas.yaml)`: the YAML makes the real validators emit `code`. A code may have several samples.
pub fn samples() -> Vec<(&'static str, String)> {
    let deploy = |from: &str, to: &str| DEPLOY_OK.replace(from, to);
    vec![
        // schema.rs
        (
            "config.unknown_key",
            format!("{BASE}    zzzzqq: 1\n"),
        ),
        (
            "config.unknown_key_suggest",
            format!("{BASE}    helth: http://localhost:1\n"),
        ),
        // semantic.rs: remote services
        (
            "config.remote_command",
            "name: t\nservices:\n  db:\n    port: 5\n    command: x\n    remote: { ssh: box, port: 1 }\n".into(),
        ),
        (
            "config.remote_port_missing",
            "name: t\nservices:\n  db:\n    remote: { ssh: box, port: 1 }\n".into(),
        ),
        (
            "config.remote_ssh_invalid",
            "name: t\nservices:\n  db:\n    port: 4\n    remote: { ssh: \"-oProxyCommand=evil\", port: 1 }\n".into(),
        ),
        (
            "config.remote_host_invalid",
            "name: t\nservices:\n  db:\n    port: 4\n    remote: { ssh: box, port: 1, host: \"a b\" }\n".into(),
        ),
        // dependencies
        (
            "config.dependency_self",
            "name: t\nservices:\n  a:\n    command: x\n    depends_on: [a]\n".into(),
        ),
        (
            "config.unknown_service",
            "name: t\nservices:\n  api:\n    command: y\n    depends_on: [qqqqqq]\n".into(),
        ),
        (
            "config.unknown_service_suggest",
            "name: t\nservices:\n  db:\n    command: x\n  api:\n    command: y\n    depends_on: [dbb]\n".into(),
        ),
        (
            "config.unknown_service",
            "name: t\nservices:\n  web:\n    command: x\nprofiles:\n  mine: [qqqqqq]\n".into(),
        ),
        (
            "config.unknown_service_suggest",
            "name: t\nservices:\n  web:\n    command: x\nprofiles:\n  mine: [webb]\n".into(),
        ),
        (
            "config.condition_log_ready",
            "name: t\nservices:\n  api:\n    command: x\n  web:\n    command: y\n    depends_on: { api: log_ready }\n".into(),
        ),
        (
            "config.condition_completed",
            "name: t\nservices:\n  db:\n    port: 5432\n  web:\n    command: y\n    depends_on: { db: completed_successfully }\n".into(),
        ),
        (
            "config.dependency_cycle",
            "name: t\nservices:\n  a:\n    command: x\n    depends_on: [b]\n  b:\n    command: y\n    depends_on: [c]\n  c:\n    command: z\n    depends_on: [a]\n".into(),
        ),
        // sizes, probes, ports
        (
            "config.size_invalid",
            "name: t\nservices:\n  a:\n    command: x\n    memory_warn: lots\n".into(),
        ),
        (
            "config.memory_warn_above_max",
            "name: t\nservices:\n  a:\n    command: x\n    memory_warn: 4G\n    max_memory: 2G\n".into(),
        ),
        (
            "config.ready_log_regex",
            "name: t\nservices:\n  a:\n    command: x\n    ready_log: \"(unclosed\"\n".into(),
        ),
        (
            "config.ready_exclusive",
            "name: t\nservices:\n  a:\n    command: x\n    ready: { http: /up, command: ok }\n".into(),
        ),
        (
            "config.port_duplicate",
            "name: t\nservices:\n  a:\n    command: x\n    port: 3000\n  b:\n    command: y\n    port: 3000\n".into(),
        ),
        // action keys
        (
            "config.key_builtin",
            "name: t\nservices:\n  a:\n    command: x\nactions:\n  - {key: q, label: Q, command: c}\n".into(),
        ),
        (
            "config.key_builtin",
            "name: t\nservices:\n  a:\n    command: x\n    open_key: D\n".into(),
        ),
        (
            "config.action_key_open_key",
            "name: t\nservices:\n  a:\n    command: x\n    open_key: a\nactions:\n  - {key: a, label: Seed, command: c}\n".into(),
        ),
        (
            "config.action_key_duplicate",
            "name: t\nservices:\n  a:\n    command: x\nactions:\n  - {key: x, label: X1, command: c}\n  - {key: x, label: X2, command: c}\n".into(),
        ),
        (
            "config.worktree_setup_unknown_action",
            "name: t\nservices:\n  a:\n    command: x\nworktree:\n  setup: \"{{action: nothing-here}}\"\n".into(),
        ),
        (
            "config.open_tunnel_without_tunnel",
            "name: t\nservices:\n  a:\n    command: x\nactions:\n  - key: o\n    label: Needs tunnel\n    command: echo hi\n    open_tunnel: true\n".into(),
        ),
        // affected
        (
            "config.affected_command_blank",
            format!("{AFFECTED_BASE}affected:\n  command: '  '\n"),
        ),
        (
            "config.affected_rule_no_match",
            format!("{AFFECTED_BASE}affected:\n  rules:\n    - {{ match: [], tasks: [Run checks] }}\n"),
        ),
        (
            "config.affected_bad_glob",
            format!("{AFFECTED_BASE}affected:\n  rules:\n    - {{ match: ['ok/**', 'a/[**'], tasks: [Run checks] }}\n"),
        ),
        (
            "config.affected_task_unknown",
            format!("{AFFECTED_BASE}affected:\n  rules:\n    - {{ match: ['ok/**'], tasks: [No such action] }}\n"),
        ),
        // deploy
        (
            "config.deploy_app_pattern",
            deploy("deploy:\n", "deploy:\n  app: Bad_App\n"),
        ),
        (
            "config.deploy_targets_empty",
            "name: s\nservices:\n  web:\n    command: x\ndeploy:\n  artifact: [dist]\n  targets: {}\n".into(),
        ),
        (
            "config.deploy_services_empty",
            "name: s\nservices:\n  web:\n    command: x\ndeploy:\n  artifact: [dist]\n  targets:\n    prod: { host: prod, services: [] }\n".into(),
        ),
        (
            "config.deploy_keep_min",
            deploy("keep: 3", "keep: 1"),
        ),
        (
            "config.deploy_path_absolute",
            deploy("dir: /opt/shop", "dir: opt/shop"),
        ),
        (
            "config.deploy_path_absolute",
            deploy("pause_marker: /opt/shop/shared/.deploy-lock", "pause_marker: shared/.lock"),
        ),
        (
            "config.deploy_link_key",
            deploy("\".env\": /opt", "\"../x\": /opt"),
        ),
        (
            "config.deploy_link_value",
            deploy("\".env\": /opt/shop/shared/.env", "\".env\": shared/.env"),
        ),
        (
            "config.deploy_service_duplicate",
            deploy("name: db,", "name: shop-api,"),
        ),
        (
            "config.deploy_restart_required",
            deploy("restart: \"pkill -HUP cron\"", "health: \"http://x\""),
        ),
        (
            "config.deploy_restart_only_command",
            deploy("kind: systemd,", "kind: systemd, restart: \"x\","),
        ),
        (
            "config.deploy_compose_file_only_compose",
            deploy("kind: systemd,", "kind: systemd, compose_file: a.yml,"),
        ),
        // warnings
        (
            "config.warn_autostart_false",
            "name: p\nservices:\n  lazy:\n    command: run-lazy\n    autostart: false\n  web:\n    command: run-web\n    depends_on: [lazy]\n".into(),
        ),
        (
            "config.warn_healthy_no_probe",
            "name: p\nservices:\n  worker:\n    command: run-worker\n  web:\n    command: run-web\n    depends_on: [worker]\n".into(),
        ),
    ]
}
