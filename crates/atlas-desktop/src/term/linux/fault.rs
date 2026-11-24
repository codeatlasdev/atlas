//! Fault injection for the live proof of the fallback (debug-assertions builds only: a release app has none of this).
//!
//! `ATLAS_TERM_FAULT=open` makes every renderer creation fail, so `term_open` fails and the page must end on xterm.js
//! with the toast. `ATLAS_TERM_FAULT=rebuild` lets the first creation through and fails every later one, so a reload or
//! a stall rebuild fails until `term://unusable` sends every terminal back to xterm.js. Anything else injects nothing.

/// The error to fail the `call`-th (0-based) renderer creation with under `spec`, or `None` to let it through.
pub(super) fn fault(spec: Option<&str>, call: usize) -> Option<String> {
    match spec? {
        "open" => Some("injected fault: renderer creation refused".into()),
        "rebuild" if call > 0 => Some("injected fault: renderer rebuild refused".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::fault;

    #[test]
    fn open_fails_every_creation_and_rebuild_only_the_later_ones() {
        assert!((0..3).all(|n| fault(Some("open"), n).is_some()));
        assert_eq!(fault(Some("rebuild"), 0), None, "the terminal opens");
        assert!(
            (1..4).all(|n| fault(Some("rebuild"), n).is_some()),
            "every rebuild fails"
        );
    }

    #[test]
    fn nothing_is_injected_without_a_known_spec() {
        for spec in [None, Some(""), Some("1"), Some("OPEN"), Some("open ")] {
            assert!((0..3).all(|n| fault(spec, n).is_none()), "{spec:?}");
        }
    }
}
