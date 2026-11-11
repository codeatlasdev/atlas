/// Reserved port block for atlas-managed local services.
/// Use 4010–4099 for all services defined in atlas.yaml to avoid
/// collisions with common defaults (3000, 8080, etc.).
pub const PORT_BLOCK_START: u16 = 4010;
pub const PORT_BLOCK_END: u16 = 4099;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetection {
    pub language: String,
    pub framework: Option<String>,
    pub package_manager: Option<String>,
    pub scripts: HashMap<String, String>,
    pub services: Vec<DetectedService>,
    pub deploy_strategy: Option<String>,
    pub monorepo: bool,
}

impl Default for ProjectDetection {
    fn default() -> Self {
        Self {
            language: "unknown".into(),
            framework: None,
            package_manager: None,
            scripts: HashMap::new(),
            services: Vec::new(),
            deploy_strategy: None,
            monorepo: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedService {
    pub name: String,
    pub command: String,
    pub port: Option<u16>,
}

pub fn detect_project(path: &Path) -> ProjectDetection {
    let mut detection = ProjectDetection::default();

    // Rust project
    if path.join("Cargo.toml").exists() {
        detection.language = "rust".into();
        detection.package_manager = Some("cargo".into());
        if let Ok(content) = std::fs::read_to_string(path.join("Cargo.toml")) {
            if content.contains("[workspace]") {
                detection.monorepo = true;
            }
            if content.contains("axum") {
                detection.framework = Some("axum".into());
            } else if content.contains("actix") {
                detection.framework = Some("actix".into());
            }
        }
    }

    // Node/TypeScript project
    if path.join("package.json").exists() {
        detection.language = "typescript".into();

        if path.join("bun.lock").exists() || path.join("bun.lockb").exists() {
            detection.package_manager = Some("bun".into());
        } else if path.join("pnpm-lock.yaml").exists() {
            detection.package_manager = Some("pnpm".into());
        } else if path.join("yarn.lock").exists() {
            detection.package_manager = Some("yarn".into());
        } else {
            detection.package_manager = Some("npm".into());
        }

        if let Ok(content) = std::fs::read_to_string(path.join("package.json"))
            && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let deps = pkg.get("dependencies").and_then(|d| d.as_object());
            let dev_deps = pkg.get("devDependencies").and_then(|d| d.as_object());

            let has_dep = |name: &str| -> bool {
                deps.is_some_and(|d| d.contains_key(name))
                    || dev_deps.is_some_and(|d| d.contains_key(name))
            };

            detection.framework = if has_dep("next") {
                Some("nextjs".into())
            } else if has_dep("astro") {
                Some("astro".into())
            } else if has_dep("elysia") {
                Some("elysia".into())
            } else if has_dep("@tanstack/start") {
                Some("tanstack-start".into())
            } else if has_dep("nuxt") {
                Some("nuxt".into())
            } else if has_dep("svelte") || has_dep("@sveltejs/kit") {
                Some("sveltekit".into())
            } else if has_dep("vue") {
                Some("vue".into())
            } else if has_dep("react") {
                Some("react".into())
            } else if has_dep("hono") {
                Some("hono".into())
            } else if has_dep("express") {
                Some("express".into())
            } else {
                None
            };

            if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
                for (name, cmd) in scripts {
                    if let Some(c) = cmd.as_str() {
                        detection.scripts.insert(name.clone(), c.to_string());
                    }
                }
            }
        }

        // Detect services from scripts
        let scripts: Vec<(String, String)> = detection
            .scripts
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, cmd) in &scripts {
            if name == "dev"
                || name.starts_with("dev:")
                || name == "start"
                || name.starts_with("start:")
                || name.contains("serve")
            {
                detection.services.push(DetectedService {
                    name: name.clone(),
                    command: cmd.clone(),
                    port: extract_port(cmd),
                });
            }
        }
    }

