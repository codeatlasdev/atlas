//! Hand things to macOS `open`: the editor, the browser, Finder. Windows: VS Code's `code`, the
//! default browser and File Explorer.

use super::root_of;

#[cfg(windows)]
fn spawn(program: &std::path::Path, args: &[&str]) -> Result<(), String> {
    use atlas_core::sys::CommandExt as _;
    std::process::Command::new(program)
        .args(args)
        .no_window()
        .stdin(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", program.display()))
}

#[cfg(windows)]
fn explorer(args: &[&str]) -> Result<(), String> {
    // Explorer's exit code is 1 even on success: only a failed start is an error.
    spawn(std::path::Path::new("explorer.exe"), args)
}

#[cfg(windows)]
#[tauri::command]
pub fn open_in_editor(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    let r = root.to_string_lossy().replace('/', "\\");
    match atlas_core::paths::which("code") {
        Some(code) => spawn(&code, &[&r]),
        None => explorer(&[&r]),
    }
}

#[cfg(windows)]
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("refusing to open {url}"));
    }
    explorer(&[&url])
}

#[cfg(windows)]
#[tauri::command]
pub fn reveal(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    let file = root.join("atlas.yaml").to_string_lossy().replace('/', "\\");
    explorer(&[&format!("/select,{file}")])
}

#[cfg(unix)]
fn open(args: &[&str]) -> Result<(), String> {
    let st = std::process::Command::new("open")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("open {} failed", args.join(" ")))
    }
}

#[cfg(unix)]
#[tauri::command]
pub fn open_in_editor(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    let r = root.to_string_lossy();
    open(&["-a", "Visual Studio Code", &r]).or_else(|_| open(&[&r]))
}

#[cfg(unix)]
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("refusing to open {url}"));
    }
    open(&[&url])
}

#[cfg(unix)]
#[tauri::command]
pub fn reveal(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    open(&["-R", &root.join("atlas.yaml").to_string_lossy()])
}
