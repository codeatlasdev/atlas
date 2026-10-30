//! `atlas <path>` (open a folder or file in Atlas Desktop) and `atlas -- <cmd…>` (run a command
//! as an ad-hoc service of the current folder's stack). Neither needs an `atlas.yaml`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::json;

/// How long the desktop gets to answer `path_open` (it may have to register the project).
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

/// True when a word that is not a subcommand is meant as a path: `.`, `..`, anything with a
/// path separator, or a name `exists` finds on disk. Anything else is a mistyped subcommand.
pub fn is_path_word(word: &OsString, exists: impl Fn(&OsString) -> bool) -> bool {
    let w = word.to_string_lossy();
    w == "." || w == ".." || w.contains(['/', '\\']) || exists(word)
}

/// `atlas <path>`: open it in Atlas Desktop (a file opens its project, then the file).
pub fn open_path(word: &OsString, line: Option<u32>) -> Result<()> {
    let path = absolute(Path::new(word))?;
    if !path.exists() {
        bail!("{} does not exist", path.display());
    }
    if atlas_core::desktop_ctl::running() {
        let mut args = json!({ "path": path.to_string_lossy() });
        if let Some(l) = line {
            args["line"] = l.into();
        }
        let r = atlas_core::desktop_ctl::call("path_open", &args, OPEN_TIMEOUT)?;
        println!("opened {} in Atlas Desktop", describe(&path, &r));
        return Ok(());
    }
    // Not running: launch the app with the path as argv; it opens it on start.
    let app = super::desktop::installed_app().with_context(|| {
        "Atlas Desktop is not installed; run `atlas desktop` to install it".to_string()
    })?;
    super::desktop::open(&app, &[path.clone().into_os_string()])?;
    println!("launching Atlas Desktop with {}", path.display());
    Ok(())
}

fn describe(path: &Path, reply: &serde_json::Value) -> String {
    reply["root"]
        .as_str()
        .map_or_else(|| path.display().to_string(), str::to_string)
}

/// The path as an absolute, `\\?\`-free, forward-slash-on-Windows path (what the desktop stores).
fn absolute(p: &Path) -> Result<PathBuf> {
    let full = std::path::absolute(p).with_context(|| format!("cannot resolve {}", p.display()))?;
    let canon = dunce_canonical(&full);
    Ok(canon)
}

/// `canonicalize` without the Windows `\\?\` verbatim prefix; the plain path on any failure.
fn dunce_canonical(p: &Path) -> PathBuf {
    let c = atlas_core::paths::canonical(p).unwrap_or_else(|_| p.to_path_buf());
    let s = c.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => c,
    }
}

/// The shell command line for `atlas -- <argv…>`. One argument is a ready command line
/// (`atlas -- "bun dev"`); several are quoted for the platform shell so `node -e "code"` keeps
/// its argument boundaries.
pub fn command_line(argv: &[String]) -> String {
    match argv {
        [one] => one.clone(),
        many => many
            .iter()
            .map(|a| quote(a, cfg!(windows)))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn quote(arg: &str, windows: bool) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c));
    if plain {
        return arg.to_string();
    }
    if windows {
        // cmd.exe /s /c: double quotes group; embedded quotes are backslash-escaped for the
        // program's own argv parsing.
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// `atlas -- <cmd…>`: add the command as a service of `resolve(cwd)`'s stack.
pub async fn run_adhoc(argv: &[String]) -> Result<()> {
    if argv.is_empty() {
        bail!("usage: atlas -- <command> [args…]");
    }
    let cwd = std::env::current_dir()?;
    let root = absolute(&atlas_core::project_root::resolve(&cwd))?;
    let command = command_line(argv);

    match atlas_tui::control::info(&root).await {
        Some(info) if info.mode == "runner" => {
            let resp =
                atlas_tui::control::call(&root, "add_service", json!({ "command": command }))
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            if !resp.ok {
                bail!(resp.error.unwrap_or_else(|| "add_service refused".into()));
            }
            let data = resp.data.unwrap_or_default();
            report(
                &root,
                data["service"].as_str().unwrap_or("?"),
                data["port"].as_u64(),
            );
            Ok(())
        }
        Some(info) => bail!(
            "atlas dev is running in a terminal for {} (pid {}); quit it, or stop it with atlas down, then run this again",
            root.display(),
            info.pid
        ),
        None => {
            let select = atlas_tui::SelectionRequest::default();
            let adhoc = [command.clone()];
            match atlas_tui::detach(&root, &select, &adhoc).await? {
                atlas_tui::runner::Detached::Started { pid } => {
                    println!(
                        "atlas runner started (pid {pid}) · attach: atlas dev · stop: atlas down"
                    );
                }
                atlas_tui::runner::Detached::AlreadyRunning { .. } => {
                    // Lost a race with another starter: it is a runner now, add live.
                    let resp = atlas_tui::control::call(
                        &root,
                        "add_service",
                        json!({ "command": command }),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                    if !resp.ok {
                        bail!(resp.error.unwrap_or_else(|| "add_service refused".into()));
                    }
                }
            }
            // The runner names it with the same function against the same config.
            let (mut cfg, _) = atlas_tui::config::load_or_detect(&root)?;
            let name = atlas_tui::config::adhoc::add_one(&mut cfg, &command);
            let port = cfg.services[&name].port.map(u64::from);
            report(&root, &name, port);
            Ok(())
        }
    }
}

fn report(root: &Path, service: &str, port: Option<u64>) {
    let log = root.join(".logs").join(format!("{service}.log"));
    println!("service {service} started (not saved to atlas.yaml)");
    println!("  logs: {}", log.display().to_string().replace('\\', "/"));
    if let Some(p) = port {
        println!("  port: {p} (http://localhost:{p})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn dot_dotdot_and_separators_are_paths_even_when_nothing_exists() {
        let nothing = |_: &OsString| false;
        for word in [".", "..", "./dev", "no/such/dir", r"C:\no\such"] {
            assert!(is_path_word(&w(word), nothing), "{word}");
        }
        assert!(!is_path_word(&w("devv"), nothing));
    }

    #[test]
    fn a_bare_word_is_a_path_when_the_file_system_has_it() {
        let t = tempfile::tempdir().unwrap();
        let there = t.path().join("my-folder");
        std::fs::create_dir(&there).unwrap();
        let on_disk = |p: &OsString| Path::new(p).exists();
        assert!(is_path_word(&there.as_os_str().to_owned(), on_disk));
        assert!(!is_path_word(&w("definitely-not-a-thing-devv"), on_disk));
    }

    #[test]
    fn one_argument_is_a_ready_command_line_and_several_are_quoted() {
        assert_eq!(command_line(&["bun dev".into()]), "bun dev");
        assert_eq!(
            command_line(&["node".into(), "server.js".into(), "--port=3000".into()]),
            "node server.js --port=3000"
        );
    }

    #[test]
    fn unix_quoting_keeps_argument_boundaries_and_single_quotes() {
        assert_eq!(quote("a b", false), "'a b'");
        assert_eq!(quote("it's", false), r"'it'\''s'");
        assert_eq!(quote("", false), "''");
        assert_eq!(quote("x.listen(4123)", false), "'x.listen(4123)'");
    }

    #[test]
    fn windows_quoting_uses_double_quotes_and_escapes_inner_ones() {
        assert_eq!(quote("a b", true), "\"a b\"");
        assert_eq!(quote(r#"say "hi""#, true), r#""say \"hi\"""#);
        assert_eq!(quote("plain-1.2", true), "plain-1.2");
    }
}
