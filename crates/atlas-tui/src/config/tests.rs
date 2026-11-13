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
    assert_eq!(
        config.services["web"].command,
        Some("npm start".to_string())
    );
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
name: shop
tunnel:
  enabled: true
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: db-bastion
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
    assert_eq!(config.name, "shop");

    let tunnel = config.tunnel.unwrap();
    assert!(tunnel.is_enabled());
    assert_eq!(tunnel.local_port, 54320);
    assert_eq!(tunnel.remote_host, "localhost");
    assert_eq!(tunnel.remote_port, 5432);
    assert_eq!(tunnel.ssh_host, "db-bastion");
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
name: shop
tunnel:
  enabled: true
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: db-bastion
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
    assert_eq!(tunnel.ssh_host, "db-bastion");
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
fn load_or_detect_prefers_the_file_and_reports_its_path() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: mine\nservices:\n  web:\n    command: x\n",
    );
    // A package.json that would detect a different service must not matter.
    write_config(
        &dir,
        "package.json",
        r#"{"scripts":{"dev":"vite --port 5173"}}"#,
    );
    let (cfg, src) = load_or_detect(dir.path()).unwrap();
    assert_eq!(cfg.name, "mine");
    assert!(cfg.services.contains_key("web") && !cfg.services.contains_key("dev"));
    assert_eq!(src, ConfigSource::File(dir.path().join("atlas.yaml")));
}

#[test]
fn load_or_detect_builds_the_node_config_in_memory_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "package.json",
        r#"{"name":"x","scripts":{"dev":"vite --port 5173","build":"vite build"}}"#,
    );
    let (cfg, src) = load_or_detect(dir.path()).unwrap();
    assert_eq!(src, ConfigSource::Detected);
    assert_eq!(
        cfg.services["dev"].command.as_deref(),
        Some("vite --port 5173")
    );
    assert_eq!(cfg.services["dev"].port, Some(5173));
    assert!(
        !dir.path().join("atlas.yaml").exists(),
        "detection must never write"
    );
}

#[test]
fn load_or_detect_of_an_empty_folder_loads_with_no_services() {
    let dir = TempDir::new().unwrap();
    let (cfg, src) = load_or_detect(dir.path()).unwrap();
    assert_eq!(src, ConfigSource::Detected);
    assert!(cfg.services.is_empty(), "{:?}", cfg.services.keys());
    assert!(!cfg.name.is_empty());
}

// Incident: Rust/Go/Python folders loaded with no services ("found nothing to run"). The detected YAML must also
// survive the real parser: a service name or command that breaks YAML would turn a hint into a broken config.
#[test]
fn load_or_detect_runs_a_rust_go_or_django_folder_through_the_real_parser() {
    let cargo = TempDir::new().unwrap();
    write_config(&cargo, "Cargo.toml", "[package]\nname = \"my-app\"\n");
    fs::create_dir_all(cargo.path().join("src")).unwrap();
    write_config(&cargo, "src/main.rs", "fn main() {}\n");
    let (cfg, src) = load_or_detect(cargo.path()).unwrap();
    assert_eq!(src, ConfigSource::Detected);
    assert_eq!(cfg.services["my-app"].command.as_deref(), Some("cargo run"));

    let go = TempDir::new().unwrap();
    write_config(&go, "go.mod", "module example.com/svc\n");
    write_config(&go, "main.go", "package main\n");
    let (cfg, _) = load_or_detect(go.path()).unwrap();
    assert_eq!(cfg.services["svc"].command.as_deref(), Some("go run ."));

    let django = TempDir::new().unwrap();
    write_config(&django, "manage.py", "");
    let (cfg, _) = load_or_detect(django.path()).unwrap();
    let py = if cfg!(windows) { "python" } else { "python3" };
    assert_eq!(
        cfg.services["web"].command.as_deref(),
        Some(format!("{py} manage.py runserver 8000").as_str())
    );
    assert_eq!(cfg.services["web"].port, Some(8000));
}

