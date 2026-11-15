//! The point where the detectors plug in. A [`Check`] looks at one finished capture and says what is
//! wrong with it (overflow, clipped text, low contrast…). The detectors live upstream
//! (codeatlasdev/ui#1290); until that lands, [`Detectors`] reads what the TEMPORARY local ones
//! (`capture/detectors.mjs`, #215) found while the page was open.

use std::path::Path;

use crate::crew::capture::Combo;
use crate::crew::capture::detectors::{self, Severity};

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

/// The UI detectors (#215), TEMPORARY until ui#1290 ships. They ran in the page during the capture
/// (the scenario's spec asks for `checks`); this reads the findings file the runner wrote next to
/// the picture. A finding of severity `error` fails the scenario; a `warn` does not (it is kept in
/// the file next to the shots). A cell with no findings file, or one that is not a list of
/// findings, is a check that could not run: the gate must not pass what it could not look at.
pub struct Detectors;

impl Check for Detectors {
    fn name(&self) -> &str {
        "ui-detectors"
    }

    fn run(&self, _combo: Combo, png: &Path) -> Result<Vec<Finding>, String> {
        let file = detectors::findings_path(png);
        if !file.is_file() {
            return Err(format!(
                "the runner wrote no {}: {}",
                file.display(),
                detectors::OLD_RUNNER_HINT
            ));
        }
        Ok(detectors::read(&file)?
            .into_iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| Finding {
                check: f.detector,
                message: match f.selector {
                    Some(s) => format!("{} ({s})", f.message),
                    None => f.message,
                },
            })
            .collect())
    }
}

/// The checks every gate run applies.
pub fn registered() -> Vec<Box<dyn Check>> {
    vec![Box::new(Detectors)]
}

/// Whether a registered check needs the page's detectors to run during the capture.
pub fn wants_detectors(checks: &[Box<dyn Check>]) -> bool {
    checks.iter().any(|c| c.name() == Detectors.name())
}