    detect_cargo_service(path, &mut detection);

    // Python project
    if path.join("pyproject.toml").exists() || path.join("requirements.txt").exists() {
        if detection.language == "unknown" {
            detection.language = "python".into();
        }
        if path.join("uv.lock").exists() {
            detection.package_manager = Some("uv".into());
        } else if path.join("poetry.lock").exists() {
            detection.package_manager = Some("poetry".into());
        } else if path.join("Pipfile.lock").exists() {
            detection.package_manager = Some("pipenv".into());
        } else {
            detection.package_manager = Some("pip".into());
        }
        if let Ok(content) = std::fs::read_to_string(path.join("pyproject.toml")) {
            detection.framework = if content.contains("fastapi") {
                Some("fastapi".into())
            } else if content.contains("django") {
                Some("django".into())
            } else if content.contains("flask") {
                Some("flask".into())
            } else {
                None
            };
        }
    }
    detect_python_service(path, &mut detection);

    // Go project
    if path.join("go.mod").exists() {
        if detection.language == "unknown" {
            detection.language = "go".into();
        }
        detection.package_manager = Some("go".into());
        if let Ok(content) = std::fs::read_to_string(path.join("go.mod")) {
            detection.framework = if content.contains("github.com/gin-gonic/gin") {
                Some("gin".into())
            } else if content.contains("github.com/gofiber/fiber") {
                Some("fiber".into())
            } else if content.contains("github.com/labstack/echo") {
                Some("echo".into())
            } else {
                None
            };
        }
        detect_go_service(path, &mut detection);
    }

    // Monorepo detection
    if path.join("turbo.json").exists()
        || path.join("pnpm-workspace.yaml").exists()
        || path.join("lerna.json").exists()
        || path.join("nx.json").exists()
    {
        detection.monorepo = true;
    }

    // Deploy strategy detection
    detection.deploy_strategy =
        if path.join("Dockerfile").exists() || path.join("docker-compose.yml").exists() {
            Some("docker".into())
        } else if path.join("fly.toml").exists() {
            Some("fly".into())
        } else if path.join("vercel.json").exists() {
            Some("vercel".into())
        } else if path.join("netlify.toml").exists() {
            Some("netlify".into())
        } else if path.join("render.yaml").exists() {
            Some("render".into())
        } else {
            None
        };

    detection
}

/// `name = "value"` on one line (TOML basic string); `None` for anything fancier.
fn quoted_value(line: &str, key: &str) -> Option<String> {
    let rest = line
        .trim()
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?;
    let v = rest.trim().strip_prefix('"')?;
    Some(v[..v.find('"')?].to_owned())
}

/// A service key that is safe as a YAML mapping key (`my-app`, `api_2`); anything else falls back to `fallback`.
fn service_key(raw: &str, fallback: &str) -> String {
    let ok = !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        raw.to_owned()
    } else {
        fallback.to_owned()
    }
}

/// The one binary `cargo run` would build, when the answer is unambiguous: a `[package]` (not a virtual workspace)
/// with exactly one bin (`[[bin]]`, `src/bin/*`, or `src/main.rs`). Anything else gets no service.
fn detect_cargo_service(path: &Path, detection: &mut ProjectDetection) {
    let Ok(content) = std::fs::read_to_string(path.join("Cargo.toml")) else {
        return;
    };
    let (mut section, mut package_name) = (String::new(), None);
    let mut bins: Vec<String> = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_owned();
            if t == "[[bin]]" {
                bins.push(String::new());
            }
        } else if section == "[package]" && package_name.is_none() {
            package_name = quoted_value(t, "name");
        } else if section == "[[bin]]"
            && let Some(n) = quoted_value(t, "name")
            && let Some(last) = bins.last_mut()
        {
            *last = n;
        }
    }
    // A virtual workspace has no `[package]`: `cargo run` has nothing to run there.
    let Some(package_name) = package_name else {
        return;
    };
    // A `[[bin]]` without a name is not understood: skip rather than guess.
    if bins.iter().any(String::is_empty) {
        return;
    }
    if path.join("src/main.rs").is_file() && !bins.contains(&package_name) {
        bins.push(package_name);
    }
    if let Ok(dir) = std::fs::read_dir(path.join("src/bin")) {
        for e in dir.flatten() {
            let p = e.path();
            let is_bin = (p.is_file() && p.extension().is_some_and(|x| x == "rs"))
                || (p.is_dir() && p.join("main.rs").is_file());
            if is_bin
                && let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().into_owned())
                && !bins.contains(&stem)
            {
                bins.push(stem);
            }
        }
    }
    if let [name] = bins.as_slice() {
        detection.services.push(DetectedService {
            name: service_key(name, "app"),
            command: "cargo run".into(),
            port: None,
        });
    }
}