// Incident class: `initial_config` wrote the folder name and each script's name and command into the YAML with a
// bare `format!`. A folder called `my: app`, `#todo` or `123`, or a script with `: ` or ` #` in its command, made the
// zero-config project fail with "detected config ... is invalid" (or parse as another type), and "Save as
// atlas.yaml" would have written that broken file to disk.
#[test]
fn load_or_detect_survives_folder_names_that_are_special_in_yaml() {
    let names = [
        "my: app",
        "#todo",
        "app #1",
        "[draft]",
        "{x}",
        "@scope",
        "*star",
        "& co",
        "!bang",
        "'q'",
        "\"dq\"",
        "- item",
        "true",
        "null",
        "123",
        "1.5",
        "~",
        "%dir",
        "> fold",
        "| lit",
        "trailing:",
        "a: b: c",
        "with space",
        "ünïcödé-项目",
        "emoji-🚀",
        "key=value",
        "back`tick",
    ];
    // Windows cannot name a folder with `<>:"/\|?*`; those cases exist only elsewhere.
    let legal = |n: &str| !cfg!(windows) || !n.contains(['<', '>', ':', '"', '|', '?', '*']);
    for name in names.into_iter().filter(|n| legal(n)) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"dev":"vite --port 5173"}}"#,
        )
        .unwrap();
        let (cfg, src) = load_or_detect(&dir).unwrap_or_else(|e| panic!("{name:?}: {e:#}"));
        assert_eq!(src, ConfigSource::Detected, "{name:?}");
        assert_eq!(cfg.name, name, "the project keeps the folder's exact name");
        assert_eq!(cfg.services["dev"].port, Some(5173), "{name:?}");
    }
}

#[test]
fn load_or_detect_survives_scripts_whose_name_or_command_is_special_in_yaml() {
    let scripts = [
        ("dev", "vite --host: 0.0.0.0 # local"),
        ("dev:watch", "*glob* run"),
        ("start", "& background"),
        ("start:prod", "!important"),
        ("serve", "{json} here"),
        ("dev:2", "[a, b]"),
        ("dev:3", "'single' and \"double\""),
        ("dev:4", "@scope/pkg run"),
        ("dev:5", "%env% node x.js"),
        ("dev:6", "node x.js\\ntwo"),
        ("dev:7", "node x.js | tee out.log"),
        ("serve: x", "node a.js"),
        ("serve #1", "node b.js"),
        ("dev:#2", "node c.js"),
    ];
    let root = TempDir::new().unwrap();
    let body: serde_json::Value = serde_json::json!({ "scripts": scripts.iter().cloned().collect::<std::collections::BTreeMap<_, _>>() });
    fs::write(root.path().join("package.json"), body.to_string()).unwrap();
    let (cfg, src) = load_or_detect(root.path()).unwrap_or_else(|e| panic!("{e:#}"));
    assert_eq!(src, ConfigSource::Detected);
    for (name, cmd) in scripts {
        let svc = cfg
            .services
            .get(name)
            .unwrap_or_else(|| panic!("service {name:?} lost: {:?}", cfg.services.keys()));
        assert_eq!(svc.command.as_deref(), Some(cmd), "{name:?}");
    }
}

// Detected services used to come out in HashMap order: the same folder got different port numbers (`4010 + index`)
// and a different service order from one launch to the next.
#[test]
fn detected_services_come_out_in_a_stable_order_with_stable_ports() {
    let root = TempDir::new().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{"scripts":{"start":"a","dev":"b","dev:api":"c","dev:web":"d","serve":"e","start:prod":"f"}}"#,
    )
    .unwrap();
    let (first, d) = atlas_core::domain::project::initial_config(root.path());
    let names: Vec<_> = d.services.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["dev", "dev:api", "dev:web", "serve", "start", "start:prod"]
    );
    for _ in 0..20 {
        let (again, _) = atlas_core::domain::project::initial_config(root.path());
        assert_eq!(again, first, "same folder, same file, every time");
    }
    let (cfg, _) = load_or_detect(root.path()).unwrap();
    assert_eq!(
        cfg.services["dev"].port,
        Some(4010),
        "ports follow the sorted order"
    );
    assert_eq!(cfg.services["dev:api"].port, Some(4011));
}

