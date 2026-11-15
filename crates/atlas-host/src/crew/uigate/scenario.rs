//! A scenario: one URL seen in a matrix of engine x width x theme. It is the unit the UI gate
//! plans, runs, remembers and replays.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::crew::capture::spec::{Engine, Theme};
use crate::crew::capture::{Combo, Spec};

/// Why a scenario is in the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The change touches a route that renders at this URL.
    Route,
    /// It failed once and was registered with `add_regression`: always replayed.
    Regression,
}

/// What to look at. The capture options are the `capture` tool's (same names, same defaults), so
/// a scenario can always be reproduced by hand with `capture`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub url: String,
    #[serde(default = "default_widths")]
    pub widths: Vec<u32>,
    #[serde(default = "default_themes")]
    pub themes: Vec<Theme>,
    #[serde(default = "default_engines")]
    pub engines: Vec<Engine>,
}

fn default_widths() -> Vec<u32> {
    crate::crew::capture::spec::DEFAULT_WIDTHS.to_vec()
}
fn default_themes() -> Vec<Theme> {
    vec![Theme::Light, Theme::Dark]
}
fn default_engines() -> Vec<Engine> {
    vec![Engine::Chromium, Engine::Webkit]
}

impl Scenario {
    /// The default matrix of `url`.
    pub fn for_url(url: &str) -> Self {
        Self {
            url: url.to_string(),
            widths: default_widths(),
            themes: default_themes(),
            engines: default_engines(),
        }
    }

    /// The `capture` request for this scenario; validates it the way the tool does. `checks` asks
    /// the runner to run the UI detectors in each page (the gate's detectors need it).
    pub fn spec(&self, checks: bool) -> Result<Spec, String> {
        Spec::from_args(&serde_json::json!({
            "url": self.url, "widths": self.widths, "themes": self.themes, "engines": self.engines,
            "checks": checks,
        }))
    }

    /// Every cell, sorted: two scenarios that ask for the same cells have the same [`id`](Self::id)
    /// however their lists were written.
    pub fn combos(&self) -> Result<Vec<Combo>, String> {
        self.spec(false).map(|s| s.combos())
    }

    /// A short stable name: 12 hex digits of sha256 over the URL and the sorted cells.
    pub fn id(&self) -> Result<String, String> {
        let mut h = Sha256::new();
        h.update(self.url.as_bytes());
        for c in self.combos()? {
            h.update(format!("|{c}"));
        }
        Ok(hex::encode(h.finalize())[..12].to_string())
    }
}