/// `go run .` when the module root holds a `package main` (a non-test `.go` file declaring it). A `cmd/*` layout or a
/// library root gets no service: which program to run is the user's call.
fn detect_go_service(path: &Path, detection: &mut ProjectDetection) {
    let Ok(dir) = std::fs::read_dir(path) else {
        return;
    };
    let has_main = dir.flatten().any(|e| {
        let p = e.path();
        p.is_file()
            && p.extension().is_some_and(|x| x == "go")
            && !p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("_test.go"))
            && std::fs::read_to_string(&p)
                .is_ok_and(|s| s.lines().any(|l| l.trim() == "package main"))
    });
    if !has_main {
        return;
    }
    let module = std::fs::read_to_string(path.join("go.mod"))
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("module ")
                    .map(|m| m.trim().to_owned())
            })
        })
        .and_then(|m| m.rsplit('/').next().map(str::to_owned))
        .unwrap_or_default();
    detection.services.push(DetectedService {
        name: service_key(&module, "app"),
        command: "go run .".into(),
        port: None,
    });
}

/// Python dev servers, only when the command is unambiguous: Django's `manage.py`, or FastAPI + uvicorn declared in
/// `pyproject.toml` with exactly one of `main.py`/`app.py`/`api.py` defining `app`.
fn detect_python_service(path: &Path, detection: &mut ProjectDetection) {
    if path.join("manage.py").is_file() {
        detection.services.push(DetectedService {
            name: "web".into(),
            command: "python manage.py runserver".into(),
            port: None,
        });
        return;
    }
    let Ok(pyproject) = std::fs::read_to_string(path.join("pyproject.toml")) else {
        return;
    };
    if !(pyproject.contains("fastapi") && pyproject.contains("uvicorn")) {
        return;
    }
    let defines_app = |m: &str| {
        std::fs::read_to_string(path.join(format!("{m}.py"))).is_ok_and(|s| {
            s.lines()
                .any(|l| l.starts_with("app =") || l.starts_with("app:"))
        })
    };
    let modules: Vec<&str> = ["main", "app", "api"]
        .into_iter()
        .filter(|m| defines_app(m))
        .collect();
    if let [module] = modules.as_slice() {
        let name = pyproject
            .lines()
            .scan(String::new(), |section, l| {
                if l.trim().starts_with('[') {
                    *section = l.trim().to_owned();
                }
                Some((section.clone(), l))
            })
            .find_map(|(s, l)| {
                (s == "[project]")
                    .then(|| quoted_value(l, "name"))
                    .flatten()
            })
            .unwrap_or_default();
        detection.services.push(DetectedService {
            name: service_key(&name, "api"),
            command: format!("uvicorn {module}:app --reload"),
            port: None,
        });
    }
}