#[test]
fn load_or_detect_still_layers_atlas_local_yaml_on_a_detected_config() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "package.json", r#"{"scripts":{"dev":"vite"}}"#);
    write_config(
        &dir,
        "atlas.local.yaml",
        "services:\n  dev:\n    port: 4999\n",
    );
    let (cfg, src) = load_or_detect(dir.path()).unwrap();
    assert_eq!(src, ConfigSource::Detected);
    assert_eq!(cfg.services["dev"].port, Some(4999));
}

#[test]
fn load_or_detect_propagates_an_invalid_atlas_yaml_instead_of_detecting() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "package.json", r#"{"scripts":{"dev":"vite"}}"#);
    write_config(&dir, "atlas.yaml", "services: [not, a, map]\n");
    assert!(load_or_detect(dir.path()).is_err());
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

#[test]
fn test_pre_command_local_override_disables() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: test
pre_command: "bun install"
services:
  app:
    command: cargo run
"#,
    );
    write_config(
        &dir,
        "atlas.local.yaml",
        r#"
pre_command: ""
"#,
    );

    let config = load(dir.path()).unwrap();
    assert_eq!(
        config.pre_command, None,
        "empty string in local override must disable pre_command"
    );
}

#[test]
fn test_pre_command_when_parses_and_local_overrides() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: test
pre_command: "bun install"
pre_command_when: lockfile
services:
  app:
    command: cargo run
"#,
    );
    assert_eq!(
        load(dir.path()).unwrap().pre_command_when,
        Some(PreCommandWhen::Lockfile)
    );

    write_config(&dir, "atlas.local.yaml", "pre_command_when: always\n");
    assert_eq!(
        load(dir.path()).unwrap().pre_command_when,
        Some(PreCommandWhen::Always)
    );
}

fn errors_for(yaml: &str) -> Vec<String> {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "atlas.yaml", yaml);
    let (_, errs) = load_checked(dir.path()).unwrap();
    errs.iter().map(ToString::to_string).collect()
}

fn assert_has(errs: &[String], needle: &str) {
    assert!(
        errs.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}, got {errs:#?}"
    );
}

#[test]
fn test_unknown_key_suggests_closest_field_with_path() {
    let errs =
        errors_for("name: t\nservices:\n  web:\n    command: x\n    helth: http://localhost:1\n");
    assert_eq!(
        errs,
        ["services.web.helth: unknown field (did you mean 'health'?)"]
    );
}

#[test]
fn test_unknown_key_without_close_match_has_no_suggestion() {
    let errs = errors_for("name: t\nservices:\n  web:\n    command: x\n    zzzzzzzz: 1\n");
    assert_eq!(errs, ["services.web.zzzzzzzz: unknown field"]);
}

#[test]
fn test_unknown_key_inside_action_and_toplevel() {
    let errs = errors_for(
        "name: t\nservicess: {}\nservices:\n  a:\n    command: x\nactions:\n  - key: z\n    label: Z\n    comand: y\n    command: y\n",
    );
    assert_has(&errs, "servicess: unknown field (did you mean 'services'?)");
    assert_has(
        &errs,
        "actions[0].comand: unknown field (did you mean 'command'?)",
    );
}

#[test]
fn test_unknown_key_in_local_override_names_the_file() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: t\nservices:\n  a:\n    command: x\n",
    );
    write_config(&dir, "atlas.local.yaml", "services:\n  a:\n    comand: y\n");
    let (_, errs) = load_checked(dir.path()).unwrap();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].file, "atlas.local.yaml");
    assert!(errs[0].to_string().contains("services.a.comand"));
}

#[test]
fn test_load_fails_fast_with_all_errors() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: t\nservices:\n  a:\n    command: x\n    depends_on: [nope]\n    helth: y\n",
    );
    let msg = load(dir.path()).unwrap_err().to_string();
    assert!(msg.contains("2 config errors"), "{msg}");
    assert!(msg.contains("unknown service 'nope'"), "{msg}");
    assert!(msg.contains("services.a.helth"), "{msg}");
}

