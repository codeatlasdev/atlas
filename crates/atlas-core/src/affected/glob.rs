//! Glob matching of project-relative, `/`-separated paths, with the same pattern syntax as the
//! task cache's `inputs:` (gitignore-style: `**` crosses folders, `*` does not, a pattern with a
//! slash is anchored to the project root).

use ignore::overrides::{Override, OverrideBuilder};
use std::path::Path;

/// A compiled set of globs: a path matches when any glob does.
pub(super) struct Globs(Override);

impl Globs {
    /// `Err` names the first glob that does not parse.
    pub(super) fn new(globs: &[String]) -> Result<Self, String> {
        let mut b = OverrideBuilder::new("/");
        for g in globs {
            b.add(g).map_err(|e| format!("bad glob {g:?}: {e}"))?;
        }
        b.build()
            .map(Self)
            .map_err(|e| format!("bad glob set {globs:?}: {e}"))
    }

    pub(super) fn is_match(&self, rel: &str) -> bool {
        self.0.matched(Path::new(rel), false).is_whitelist()
    }
}