/// What was detected, for the UI: `Rust project (Cargo.toml, axum)`; `None` when nothing was recognised.
pub fn detection_label(root: &Path, d: &ProjectDetection) -> Option<String> {
    let (kind, manifest) = match d.language.as_str() {
        "rust" => ("Rust", "Cargo.toml"),
        "typescript" => ("Node", "package.json"),
        "go" => ("Go", "go.mod"),
        "python" => (
            "Python",
            ["pyproject.toml", "requirements.txt"]
                .into_iter()
                .find(|f| root.join(f).is_file())?,
        ),
        _ => return None,
    };
    Some(match &d.framework {
        Some(fw) => format!("{kind} project ({manifest}, {fw})"),
        None => format!("{kind} project ({manifest})"),
    })
}

/// The port a dev command announces: `--port N`, `-p N`, `--port=N`, `PORT=N` or `.listen(N)`.
pub fn extract_port(cmd: &str) -> Option<u16> {
    if let Some(i) = cmd.find(".listen(") {
        let digits: String = cmd[i + ".listen(".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(port) = digits.parse::<u16>() {
            return Some(port);
        }
    }
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "--port" || *part == "-p")
            && i + 1 < parts.len()
            && let Ok(port) = parts[i + 1].parse::<u16>()
        {
            return Some(port);
        }
        // --port=3000
        if let Some(val) = part.strip_prefix("--port=")
            && let Ok(port) = val.parse::<u16>()
        {
            return Some(port);
        }
        // PORT=3000 in env prefix
        if let Some(val) = part.strip_prefix("PORT=")
            && let Ok(port) = val.parse::<u16>()
        {
            return Some(port);
        }
    }
    None
}