#[test]
fn test_missing_dependency_suggests_service_name() {
    let errs = errors_for(
        "name: t\nservices:\n  db:\n    command: x\n  api:\n    command: y\n    depends_on: [dbb]\n",
    );
    assert_has(
        &errs,
        "services.api.depends_on.dbb: unknown service 'dbb' (did you mean 'db'?)",
    );
}

#[test]
fn test_dependency_cycle_is_reported_once_with_path() {
    let errs = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    depends_on: [b]\n  b:\n    command: y\n    depends_on: [c]\n  c:\n    command: z\n    depends_on: [a]\n",
    );
    let cycles: Vec<_> = errs.iter().filter(|e| e.contains("cycle")).collect();
    assert_eq!(cycles.len(), 1, "{errs:#?}");
    assert!(cycles[0].contains("a -> b -> c -> a"), "{}", cycles[0]);
}

#[test]
fn test_self_dependency_is_rejected_not_a_cycle() {
    let errs = errors_for("name: t\nservices:\n  a:\n    command: x\n    depends_on: [a]\n");
    assert_has(&errs, "cannot depend on itself");
    assert!(!errs.iter().any(|e| e.contains("cycle")));
}

#[test]
fn test_invalid_condition_name_is_a_parse_error() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: t\nservices:\n  db:\n    command: x\n  a:\n    command: y\n    depends_on: { db: ready }\n",
    );
    let err = format!("{:#}", load_checked(dir.path()).unwrap_err());
    assert!(
        err.contains("ready") || err.contains("did not match"),
        "{err}"
    );
}

#[test]
fn test_log_ready_requires_ready_log_on_target() {
    let errs = errors_for(
        "name: t\nservices:\n  api:\n    command: x\n  web:\n    command: y\n    depends_on: { api: log_ready }\n",
    );
    assert_has(
        &errs,
        "services.web.depends_on.api: condition 'log_ready' requires services.api.ready_log",
    );

    let ok = errors_for(
        "name: t\nservices:\n  api:\n    command: x\n    ready_log: listening\n  web:\n    command: y\n    depends_on: { api: log_ready }\n",
    );
    assert!(ok.is_empty(), "{ok:#?}");
}

#[test]
fn test_completed_successfully_target_needs_a_command() {
    let errs = errors_for(
        "name: t\nservices:\n  db:\n    port: 5432\n  web:\n    command: y\n    depends_on: { db: completed_successfully }\n",
    );
    assert_has(&errs, "completed_successfully' needs services.db.command");
}

#[test]
fn test_invalid_ready_log_regex_is_reported() {
    let errs = errors_for("name: t\nservices:\n  a:\n    command: x\n    ready_log: '('\n");
    assert_has(&errs, "services.a.ready_log: invalid regex");
}

#[test]
fn memory_budget_sizes_parse_like_max_memory_and_bad_ones_name_the_key() {
    // A size is a string like `max_memory`'s; a bare number is bytes only when quoted.
    for ok in ["1.5GB", "512M", "3G", "800mb", "'2048'"] {
        let errs = errors_for(&format!(
            "name: t\nservices:\n  a:\n    command: x\n    memory_warn: {ok}\n"
        ));
        assert!(errs.is_empty(), "{ok}: {errs:#?}");
    }
    let errs = errors_for("name: t\nservices:\n  a:\n    command: x\n    memory_warn: lots\n");
    assert_has(&errs, "services.a.memory_warn: 'lots' is not a size");
    let errs = errors_for("name: t\nservices:\n  a:\n    command: x\n    max_memory: 0M\n");
    assert_has(&errs, "services.a.max_memory: '0M' is not a size");
    // Unquoted `2048` is a YAML integer: rejected at load, for both keys alike.
    for key in ["memory_warn", "max_memory"] {
        let dir = TempDir::new().unwrap();
        write_config(
            &dir,
            "atlas.yaml",
            &format!("name: t\nservices:\n  a:\n    command: x\n    {key}: 2048\n"),
        );
        let e = load_checked(dir.path()).expect_err("integer must not load");
        assert!(e.to_string().contains("expected a string"), "{key}: {e}");
    }
}

