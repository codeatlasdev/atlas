//! The pane key of a crewmate's task: `crew:<crewmate id>:<task id>`. A task runs in a pane of its
//! own, and hostd tells crewmate panes from terminals by this prefix (approvals wait longer and can
//! be answered from Telegram or the phone). The one place that builds or reads the format.

const PREFIX: &str = "crew:";

/// The pane key of `task_id` of `crewmate_id`.
pub(crate) fn key(crewmate_id: &str, task_id: &str) -> String {
    format!("{PREFIX}{crewmate_id}:{task_id}")
}

/// Whether `pane_key` belongs to a crewmate's task (any crewmate).
pub(crate) fn is_crewmate_pane(pane_key: &str) -> bool {
    pane_key.starts_with(PREFIX)
}

/// The crewmate id of a crewmate pane key, `None` for any other pane.
pub(crate) fn crewmate_id(pane_key: &str) -> Option<&str> {
    pane_key.strip_prefix(PREFIX)?.split(':').next()
}

/// Whether `pane_key` is a pane of the crewmate `id`.
pub(crate) fn is_pane_of(pane_key: &str, id: &str) -> bool {
    crewmate_id(pane_key) == Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guards the rename Bot -> Crew: the key is `crew:<id>:<task>`, never the old `bot:` prefix.
    #[test]
    fn a_key_round_trips_and_only_crewmate_panes_match() {
        let k = key("abc", "t1");
        assert_eq!(k, "crew:abc:t1");
        assert!(is_crewmate_pane(&k));
        assert_eq!(crewmate_id(&k), Some("abc"));
        assert!(is_pane_of(&k, "abc"));
        assert!(!is_pane_of(&k, "ab"), "a prefix of the id is not the id");
        assert!(!is_crewmate_pane("p1"));
        assert!(!is_crewmate_pane("bot:abc:t1"), "the old prefix is gone");
        assert_eq!(crewmate_id("p1"), None);
    }
}
