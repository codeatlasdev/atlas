//! An update-check failure (`atlas_core::github::GithubError`) in words a person reads. `atlas-core` writes the
//! English of its errors (`Display`, logs, `--json`); the CLI (`caused by:` lines) and the desktop shell (`u.error`, the
//! What's new sheet) show these instead. The reason inside is the underlying tool's own message (curl's, the feed's)
//! and is kept as it is; the sentence around it is the catalog's.

use atlas_core::github::GithubError;

/// `e` in the language `lang` (a catalog tag).
pub fn github_error_in(lang: &str, e: &GithubError) -> String {
    match e {
        GithubError::Missing => crate::t_in!(lang, "native_updates_github_missing"),
        GithubError::Failed(reason) => {
            crate::t_in!(lang, "native_updates_github_failed", reason = reason)
        }
        GithubError::Feed(reason) => {
            crate::t_in!(lang, "native_updates_github_feed", reason = reason)
        }
        GithubError::Unknown { tag, reason } => crate::t_in!(
            lang,
            "native_updates_github_unknown",
            tag = tag,
            reason = reason
        ),
    }
}

/// `e` in the display language.
pub fn github_error(e: &GithubError) -> String {
    github_error_in(crate::lang(), e)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sentence follows the language, the reason of the underlying tool is kept, and a missing curl is named.
    #[test]
    fn a_github_failure_is_worded_in_the_asked_language_and_keeps_its_reason() {
        let cases = [
            (
                GithubError::Failed("curl: (6) no host".into()),
                "curl: (6) no host",
            ),
            (GithubError::Feed("bad atom".into()), "bad atom"),
            (
                GithubError::Unknown {
                    tag: "v1.4.0".into(),
                    reason: "timed out".into(),
                },
                "timed out",
            ),
            (GithubError::Missing, "curl"),
        ];
        for (e, kept) in &cases {
            let (en, pt) = (github_error_in("en", e), github_error_in("pt-BR", e));
            assert!(en.contains(kept) && pt.contains(kept), "{en} / {pt}");
            assert_ne!(en, pt, "the sentence must follow the language");
            assert_ne!(en, e.to_string(), "not the English of atlas-core: {e}");
        }
    }
}