/// `warn` at or above `max` would make the soft budget unreachable: the config is wrong.
#[test]
fn memory_warn_must_be_below_max_memory() {
    let errs = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    memory_warn: 4G\n    max_memory: 2G\n",
    );
    assert_has(
        &errs,
        "services.a.memory_warn: must be below max_memory (2G)",
    );
    let equal = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    memory_warn: 2G\n    max_memory: 2048M\n",
    );
    assert_has(&equal, "services.a.memory_warn: must be below max_memory");
    let ok = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    memory_warn: 1.5GB\n    max_memory: 2G\n",
    );
    assert!(ok.is_empty(), "{ok:#?}");
}

#[test]
fn on_memory_max_takes_restart_or_notify_and_defaults_to_restart() {
    let cfg = |extra: &str| {
        let dir = TempDir::new().unwrap();
        write_config(
            &dir,
            "atlas.yaml",
            &format!("name: t\nservices:\n  a:\n    command: x\n    max_memory: 2G\n{extra}"),
        );
        load(dir.path())
    };
    assert_eq!(
        cfg("").unwrap().services["a"].on_memory_max,
        OnMemoryMax::Restart
    );
    assert_eq!(
        cfg("    on_memory_max: notify\n").unwrap().services["a"].on_memory_max,
        OnMemoryMax::Notify
    );
    assert!(cfg("    on_memory_max: explode\n").is_err());
}

#[test]
fn test_duplicate_port_flagged_but_disabled_service_exempt() {
    let errs = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    port: 3000\n  b:\n    command: y\n    port: 3000\n",
    );
    assert_has(&errs, "services.b.port: port 3000 is also used by 'a'");

    let ok = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    port: 3000\n  b:\n    command: y\n    port: 3000\n    enabled: false\n",
    );
    assert!(ok.is_empty(), "{ok:#?}");
}

#[test]
fn test_action_keys_unique_and_not_builtin() {
    let errs = errors_for(
        "name: t\nservices:\n  a:\n    command: x\nactions:\n  - {key: q, label: Q, command: c}\n  - {key: x, label: X1, command: c}\n  - {key: x, label: X2, command: c}\n",
    );
    assert_has(&errs, "actions[0].key: key 'q' is a built-in dashboard key");
    assert_has(
        &errs,
        "actions[2].key: key 'x' is already used by action 'X1'",
    );
    assert_eq!(errs.len(), 2, "{errs:#?}");
}

/// `D` (detach while attached) is reserved: an action or open_key on it would never fire.
/// Lowercase `d` stays free (the ac project binds it to an action).
#[test]
fn test_detach_key_is_reserved_for_actions_and_open_keys() {
    let errs = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    open_key: D\nactions:\n  - {key: D, label: Dd, command: c}\n",
    );
    assert_has(&errs, "actions[0].key: key 'D' is a built-in dashboard key");
    assert_has(&errs, "key 'D' is a built-in dashboard key");

    let ok = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    open_key: a\nactions:\n  - {key: d, label: Seed, command: c}\n",
    );
    assert!(ok.is_empty(), "{ok:#?}");
}

#[test]
fn test_action_key_colliding_with_open_key_is_flagged() {
    let errs = errors_for(
        "name: t\nservices:\n  a:\n    command: x\n    open_key: w\nactions:\n  - {key: w, label: W, command: c}\n",
    );
    assert_has(&errs, "already the open_key of service 'a'");
}

#[test]
fn test_depends_on_list_and_map_forms_parse_to_conditions() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: t\nservices:\n  db:\n    command: x\n  m:\n    command: x\n  l:\n    command: x\n    depends_on: [db]\n  mp:\n    command: x\n    depends_on: { db: started, m: completed_successfully }\n",
    );
    let (cfg, errs) = load_checked(dir.path()).unwrap();
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(
        cfg.services["l"].deps(),
        [("db".into(), Condition::Healthy)]
    );
    assert_eq!(
        cfg.services["mp"].deps(),
        [
            ("db".into(), Condition::Started),
            ("m".into(), Condition::CompletedSuccessfully)
        ]
    );
}

