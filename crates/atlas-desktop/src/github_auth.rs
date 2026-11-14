//! Git metadata helpers for crewmate project tracking.
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct GitInfo {
    pub url: Option<String>,
    pub branch: Option<String>,
}

/// Git remote URL and current branch for a local project path.
#[tauri::command]
pub fn project_git_info(path: String) -> GitInfo {
    let p = std::path::Path::new(&path);
    GitInfo {
        url: crate::cmds::discover::git_remote(p),
        branch: crate::cmds::discover::git_branch(p),
    }
}
