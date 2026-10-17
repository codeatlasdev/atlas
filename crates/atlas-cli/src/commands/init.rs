use std::path::Path;

use anyhow::Result;
use atlas_core::domain::project::initial_config;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(short, long, default_value = ".")]
    pub dir: String,
}

impl AtlasCommand for InitArgs {
    async fn run(self) -> Result<()> {
        handle(&self.dir).await
    }
}

pub async fn handle(dir: &str) -> Result<()> {
    let root = Path::new(dir);

    let config_path = root.join("atlas.yaml");
    if config_path.exists() {
        anyhow::bail!(
            "atlas.yaml already exists in {}",
            root.canonicalize().unwrap_or(root.to_path_buf()).display()
        );
    }

    let (yaml, detection) = initial_config(root);

    std::fs::write(&config_path, &yaml)?;

    println!("\x1b[1;32m\u{2713}\x1b[0m Created atlas.yaml");

    if detection.language != "unknown" {
        let detail = match detection.framework.as_deref() {
            Some(fw) => format!("{} ({})", detection.language, fw),
            None => detection.language.clone(),
        };
        println!("  Detected: {detail}");
    }

    if detection.services.is_empty() {
        println!("  No services auto-detected — edit atlas.yaml to add your services.");
    } else {
        let names: Vec<&str> = detection.services.iter().map(|s| s.name.as_str()).collect();
        println!("  Services: {}", names.join(", "));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        handle(dir.path().to_str().unwrap()).await.unwrap();
        assert!(dir.path().join("atlas.yaml").exists());
    }

    #[tokio::test]
    async fn test_init_fails_if_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("atlas.yaml"), "name: x\n").unwrap();
        assert!(handle(dir.path().to_str().unwrap()).await.is_err());
    }
}
