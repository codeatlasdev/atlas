//! Ad-hoc services: `atlas -- <cmd>` runs a command as a supervised service of the current
//! stack without touching `atlas.yaml`.

use std::collections::HashSet;

use super::project::{ProjectConfig, RestartPolicy, ServiceDef};

/// Longest generated service name; a pasted one-liner must not become a 200-char tab.
const MAX_NAME: usize = 32;

/// A service name from a command line: its leading plain words (at most two), lowercased and
/// joined with `-`: `bun dev` → `bun-dev`, `node -e "…"` → `node`, `npm run dev` → `npm-run`.
/// The words stop at the first flag or quoted word, so code and option values never enter the
/// name. A program path loses its directory and `.exe`; characters outside `[a-z0-9._-]` drop.
pub fn name_from_command(command: &str) -> String {
    let parts: Vec<String> = command
        .split_whitespace()
        .take_while(|w| !w.starts_with(['-', '"', '\'']))
        .filter_map(|w| {
            let w = w.rsplit(['/', '\\']).next().unwrap_or(w);
            let w = w.strip_suffix(".exe").unwrap_or(w);
            let clean: String = w
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                .collect::<String>()
                .to_ascii_lowercase();
            (!clean.is_empty()).then_some(clean)
        })
        .take(2)
        .collect();
    let mut name = parts.join("-");
    name.truncate(MAX_NAME);
    if name.is_empty() {
        "adhoc".into()
    } else {
        name
    }
}

/// `base`, or `base-2`, `base-3`… the first not in `taken`.
pub fn unique_name(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !taken.contains(c))
        .expect("unbounded range")
}

/// The service for `command`: restart `no` (a crashed ad-hoc command stays visible, never loops),
/// port taken from the command (`--port N`, `PORT=N`, `.listen(N)`) when it says one.
pub fn service_def(command: &str) -> ServiceDef {
    ServiceDef {
        command: Some(command.to_string()),
        port: atlas_core::domain::project::extract_port(command),
        restart: RestartPolicy::No,
        ..ServiceDef::default()
    }
}

/// Add `command` to `config` under a name not yet used; returns the name.
pub fn add_one(config: &mut ProjectConfig, command: &str) -> String {
    let taken: HashSet<String> = config.services.keys().cloned().collect();
    let name = unique_name(&name_from_command(command), &taken);
    config.services.insert(name.clone(), service_def(command));
    name
}

/// [`add_one`] for each of `commands`; the names in order.
pub fn add_all(config: &mut ProjectConfig, commands: &[String]) -> Vec<String> {
    commands.iter().map(|c| add_one(config, c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_the_leading_plain_words() {
        assert_eq!(name_from_command("bun dev"), "bun-dev");
        assert_eq!(name_from_command("npm run dev -- --port 3000"), "npm-run");
        assert_eq!(name_from_command("python -m http.server 8000"), "python");
        assert_eq!(
            name_from_command("C:\\tools\\node.exe server.js"),
            "node-server.js"
        );
        assert_eq!(name_from_command("./run.sh"), "run.sh");
    }

    #[test]
    fn quoted_code_never_leaks_into_the_name() {
        assert_eq!(
            name_from_command(
                r#"node -e "require('http').createServer((q,s)=>s.end('ok')).listen(4123)""#
            ),
            "node"
        );
        assert_eq!(name_from_command("   "), "adhoc");
        assert_eq!(name_from_command("&&"), "adhoc");
    }

    #[test]
    fn a_taken_name_gets_a_numeric_suffix() {
        let taken: HashSet<String> = ["bun-dev", "bun-dev-2"].map(String::from).into();
        assert_eq!(unique_name("bun-dev", &taken), "bun-dev-3");
        assert_eq!(unique_name("other", &taken), "other");
    }

    #[test]
    fn add_all_dedupes_against_the_config_and_among_themselves() {
        let mut cfg = ProjectConfig::default();
        cfg.services.insert("bun-dev".into(), ServiceDef::default());
        let names = add_all(&mut cfg, &["bun dev".into(), "bun dev".into()]);
        assert_eq!(names, ["bun-dev-2", "bun-dev-3"]);
        assert_eq!(
            cfg.services["bun-dev-2"].command.as_deref(),
            Some("bun dev")
        );
        assert_eq!(cfg.services["bun-dev-2"].restart, RestartPolicy::No);
    }

    #[test]
    fn the_port_is_read_from_the_command() {
        assert_eq!(service_def("vite --port 5173").port, Some(5173));
        assert_eq!(service_def("echo hi").port, None);
    }
}
