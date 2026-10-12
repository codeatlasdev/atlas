use tokio::process::Command;

use super::ServiceManager;
use super::types;

impl ServiceManager {
    /// Patch gen/apple/project.yml to include bun and cargo in the build phase PATH.
    /// Xcode runs build phases with a minimal PATH that doesn't include ~/.bun/bin
    /// or ~/.cargo/bin. This runs automatically before any iOS session spawn and
    /// re-applies the patch if tauri ios init overwrote project.yml.
    pub(super) async fn patch_ios_project_yml_path(&self) {
        // Look for project.yml in the standard Tauri gen location
        let candidates = [
            self.root_dir.join("src-tauri/gen/apple/project.yml"),
            self.root_dir.join("app/src-tauri/gen/apple/project.yml"),
        ];

        for project_yml in &candidates {
            if !tokio::fs::try_exists(project_yml).await.unwrap_or(false) {
                continue;
            }

            let content = match tokio::fs::read_to_string(project_yml).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Already patched — skip
            if content.contains(".bun/bin") || content.contains(".cargo/bin") {
                return;
            }

            // Patch the preBuildScripts line: prepend PATH export before `bun tauri ios xcode-script`
            let patched = content.replace(
                "script: bun tauri ios xcode-script",
                "script: \"export PATH=\\\"/opt/homebrew/bin:$HOME/.bun/bin:$HOME/.cargo/bin:$PATH\\\"; bun tauri ios xcode-script",
            ).replace(
                // Also handle the quoted form that xcodegen may produce
                "\"script\": \"bun tauri ios xcode-script",
                "\"script\": \"export PATH=\\\"/opt/homebrew/bin:$HOME/.bun/bin:$HOME/.cargo/bin:$PATH\\\"; bun tauri ios xcode-script",
            );

            if patched != content {
                let _ = tokio::fs::write(project_yml, &patched).await;
                // Regenerate .xcodeproj from the patched project.yml
                let _ = Command::new("xcodegen")
                    .arg("generate")
                    .arg("--spec")
                    .arg(project_yml)
                    .current_dir(project_yml.parent().unwrap_or(&self.root_dir))
                    .output()
                    .await;
            }
            return;
        }
    }
}

/// Parse a single log line from a mobile session and return the new build phase
/// if the line signals a phase transition. Returns None for ordinary log lines.
pub(super) fn detect_mobile_phase(
    line: &str,
    platform: &crate::config::SessionPlatform,
) -> Option<types::MobileBuildPhase> {
    use crate::config::SessionPlatform;
    use types::MobileBuildPhase::*;

    // Shared: cargo output
    if line.contains("Compiling ") && line.starts_with("   Compiling") {
        return Some(CompilingRust);
    }
    if line.contains("Finished ") && line.contains("profile") {
        return Some(match platform {
            SessionPlatform::Android => GradleBuild,
            SessionPlatform::Ios => XcodeBuild,
        });
    }
    // Shared: error → Failed
    if line.contains("error[E") || line.contains("FAILURE:") || line.contains("** BUILD FAILED **")
    {
        return Some(Failed(line.trim().to_string()));
    }

    match platform {
        SessionPlatform::Android => {
            if line.contains("Running Gradle task")
                || line.contains("Executing tasks")
                || line.contains("> Task :")
            {
                return Some(GradleBuild);
            }
            if line.contains("BUILD SUCCESSFUL") {
                return Some(Installing);
            }
            if line.contains("adb install") || line.contains("Installing build/") {
                return Some(Installing);
            }
            if line.contains("Starting activity") || line.contains("Launching") {
                return Some(Launching);
            }
            // Logcat lines start with a date/pid pattern — signals app is running
            if line.len() > 10 && line.chars().nth(2) == Some('-') {
                return Some(Running);
            }
        }
        SessionPlatform::Ios => {
            if line.contains("PhaseScriptExecution")
                || line.contains("xcodebuild")
                || line.contains("CompileC ")
                || line.contains("Ld ")
            {
                return Some(XcodeBuild);
            }
            if line.contains("** BUILD SUCCEEDED **") {
                return Some(Installing);
            }
            if line.contains("Installing and launching") || line.contains("Install ") {
                return Some(Installing);
            }
            if line.contains("Launching") || line.contains("BundleID") {
                return Some(Launching);
            }
            // Post-launch: Tauri prints the WKWebView url line
            if line.contains("TAURI_DEV_HOST") || line.contains("webview") {
                return Some(Running);
            }
        }
    }

    None
}