#[test]
fn test_restart_defaults_keep_old_behavior() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: t\nservices:\n  a:\n    command: x\n",
    );
    let (cfg, errs) = load_checked(dir.path()).unwrap();
    assert!(errs.is_empty());
    let a = &cfg.services["a"];
    assert_eq!(a.restart, RestartPolicy::No);
    assert_eq!(a.max_restarts, None);
    assert!(!a.oneshot && a.ready_log.is_none() && a.deps().is_empty());
}

#[test]
fn test_schema_is_valid_json_listing_new_fields() {
    let schema: serde_json::Value = serde_json::from_str(&json_schema()).unwrap();
    let props = &schema["definitions"]["ServiceDef"]["properties"];
    for key in [
        "restart",
        "max_restarts",
        "ready_log",
        "oneshot",
        "depends_on",
    ] {
        assert!(props.get(key).is_some(), "schema missing {key}");
    }
}

#[test]
fn test_open_tunnel_without_tunnel_block_is_a_config_error() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: t
services:
  a:
    command: x
actions:
  - key: o
    label: Needs tunnel
    command: echo hi
    open_tunnel: true
"#,
    );
    let err = load(dir.path()).unwrap_err().to_string();
    assert!(
        err.contains("actions[0].open_tunnel") && err.contains("no `tunnel:` block"),
        "{err}"
    );
}

#[test]
fn test_open_tunnel_with_disabled_tunnel_block_is_valid() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: t
tunnel:
  enabled: false
  local_port: 54320
  remote_host: localhost
  remote_port: 5432
  ssh_host: h
services:
  a:
    command: x
actions:
  - key: o
    label: Needs tunnel
    command: echo hi
    open_tunnel: true
"#,
    );
    let (cfg, errs) = load_checked(dir.path()).unwrap();
    assert!(errs.is_empty(), "{errs:?}");
    assert!(cfg.actions[0].open_tunnel);
}

#[test]
fn start_timeout_and_autostart_parse_with_defaults() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: p\nservices:\n  a:\n    command: x\n  b:\n    command: y\n    autostart: false\n    start_timeout: 5\n",
    );
    let c = load(dir.path()).unwrap();
    assert!(c.services["a"].is_autostart());
    assert_eq!(
        c.services["a"].start_timeout_secs(),
        DEFAULT_START_TIMEOUT_SECS
    );
    assert!(!c.services["b"].is_autostart());
    assert_eq!(c.services["b"].start_timeout_secs(), 5);
}

#[test]
fn warns_when_healthy_targets_probe_less_service_but_still_loads() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: p
services:
  worker:
    command: run-worker
  probed:
    command: run-api
    port: 3000
  logged:
    command: run-x
    ready_log: ready
  lazy:
    command: run-lazy
    autostart: false
  web:
    command: run-web
    depends_on: [worker, probed, logged, lazy]
  after:
    command: run-after
    depends_on: { worker: started }
"#,
    );
    let (config, errors) = load_checked(dir.path()).unwrap();
    assert!(errors.is_empty(), "warnings are not errors: {errors:?}");
    let w = warnings(&config);
    let paths: Vec<&str> = w.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "services.web.depends_on.worker",
            "services.web.depends_on.lazy",
            "services.web.depends_on.lazy",
        ],
        "{w:?}"
    );
    assert!(w.iter().any(|e| e.message.contains("autostart: false")));
    assert!(
        w.iter()
            .any(|e| e.message.contains("no port, health or ready_log"))
    );
}

#[test]
fn profiles_parse_and_local_overrides_by_name() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: p\nservices:\n  server:\n    command: a\n  worker:\n    command: b\n  web:\n    command: c\nprofiles:\n  backend: [server, worker]\n  front: [web]\n",
    );
    write_config(
        &dir,
        "atlas.local.yaml",
        "profiles:\n  backend: [server]\n  all: [server, worker, web]\n",
    );
    let c = load(dir.path()).unwrap();
    assert_eq!(c.profiles["backend"], ["server"]);
    assert_eq!(c.profiles["front"], ["web"]);
    assert_eq!(c.profiles["all"].len(), 3);
}

