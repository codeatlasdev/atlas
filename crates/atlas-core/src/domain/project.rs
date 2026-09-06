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

        if let Ok(content) = std::fs::read_to_string(path.join("package.json")) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
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

fn extract_port(cmd: &str) -> Option<u16> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "--port" || *part == "-p") && i + 1 < parts.len() {
            if let Ok(port) = parts[i + 1].parse::<u16>() {
                return Some(port);
            }
        }
        // --port=3000
        if let Some(val) = part.strip_prefix("--port=") {
            if let Ok(port) = val.parse::<u16>() {
                return Some(port);
            }
        }
        // PORT=3000 in env prefix
        if let Some(val) = part.strip_prefix("PORT=") {
            if let Ok(port) = val.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}
