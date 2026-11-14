//! The point where the detectors plug in. A [`Check`] looks at one finished capture and says what is
//! wrong with it (overflow, clipped text, low contrast…). The detectors live upstream
//! (codeatlasdev/ui#1290); until they land the registry is empty and a scenario passes when every
//! cell captured.

use std::path::Path;

use crate::crew::capture::Combo;

/// One thing a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The check's [`name`](Check::name).
    pub check: String,
    pub message: String,
}

/// A detector over one capture.
pub trait Check: Send + Sync {
    /// Stable name, shown in findings.
    fn name(&self) -> &str;
    /// Look at the screenshot of `combo` at `png`. `Err` is a check that could not run: it fails
    /// the scenario like a finding does (a gate that cannot look must not pass).
    fn run(&self, combo: Combo, png: &Path) -> Result<Vec<Finding>, String>;
}

/// The checks every gate run applies. Empty today; the detectors register here.
pub fn registered() -> Vec<Box<dyn Check>> {
    vec![]
}
