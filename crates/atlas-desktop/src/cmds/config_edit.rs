//! Reading, validating and saving a project's `atlas.yaml` from the editor.

use std::path::Path;

use atlas_tui::config::{self, ConfigError};
use serde::Serialize;

use super::root_of;
use crate::registry;

#[derive(Serialize)]
pub struct Problem {
    path: String,
    message: String,
}

#[derive(Serialize)]
pub struct ConfigText {
    text: String,
    path: String,
}

/// The text of a project's `atlas.yaml`, or, with no file, what Atlas detected (what "Save as atlas.yaml" writes).
pub(super) fn read_config_text(root: &Path) -> Result<String, String> {
    let path = root.join("atlas.yaml");
    if path.is_file() {
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    } else {
        Ok(atlas_core::domain::project::initial_config(root).0)
    }
}

/// The project's `atlas.yaml` (or the detected text, at the path it would be saved to, so the Config tab can edit
/// and save a first version).
#[tauri::command]
pub fn config_read(id: String) -> Result<ConfigText, String> {
    let root = root_of(&id)?;
    Ok(ConfigText {
        text: read_config_text(&root)?,
        path: root.join("atlas.yaml").to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn config_schema() -> String {
    config::json_schema()
}

fn validate(root: &Path, text: &str) -> Result<Vec<Problem>, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("atlas.yaml"), text).map_err(|e| e.to_string())?;
    let local = root.join("atlas.local.yaml");
    if local.is_file() {
        std::fs::copy(&local, dir.path().join("atlas.local.yaml")).map_err(|e| e.to_string())?;
    }
    let res = config::load_checked(dir.path());
    Ok(match res {
        Ok((_, errs)) => errs.iter().map(problem).collect(),
        Err(e) => vec![Problem {
            path: String::new(),
            message: format!("{e:#}"),
        }],
    })
}

fn problem(e: &ConfigError) -> Problem {
    Problem {
        path: e.path.clone(),
        message: e.message.clone(),
    }
}

#[tauri::command]
pub fn config_validate(id: String, text: String) -> Result<Vec<Problem>, String> {
    validate(&root_of(&id)?, &text)
}

#[tauri::command]
pub fn config_write(id: String, text: String) -> Result<(), String> {
    write_checked(&root_of(&id)?, &text)
}

/// Validate, then tmp+rename; an invalid text never reaches disk.
fn write_checked(root: &Path, text: &str) -> Result<(), String> {
    let problems = validate(root, text)?;
    if let Some(p) = problems.first() {
        return Err(format!(
            "not saved: {} problem(s), first: {} {}",
            problems.len(),
            p.path,
            p.message
        ));
    }
    registry::write_atomic(&root.join("atlas.yaml"), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "name: demo\nservices:\n  web: { command: \"true\", port: 4711 }\n";

    #[test]
    fn validate_reports_semantic_errors_with_paths() {
        let root = tempfile::tempdir().unwrap();
        let bad = format!("{OK}  api: {{ command: \"true\", depends_on: [nope] }}\n");
        let probs = validate(root.path(), &bad).unwrap();
        assert_eq!(probs.len(), 1);
        assert_eq!(probs[0].path, "services.api.depends_on.nope");
        assert!(probs[0].message.contains("unknown service 'nope'"));
        assert!(validate(root.path(), OK).unwrap().is_empty());
    }

    #[test]
    fn validate_merges_the_projects_local_override() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("atlas.local.yaml"),
            "services:\n  web: { helth: x }\n",
        )
        .unwrap();
        let probs = validate(root.path(), OK).unwrap();
        assert!(
            probs.iter().any(|p| p.path.contains("helth")),
            "{:?}",
            probs.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn validate_turns_parse_failures_into_one_problem() {
        let root = tempfile::tempdir().unwrap();
        let probs = validate(root.path(), "services: [").unwrap();
        assert_eq!(probs.len(), 1);
        assert!(probs[0].path.is_empty());
    }

    #[test]
    fn write_refuses_invalid_and_saves_valid() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("atlas.yaml");
        std::fs::write(&file, OK).unwrap();
        let bad = format!("{OK}  api: {{ command: \"true\", depends_on: [nope] }}\n");
        let err = write_checked(root.path(), &bad).unwrap_err();
        assert!(err.contains("unknown service 'nope'"), "{err}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), OK);
        let fixed = format!("{OK}  api: {{ command: \"true\", depends_on: [web] }}\n");
        write_checked(root.path(), &fixed).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), fixed);
    }

    // Zero-config: the Config tab of a folder without atlas.yaml shows the detected text (never an error), and once a
    // file exists the file wins over detection.
    #[test]
    fn read_config_text_is_the_detected_config_until_a_file_exists() {
        let root = tempfile::tempdir().unwrap();
        let folder = root
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let detected = read_config_text(root.path()).unwrap();
        assert!(
            detected.starts_with(&format!("name: {folder}\n")),
            "{detected}"
        );
        assert!(
            !root.path().join("atlas.yaml").exists(),
            "reading must not write"
        );
        std::fs::write(root.path().join("atlas.yaml"), OK).unwrap();
        assert_eq!(read_config_text(root.path()).unwrap(), OK);
    }
}
