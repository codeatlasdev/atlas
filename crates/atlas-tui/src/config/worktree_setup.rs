//! `worktree.setup` with cached actions in it (#130): a `{{action: <key or label>}}` step is
//! replaced by that action's spawn command, so a build declared once as an action with a
//! `cache:` block runs through `atlas run --cache` during worktree setup. In a new worktree
//! with the same input bytes as one that already built, that step restores the outputs from the
//! task cache instead of running.
//!
//! [`ProjectConfig::setup_command`] is the single place that turns the configured text into the
//! shell line Atlas Desktop runs; [`unknown_refs`] feeds config validation, so a typo is a
//! `atlas config check` error and never a half-created worktree.

use super::project::{ActionConfig, ProjectConfig};

const OPEN: &str = "{{action:";
const CLOSE: &str = "}}";

/// One `{{action: …}}` mention: where it sits in the text and the reference inside.
struct Mention<'a> {
    start: usize,
    end: usize,
    reference: &'a str,
}

fn mentions(text: &str) -> Vec<Mention<'_>> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find(OPEN) {
        let start = at + i;
        let inner = start + OPEN.len();
        let Some(j) = text[inner..].find(CLOSE) else {
            break;
        };
        found.push(Mention {
            start,
            end: inner + j + CLOSE.len(),
            reference: text[inner..inner + j].trim(),
        });
        at = inner + j + CLOSE.len();
    }
    found
}

/// The action a reference names: its label (exact), or its one-character key.
fn resolve<'a>(actions: &'a [ActionConfig], reference: &str) -> Option<&'a ActionConfig> {
    actions.iter().find(|a| a.label == reference).or_else(|| {
        let mut chars = reference.chars();
        let (Some(k), None) = (chars.next(), chars.next()) else {
            return None;
        };
        actions.iter().find(|a| a.key == k)
    })
}

/// References in `text` that name no action, in order of appearance.
pub(crate) fn unknown_refs<'a>(text: &'a str, actions: &[ActionConfig]) -> Vec<&'a str> {
    mentions(text)
        .into_iter()
        .map(|m| m.reference)
        .filter(|r| resolve(actions, r).is_none())
        .collect()
}

impl ProjectConfig {
    /// `worktree.setup` as the shell line to run: every `{{action: X}}` replaced by action X's
    /// spawn command (through the task cache when it has a `cache:` block, behind the heavy queue
    /// when `heavy`). `None` without a `worktree.setup`; an unknown reference is an error naming it.
    pub fn setup_command(&self) -> Result<Option<String>, String> {
        let Some(text) = self.worktree.as_ref().and_then(|w| w.setup.as_deref()) else {
            return Ok(None);
        };
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for m in mentions(text) {
            let action = resolve(&self.actions, m.reference).ok_or_else(|| {
                format!(
                    "worktree.setup mentions action '{}', which atlas.yaml does not define",
                    m.reference
                )
            })?;
            out.push_str(&text[at..m.start]);
            out.push_str(&action.spawn_command());
            at = m.end;
        }
        out.push_str(&text[at..]);
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{load, load_checked};
    use tempfile::TempDir;

    fn config(extra_worktree: &str) -> (TempDir, ProjectConfig) {
        let dir = TempDir::new().unwrap();
        let yaml = format!(
            "name: t\nservices:\n  a:\n    command: x\nactions:\n  - key: b\n    label: Build web\n    command: bun run build\n    cache:\n      inputs: ['src/**', bun.lock]\n      outputs: [dist]\n  - key: l\n    label: Lint\n    command: bun run lint\nworktree:\n{extra_worktree}"
        );
        std::fs::write(dir.path().join("atlas.yaml"), yaml).unwrap();
        let cfg = load(dir.path()).unwrap();
        (dir, cfg)
    }

    // #130: a cached action named in setup runs through `atlas run --cache`, so a worktree with
    // the same input bytes restores its outputs instead of rebuilding.
    #[test]
    fn a_cached_action_in_setup_runs_through_the_task_cache() {
        let (_d, cfg) = config("  setup: 'bun install && {{action: Build web}} && echo ready'\n");
        let line = cfg.setup_command().unwrap().unwrap();
        assert!(line.starts_with("bun install && "), "{line}");
        assert!(line.contains(" run --cache --label 'Build web'"), "{line}");
        assert!(line.contains("--input 'src/**'"), "{line}");
        assert!(line.contains("--output dist"), "{line}");
        assert!(line.contains("-- 'bun run build'"), "{line}");
        assert!(line.ends_with(" && echo ready"), "{line}");
        assert!(!line.contains("{{"), "{line}");
    }

    // The action key works too, and an action without `cache:` is spliced as its plain command.
    #[test]
    fn an_action_can_be_named_by_key_and_an_uncached_one_is_its_plain_command() {
        let (_d, cfg) = config("  setup: '{{action: l}} && {{action:Lint}}'\n");
        assert_eq!(
            cfg.setup_command().unwrap().unwrap(),
            "bun run lint && bun run lint"
        );
    }

    // A setup without mentions is byte-for-byte what the user wrote; none at all is None.
    #[test]
    fn a_setup_without_mentions_is_unchanged_and_no_setup_is_none() {
        let (_d, cfg) = config("  setup: bun install && echo '{ not an action }'\n");
        assert_eq!(
            cfg.setup_command().unwrap().unwrap(),
            "bun install && echo '{ not an action }'"
        );
        let (_d, cfg) = config("  shared: [.env]\n");
        assert_eq!(cfg.setup_command().unwrap(), None);
    }

    // An unterminated mention is plain text (the shell will report it), never a panic.
    #[test]
    fn an_unterminated_mention_is_left_as_text() {
        let (_d, cfg) = config("  setup: 'echo {{action: Build web'\n");
        assert_eq!(
            cfg.setup_command().unwrap().unwrap(),
            "echo {{action: Build web"
        );
    }

    // A typo is caught when the config is checked, with the path, not when a worktree is half made.
    #[test]
    fn an_unknown_action_in_setup_is_a_config_error_and_a_setup_error() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("atlas.yaml"),
            "name: t\nservices:\n  a:\n    command: x\nworktree:\n  setup: '{{action: Nope}}'\n",
        )
        .unwrap();
        let (cfg, errors) = load_checked(dir.path()).unwrap();
        let msg = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            msg.contains("worktree.setup") && msg.contains("Nope"),
            "{msg}"
        );
        assert!(cfg.setup_command().unwrap_err().contains("Nope"));
    }
}