/// The `atlas.yaml` that `atlas init` writes for `root`, and what was detected there. The one place the file is
/// rendered: the CLI writes it, Atlas Desktop previews it before creating the project.
pub fn initial_config(root: &Path) -> (String, ProjectDetection) {
    let detection = detect_project(root);
    let name = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .file_name()
        .map_or_else(
            || "my-project".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );

    let mut yaml = format!("name: {name}\n\nservices:\n");
    if detection.services.is_empty() {
        yaml.push_str("  # app:\n");
        yaml.push_str("  #   command: npm run dev\n");
        yaml.push_str(&format!("  #   port: {PORT_BLOCK_START}\n"));
    } else {
        for (i, svc) in detection.services.iter().enumerate() {
            let port = svc.port.unwrap_or(PORT_BLOCK_START + i as u16);
            yaml.push_str(&format!("  {}:\n", svc.name));
            yaml.push_str(&format!("    command: {}\n", svc.command));
            yaml.push_str(&format!("    port: {port}\n"));
        }
    }
    (yaml, detection)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Incident (#65): the desktop could only add a folder that already had an atlas.yaml. Its "set up this folder"
    // flow previews exactly what `atlas init` will write, so both must come from this one function.
    #[test]
    fn initial_config_of_an_unknown_folder_is_named_after_the_folder_with_only_commented_hints() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("my-api");
        std::fs::create_dir_all(&dir).unwrap();
        let (yaml, d) = initial_config(&dir);
        assert_eq!(d.language, "unknown");
        assert!(yaml.starts_with("name: my-api\n\nservices:\n"), "{yaml}");
        // No service line is active: every line after `services:` is a comment.
        let active: Vec<&str> = yaml
            .lines()
            .skip(3)
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .collect();
        assert!(active.is_empty(), "{active:?}");
    }

    #[test]
    fn initial_config_lists_every_detected_service_with_a_command_and_a_port() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("package.json"),
            r#"{"name":"x","scripts":{"dev":"vite --port 5173"}}"#,
        )
        .unwrap();
        let (yaml, d) = initial_config(t.path());
        assert!(!d.services.is_empty(), "{d:?}");
        for svc in &d.services {
            assert!(yaml.contains(&format!("  {}:\n", svc.name)), "{yaml}");
            assert!(
                yaml.contains(&format!("    command: {}\n", svc.command)),
                "{yaml}"
            );
        }
        assert_eq!(
            yaml.matches("    port: ").count(),
            d.services.len(),
            "{yaml}"
        );
    }

    #[test]
    fn extract_port_reads_flags_env_prefix_and_node_listen_calls() {
        assert_eq!(extract_port("vite --port 5173"), Some(5173));
        assert_eq!(extract_port("next dev -p 3001"), Some(3001));
        assert_eq!(extract_port("serve --port=8080"), Some(8080));
        assert_eq!(extract_port("PORT=4000 node x.js"), Some(4000));
        assert_eq!(
            extract_port(r#"node -e "require('http').createServer().listen(4123)""#),
            Some(4123)
        );
        assert_eq!(extract_port("node -e \"x.listen(port)\""), None);
        assert_eq!(extract_port("bun dev"), None);
    }

    fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        for (name, body) in files {
            let p = t.path().join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        t
    }

    fn commands(d: &ProjectDetection) -> Vec<(&str, &str)> {
        d.services
            .iter()
            .map(|s| (s.name.as_str(), s.command.as_str()))
            .collect()
    }

    // Incident: a brand-new user opened a Rust/Go/Python folder and Atlas said "found nothing to run".
    #[test]
    fn a_cargo_package_with_one_bin_runs_with_cargo_run() {
        let t = fixture(&[
            (
                "Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
            ),
            ("src/main.rs", "fn main() {}\n"),
        ]);
        assert_eq!(commands(&detect_project(t.path())), [("app", "cargo run")]);
    }

    #[test]
    fn a_cargo_package_with_a_named_bin_runs_with_cargo_run() {
        let t = fixture(&[
            (
                "Cargo.toml",
                "[package]\nname = \"lib-name\"\n\n[[bin]]\nname = \"tool\"\npath = \"src/tool.rs\"\n",
            ),
            ("src/tool.rs", "fn main() {}\n"),
        ]);
        assert_eq!(commands(&detect_project(t.path())), [("tool", "cargo run")]);
    }

    #[test]
    fn cargo_folders_without_exactly_one_bin_get_no_service() {
        // Library only.
        let lib = fixture(&[
            ("Cargo.toml", "[package]\nname = \"l\"\n"),
            ("src/lib.rs", ""),
        ]);
        assert!(detect_project(lib.path()).services.is_empty());
        // Virtual workspace: `cargo run` would fail with no package.
        let ws = fixture(&[("Cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n")]);
        assert!(detect_project(ws.path()).services.is_empty());
        // Two binaries: `cargo run` is ambiguous.
        let two = fixture(&[
            ("Cargo.toml", "[package]\nname = \"t\"\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("src/bin/other.rs", "fn main() {}\n"),
        ]);
        assert!(detect_project(two.path()).services.is_empty());
        let two_decl = fixture(&[(
            "Cargo.toml",
            "[package]\nname = \"t\"\n[[bin]]\nname = \"a\"\npath = \"a.rs\"\n[[bin]]\nname = \"b\"\npath = \"b.rs\"\n",
        )]);
        assert!(detect_project(two_decl.path()).services.is_empty());
    }

    #[test]
    fn a_go_module_with_a_main_package_at_the_root_runs_with_go_run() {
        let t = fixture(&[
            ("go.mod", "module example.com/app\n\ngo 1.22\n"),
            ("main.go", "package main\n\nfunc main() {}\n"),
        ]);
        assert_eq!(commands(&detect_project(t.path())), [("app", "go run .")]);
    }

    #[test]
    fn go_modules_without_a_root_main_package_get_no_service() {
        // main lives under cmd/: which one to run is the user's call.
        let cmd = fixture(&[
            ("go.mod", "module example.com/app\n"),
            ("cmd/app/main.go", "package main\n"),
        ]);
        assert!(detect_project(cmd.path()).services.is_empty());
        // A library package at the root.
        let lib = fixture(&[
            ("go.mod", "module example.com/lib\n"),
            ("lib.go", "package lib\n"),
        ]);
        assert!(detect_project(lib.path()).services.is_empty());
        // A test file alone is not a program.
        let test_only = fixture(&[
            ("go.mod", "module example.com/lib\n"),
            ("x_test.go", "package main\n"),
        ]);
        assert!(detect_project(test_only.path()).services.is_empty());
    }

    #[test]
    fn a_django_manage_py_runs_the_dev_server() {
        let t = fixture(&[
            ("manage.py", "import django\n"),
            ("requirements.txt", "django\n"),
        ]);
        assert_eq!(
            commands(&detect_project(t.path())),
            [("web", "python manage.py runserver")]
        );
    }

    #[test]
    fn fastapi_with_one_obvious_app_module_runs_uvicorn() {
        let t = fixture(&[
            (
                "pyproject.toml",
                "[project]\nname = \"api\"\ndependencies = [\"fastapi\", \"uvicorn\"]\n",
            ),
            ("main.py", "from fastapi import FastAPI\napp = FastAPI()\n"),
        ]);
        assert_eq!(
            commands(&detect_project(t.path())),
            [("api", "uvicorn main:app --reload")]
        );
    }

    #[test]
    fn fastapi_without_one_unambiguous_app_gets_no_service() {
        let deps = "[project]\nname = \"api\"\ndependencies = [\"fastapi\", \"uvicorn\"]\n";
        // No module with an `app` object.
        let none = fixture(&[("pyproject.toml", deps), ("main.py", "print('hi')\n")]);
        assert!(detect_project(none.path()).services.is_empty());
        // Two candidates.
        let two = fixture(&[
            ("pyproject.toml", deps),
            ("main.py", "app = FastAPI()\n"),
            ("app.py", "app = FastAPI()\n"),
        ]);
        assert!(detect_project(two.path()).services.is_empty());
        // fastapi without uvicorn: the server to run is not known.
        let no_server = fixture(&[
            (
                "pyproject.toml",
                "[project]\nname = \"api\"\ndependencies = [\"fastapi\"]\n",
            ),
            ("main.py", "app = FastAPI()\n"),
        ]);
        assert!(detect_project(no_server.path()).services.is_empty());
    }

    #[test]
    fn package_json_services_are_unchanged_next_to_a_cargo_manifest() {
        let t = fixture(&[
            (
                "package.json",
                r#"{"scripts":{"dev":"vite --port 5173","build":"vite build"}}"#,
            ),
            ("Cargo.toml", "[package]\nname = \"native\"\n"),
            ("src/main.rs", "fn main() {}\n"),
        ]);
        let d = detect_project(t.path());
        let names: Vec<&str> = d.services.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["dev", "native"], "{d:?}");
        assert_eq!(d.services[0].port, Some(5173));
    }

    #[test]
    fn detection_label_names_the_stack_and_its_manifest() {
        let rust = fixture(&[(
            "Cargo.toml",
            "[package]\nname = \"x\"\n[dependencies]\naxum = \"1\"\n",
        )]);
        assert_eq!(
            detection_label(rust.path(), &detect_project(rust.path())).as_deref(),
            Some("Rust project (Cargo.toml, axum)")
        );
        let node = fixture(&[("package.json", "{}")]);
        assert_eq!(
            detection_label(node.path(), &detect_project(node.path())).as_deref(),
            Some("Node project (package.json)")
        );
        let go = fixture(&[("go.mod", "module x\n")]);
        assert_eq!(
            detection_label(go.path(), &detect_project(go.path())).as_deref(),
            Some("Go project (go.mod)")
        );
        let py = fixture(&[("requirements.txt", "flask\n")]);
        assert_eq!(
            detection_label(py.path(), &detect_project(py.path())).as_deref(),
            Some("Python project (requirements.txt)")
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            detection_label(empty.path(), &detect_project(empty.path())),
            None
        );
    }
}
