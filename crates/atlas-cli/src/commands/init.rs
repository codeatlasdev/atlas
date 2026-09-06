use std::path::Path;

use anyhow::Result;
use atlas_core::domain::project::{PORT_BLOCK_START, detect_project};

pub async fn handle(dir: &str) -> Result<()> {
    let root = Path::new(dir);

    let config_path = root.join("atlas.yaml");
    if config_path.exists() {
        anyhow::bail!(
            "atlas.yaml already exists in {}",
            root.canonicalize().unwrap_or(root.to_path_buf()).display()
        );
    }

    let detection = detect_project(root);
    let project_name = root
        .canonicalize()
        .unwrap_or(root.to_path_buf())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "my-project".to_string());

    let mut yaml = format!("name: {project_name}\n");

    yaml.push_str("\nservices:\n");
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
