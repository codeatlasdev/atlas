use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{AtlasError, Result};

// MARK: - Project Config (atlas.yaml)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub org: Option<String>,
    #[serde(default)]
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub user: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub command: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub env_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub strategy: String,
    #[serde(default)]
    pub domain: Option<String>,
}

pub fn load_project(path: &Path) -> Result<ProjectConfig> {
    let yaml_path = path.join("atlas.yaml");
    let content = std::fs::read_to_string(&yaml_path).map_err(|e| {
        AtlasError::InvalidInput(format!(
            "failed to read atlas.yaml at {}: {e}",
            yaml_path.display()
        ))
    })?;

    serde_yaml::from_str(&content).map_err(|e| {
        AtlasError::InvalidInput(format!("failed to parse atlas.yaml: {e}"))
    })
}

// MARK: - Project Detection

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
    pub dev_command: Option<String>,
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

        // Detect package manager
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
                // Detect framework from dependencies
                let deps = pkg.get("dependencies").and_then(|d| d.as_object());
                let dev_deps = pkg.get("devDependencies").and_then(|d| d.as_object());

                let has_dep = |name: &str| -> bool {
                    deps.is_some_and(|d| d.contains_key(name))
                        || dev_deps.is_some_and(|d| d.contains_key(name))
                };

                if has_dep("next") {
                    detection.framework = Some("nextjs".into());
                } else if has_dep("astro") {
                    detection.framework = Some("astro".into());
                } else if has_dep("elysia") {
                    detection.framework = Some("elysia".into());
                } else if has_dep("@tanstack/start") {
                    detection.framework = Some("tanstack-start".into());
                } else if has_dep("nuxt") {
                    detection.framework = Some("nuxt".into());
                } else if has_dep("svelte") || has_dep("@sveltejs/kit") {
                    detection.framework = Some("sveltekit".into());
                } else if has_dep("vue") {
                    detection.framework = Some("vue".into());
                } else if has_dep("react") {
                    detection.framework = Some("react".into());
                } else if has_dep("hono") {
                    detection.framework = Some("hono".into());
                } else if has_dep("express") {
                    detection.framework = Some("express".into());
                }

                // Extract scripts
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
        let scripts_clone: Vec<(String, String)> =
            detection.scripts.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (name, cmd) in &scripts_clone {
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
                    dev_command: Some(cmd.clone()),
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
        // Detect framework from pyproject.toml
        if let Ok(content) = std::fs::read_to_string(path.join("pyproject.toml")) {
            if content.contains("fastapi") {
                detection.framework = Some("fastapi".into());
            } else if content.contains("django") {
                detection.framework = Some("django".into());
            } else if content.contains("flask") {
                detection.framework = Some("flask".into());
            }
        }
    }

    // Go project
    if path.join("go.mod").exists() {
        if detection.language == "unknown" {
            detection.language = "go".into();
        }
        detection.package_manager = Some("go".into());
        if let Ok(content) = std::fs::read_to_string(path.join("go.mod")) {
            if content.contains("github.com/gin-gonic/gin") {
                detection.framework = Some("gin".into());
            } else if content.contains("github.com/gofiber/fiber") {
                detection.framework = Some("fiber".into());
            } else if content.contains("github.com/labstack/echo") {
                detection.framework = Some("echo".into());
            }
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
    if path.join("Dockerfile").exists() || path.join("docker-compose.yml").exists() {
        detection.deploy_strategy = Some("docker".into());
    } else if path.join("fly.toml").exists() {
        detection.deploy_strategy = Some("fly".into());
    } else if path.join("vercel.json").exists() {
        detection.deploy_strategy = Some("vercel".into());
    } else if path.join("netlify.toml").exists() {
        detection.deploy_strategy = Some("netlify".into());
    } else if path.join("render.yaml").exists() {
        detection.deploy_strategy = Some("render".into());
    } else {
        detection.deploy_strategy = Some("systemd".into());
    }

    detection
}

fn extract_port(cmd: &str) -> Option<u16> {
    // --port NNNN
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "--port" || *part == "-p") && i + 1 < parts.len() {
            if let Ok(port) = parts[i + 1].parse::<u16>() {
                return Some(port);
            }
        }
        // --port=NNNN
        if let Some(val) = part.strip_prefix("--port=") {
            if let Ok(port) = val.parse::<u16>() {
                return Some(port);
            }
        }
    }
    // PORT=NNNN
    for part in &parts {
        if let Some(val) = part.strip_prefix("PORT=") {
            if let Ok(port) = val.parse::<u16>() {
                return Some(port);
            }
        }
    }
    // Common framework defaults
    if cmd.contains("next") {
        return Some(3000);
    }
    if cmd.contains("astro") {
        return Some(4321);
    }
    if cmd.contains("vite") {
        return Some(5173);
    }
    None
}
