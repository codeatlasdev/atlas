//! The yank list: releases withdrawn after they were published (`data/yanked.json` on `main`).
//!
//! A tag is immutable and a release stays downloadable, so the withdrawal cannot live in the release
//! itself. Clients fetch this file from `main` at update-check time (see [`crate::github`]), skip every
//! listed version when they pick what a channel offers, and tell a user who runs one. Editing the file
//! takes effect for every client without cutting a release (`docs/release.md`, "Yanking a bad release").

use serde::{Deserialize, Serialize};

use crate::channel;

/// Where the list lives in the repository; clients read `raw.githubusercontent.com/<repo>/main/<PATH>`.
pub const PATH: &str = "data/yanked.json";

/// One withdrawn version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Yank {
    /// `X.Y.Z` or `X.Y.Z-(alpha|beta|rc).N`, no leading `v`.
    pub version: String,
    /// One line for the user: what is wrong with this release.
    pub reason: String,
    /// The version to use instead, when one exists.
    #[serde(default)]
    pub replacement: Option<String>,
}

/// The whole file. Unknown keys are ignored so the format can grow without breaking old clients.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct YankList {
    #[serde(default)]
    pub yanked: Vec<Yank>,
}

impl YankList {
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// The entry for `version_or_tag` (`1.5.0-alpha.3` or `v1.5.0-alpha.3`), compared as SemVer
    /// versions. An entry whose version is not SemVer matches nothing.
    pub fn find(&self, version_or_tag: &str) -> Option<&Yank> {
        let wanted = channel::parse_tag(version_or_tag)?;
        self.yanked.iter().find(|y| {
            channel::parse_tag(&y.version).is_some_and(|v| channel::precedence(&v, &wanted).is_eq())
        })
    }

    pub fn is_yanked(&self, version_or_tag: &str) -> bool {
        self.find(version_or_tag).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file clients really fetch: what a maintainer commits must parse with the type clients use.
    const REAL: &str = include_str!("../../../data/yanked.json");

    #[test]
    fn the_committed_list_parses_and_every_entry_is_well_formed() {
        let list = YankList::parse(REAL).expect("data/yanked.json parses");
        let mut seen = std::collections::HashSet::new();
        for y in &list.yanked {
            let v = channel::parse_tag(&y.version)
                .unwrap_or_else(|| panic!("{:?} is not a version", y.version));
            assert!(
                !y.version.starts_with('v'),
                "{}: no leading v (the tag is v<version>)",
                y.version
            );
            assert!(
                channel::Channel::of_version(&v).is_some(),
                "{}: a label Atlas does not ship",
                y.version
            );
            assert!(seen.insert(v), "{} is listed twice", y.version);
            assert!(!y.reason.trim().is_empty(), "{}: say why", y.version);
            if let Some(r) = &y.replacement {
                assert!(
                    channel::parse_tag(r).is_some() && !list.is_yanked(r),
                    "{}: replacement {r} must be a version that is not yanked itself",
                    y.version
                );
            }
        }
    }

    #[test]
    fn the_path_clients_fetch_is_where_the_file_is() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(PATH);
        assert!(p.is_file(), "{} must exist in the repository", p.display());
    }

    #[test]
    fn a_tag_and_a_version_find_the_same_entry_and_neighbours_are_untouched() {
        let list = YankList::parse(
            r#"{"yanked":[{"version":"1.5.0-alpha.3","reason":"data loss","replacement":"1.5.0-alpha.4"}]}"#,
        )
        .unwrap();
        for q in ["1.5.0-alpha.3", "v1.5.0-alpha.3"] {
            assert_eq!(list.find(q).unwrap().reason, "data loss", "{q}");
        }
        for q in ["1.5.0-alpha.2", "1.5.0-alpha.4", "1.5.0", "nightly", ""] {
            assert!(!list.is_yanked(q), "{q}");
        }
    }

    #[test]
    fn unknown_keys_and_an_empty_object_are_fine_and_garbage_entries_match_nothing() {
        assert!(YankList::parse("{}").unwrap().yanked.is_empty());
        let list = YankList::parse(
            r#"{"min_supported":"1.0.0","yanked":[{"version":"banana","reason":"x","future":1}]}"#,
        )
        .unwrap();
        assert!(!list.is_yanked("1.0.0"));
        assert!(YankList::parse("<html>404</html>").is_err());
    }
}
