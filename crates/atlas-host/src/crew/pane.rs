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

/// The task id of a pane of crewmate `id`, `None` for any other pane or a task part that is not a plain
/// name. The crewmate writes this key, so the task part is as untrusted as the key: it is what hostd
/// joins to a folder, so it must be one path component (see [`is_plain_name`]).
pub(crate) fn task_id<'a>(pane_key: &'a str, id: &str) -> Option<&'a str> {
    if !is_pane_of(pane_key, id) {
        return None;
    }
    pane_key.splitn(3, ':').nth(2).filter(|t| is_plain_name(t))
}

/// Whether `name` is one normal path component: not empty, not `.` or `..`, no separator, no NUL, never
/// absolute. `dir.join(name)` then stays directly under `dir`.
pub(crate) fn is_plain_name(name: &str) -> bool {
    let mut parts = std::path::Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(n)), None) if n == name
    ) && !name.contains(['\\', '\0'])
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

    #[test]
    fn a_task_id_is_one_plain_path_component_of_the_callers_own_pane() {
        assert_eq!(task_id("crew:abc:t1", "abc"), Some("t1"));
        assert_eq!(task_id("crew:abc:t1", "other"), None);
        assert_eq!(task_id("crew:abc:", "abc"), None);
        assert_eq!(task_id("crew:abc", "abc"), None);
        for bad in [
            "/etc",
            "/var/lib/x/work/y",
            "..",
            ".",
            "../x",
            "a/b",
            "a\\b",
            "x/..",
            "a\0b",
        ] {
            assert_eq!(task_id(&format!("crew:abc:{bad}"), "abc"), None, "{bad:?}");
            assert!(!is_plain_name(bad), "{bad:?}");
        }
        assert!(is_plain_name("task-9_x.y"));
    }
}
