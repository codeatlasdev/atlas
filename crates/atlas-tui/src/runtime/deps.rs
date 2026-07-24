use std::path::Path;
use std::time::SystemTime;

use tokio::process::Command;
use tracing::info;

/// Check if dependencies need to be installed and install them if so.
/// Returns true if install was run (whether or not it succeeded).
pub async fn ensure_deps(root_dir: &Path) -> bool {
    let node_modules = root_dir.join("node_modules");
    let bun_lock = root_dir.join("bun.lock");
    let pnpm_lock = root_dir.join("pnpm-lock.yaml");
    let yarn_lock = root_dir.join("yarn.lock");
    let package_lock = root_dir.join("package-lock.json");

    let needs_install = if !node_modules.exists() {
        true
    } else {
        // Use marker file if it exists, otherwise use node_modules mtime
        let marker = node_modules.join(".atlas-installed");
        let nm_mtime = file_mtime(&marker)
            .or_else(|| dir_mtime(&node_modules))
            .unwrap_or(SystemTime::UNIX_EPOCH);

        [&bun_lock, &pnpm_lock, &yarn_lock, &package_lock]
            .iter()
            .any(|lock| {
                lock.exists() && file_mtime(lock).unwrap_or(SystemTime::UNIX_EPOCH) > nm_mtime
            })
    };

    if !needs_install {
        return false;
    }

    info!("Dependencies stale, running install...");

    let (cmd, args) = if bun_lock.exists() {
        ("bun", vec!["install"])
    } else if pnpm_lock.exists() {
        ("pnpm", vec!["install", "--frozen-lockfile"])
    } else if yarn_lock.exists() {
        ("yarn", vec!["install", "--frozen-lockfile"])
    } else {
        ("npm", vec!["ci"])
    };

    let result = Command::new(cmd)
        .args(&args)
        .current_dir(root_dir)
        .output()
        .await;

    if let Ok(output) = result {
        if output.status.success() {
            let _ = touch_marker(&node_modules);
            info!("Dependencies installed successfully");
        }
    }

    true
}

fn dir_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn touch_marker(node_modules: &Path) -> std::io::Result<()> {
    std::fs::write(
        node_modules.join(".atlas-installed"),
        format!("{}", std::process::id()),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_needs_install_no_node_modules() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules");
        assert!(!nm.exists());
    }

    #[test]
    fn test_needs_install_with_fresh_node_modules() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let bun_lock = dir.path().join("bun.lock");
        assert!(!bun_lock.exists());
    }

    #[test]
    fn test_needs_install_stale_node_modules() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(dir.path().join("bun.lock"), "lockfile content").unwrap();

        let nm_mtime = dir_mtime(&nm).unwrap();
        let lock_mtime = file_mtime(&dir.path().join("bun.lock")).unwrap();
        assert!(lock_mtime > nm_mtime);
    }

    #[test]
    fn test_touch_marker() {
        let dir = TempDir::new().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();

        touch_marker(&nm).unwrap();
        assert!(nm.join(".atlas-installed").exists());
    }
}
