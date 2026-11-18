//! The words of a deploy run the host's driver writes (`state.json` phase, a release's status, the `mode` and
//! `result` of `deploys.jsonl`), shown to a person in the display language. The host writes them in English and
//! machine output (`--json`) keeps them as written; a word this build does not know is shown as it is.

use atlas_i18n::{t, t_in};

/// The phase or status `raw` in the display language.
pub(super) fn phase_label(raw: &str) -> String {
    phase_label_in(atlas_i18n::lang(), raw)
}

pub(super) fn phase_label_in(tag: &str, raw: &str) -> String {
    match raw {
        "preflight" => t_in!(tag, "cmd_deploy_phase_preflight"),
        "uploaded" => t_in!(tag, "cmd_deploy_phase_uploaded"),
        "sealed" => t_in!(tag, "cmd_deploy_phase_sealed"),
        "installing" => t_in!(tag, "cmd_deploy_phase_installing"),
        "migrating" => t_in!(tag, "cmd_deploy_phase_migrating"),
        "pre_swap" => t_in!(tag, "cmd_deploy_phase_pre_swap"),
        "swapped" => t_in!(tag, "cmd_deploy_phase_swapped"),
        "post_swap" => t_in!(tag, "cmd_deploy_phase_post_swap"),
        "restarting" => t_in!(tag, "cmd_deploy_phase_restarting"),
        "rolling_back" => t_in!(tag, "cmd_deploy_phase_rolling_back"),
        "rolled_back" => t_in!(tag, "cmd_deploy_phase_rolled_back"),
        "healthy" => t_in!(tag, "cmd_deploy_phase_healthy"),
        "noop" => t_in!(tag, "cmd_deploy_phase_noop"),
        "failed" => t_in!(tag, "cmd_deploy_phase_failed"),
        "locked" => t_in!(tag, "cmd_deploy_phase_locked"),
        "unknown" => t_in!(tag, "cmd_deploy_phase_unknown"),
        other => other.to_string(),
    }
}

/// The `mode` of a run (`deploy` or `rollback`) in the display language.
pub(super) fn mode_label(raw: &str) -> String {
    match raw {
        "deploy" => t!("cmd_deploy_mode_deploy"),
        "rollback" => t!("cmd_deploy_mode_rollback"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every word the driver script can write has a message in every language: the contract with `driver.sh`.
    #[test]
    fn every_phase_the_driver_writes_has_a_label_in_every_language() {
        let driver = include_str!("driver.sh");
        let mut words: Vec<String> = ["locked", "unknown"].map(String::from).to_vec();
        for line in driver.lines() {
            let line = line.trim();
            // `state <phase> ...`, `meta <id> <status>`, `audit <result>` and the `phase:VAR` hook list.
            for key in ["state ", "audit "] {
                if let Some(rest) = line.find(key).map(|i| &line[i + key.len()..]) {
                    let w: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                        .collect();
                    if !w.is_empty() {
                        words.push(w);
                    }
                }
            }
            if let Some(rest) = line.strip_prefix("for step in ") {
                for pair in rest.trim_end_matches("; do").split(' ') {
                    if let Some((phase, _)) = pair.split_once(':') {
                        words.push(phase.to_string());
                    }
                }
            }
            if let Some(i) = line.find("meta \"$")
                && let Some(w) = line[i..].split(' ').nth(2)
            {
                words.push(w.trim_matches('"').to_string());
            }
        }
        words.sort();
        words.dedup();
        // The scan must have found the whole vocabulary, not nothing.
        for expect in [
            "preflight",
            "sealed",
            "healthy",
            "rolled_back",
            "noop",
            "failed",
            "pre_swap",
        ] {
            assert!(
                words.iter().any(|w| w == expect),
                "{expect} not found in {words:?}"
            );
        }
        for tag in atlas_i18n::locales() {
            for w in &words {
                let key = format!("cmd_deploy_phase_{w}");
                let label = atlas_i18n::lookup(tag, &key, None);
                assert!(label.is_some(), "{tag}: no message {key} for the word {w}");
                assert_eq!(label, Some(phase_label_in(tag, w)), "{tag}: {w} not mapped");
            }
        }
    }

    #[test]
    fn an_unknown_word_is_shown_as_it_is() {
        assert_eq!(phase_label_in("en", "teleporting"), "teleporting");
        assert_eq!(mode_label("sideways"), "sideways");
    }
}