#[test]
fn profile_with_unknown_service_is_a_config_error_with_suggestion() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: p\nservices:\n  server:\n    command: a\nprofiles:\n  backend: [servr, ghost]\n",
    );
    let (_, errors) = load_checked(dir.path()).unwrap();
    let msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    assert!(
        msgs.iter().any(|m| m.starts_with("profiles.backend[0]:")
            && m.contains("unknown service 'servr'")
            && m.contains("did you mean 'server'?")),
        "{msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("unknown service 'ghost'")),
        "{msgs:?}"
    );
    assert!(load(dir.path()).is_err());
}

#[test]
fn local_profile_with_unknown_service_is_a_config_error() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        "name: p\nservices:\n  server:\n    command: a\n",
    );
    write_config(&dir, "atlas.local.yaml", "profiles:\n  mine: [nope]\n");
    let (_, errors) = load_checked(dir.path()).unwrap();
    assert!(
        errors.iter().any(|e| e.path == "profiles.mine[0]"),
        "{errors:?}"
    );
}

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

#[test]
fn deploy_block_parses_all_kinds() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "atlas.yaml", DEPLOY_OK);
    let (cfg, errors) = load_checked(dir.path()).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let t = &cfg.deploy.unwrap().targets["prod"];
    let kinds: Vec<_> = t.services.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            DeployKind::Systemd,
            DeployKind::Compose,
            DeployKind::Command
        ]
    );
    assert_eq!(
        t.hooks.as_ref().unwrap().post_swap.as_deref(),
        Some("./sync.sh")
    );
    assert_eq!(t.link.as_ref().unwrap()[".env"], "/opt/shop/shared/.env");
}

#[test]
fn deploy_semantic_rejections() {
    let bad = DEPLOY_OK
        .replace("keep: 3", "keep: 1")
        .replace("restart: \"pkill -HUP cron\"", "health: \"http://x\"")
        .replace("\".env\": /opt", "\"../x\": /opt")
        .replace("name: db,", "name: shop-api,");
    let dir = TempDir::new().unwrap();
    write_config(&dir, "atlas.yaml", &bad);
    let (_, errors) = load_checked(dir.path()).unwrap();
    let paths: Vec<_> = errors.iter().map(|e| e.path.as_str()).collect();
    for want in [
        "deploy.targets.prod.keep",
        "deploy.targets.prod.services[2].restart",
        "deploy.targets.prod.link.../x",
        "deploy.targets.prod.services[1].name",
    ] {
        assert!(paths.contains(&want), "missing {want} in {paths:?}");
    }
}

#[test]
fn deploy_unknown_key_rejected() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "atlas.yaml", &DEPLOY_OK.replace("keep: 3", "kep: 3"));
    let (_, errors) = load_checked(dir.path()).unwrap();
    assert!(errors.iter().any(|e| e.path.contains("kep")), "{errors:?}");
}

#[test]
fn port_offset_shifts_ports_env_and_urls() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        "atlas.yaml",
        r#"
name: p
services:
  api:
    command: run
    port: 3000
    health: http://localhost:3000/health
    env: { PORT: "3000", WEB: "5173", OTHER: "30000" }
  web:
    command: run
    port: 5173
    open_url: http://localhost:5173/x
"#,
    );
    let (mut cfg, _) = load_checked(dir.path()).unwrap();
    crate::config::apply_port_offset(&mut cfg, 100);
    let api = &cfg.services["api"];
    assert_eq!(api.port, Some(3100));
    assert_eq!(api.health.as_deref(), Some("http://localhost:3100/health"));
    assert_eq!(api.env["PORT"], "3100");
    assert_eq!(api.env["WEB"], "5273");
    assert_eq!(api.env["OTHER"], "30000");
    assert_eq!(
        cfg.services["web"].open_url.as_deref(),
        Some("http://localhost:5273/x")
    );
    assert_eq!(cfg.port_offset, 100);
}
