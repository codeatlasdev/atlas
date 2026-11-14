//! The plan of one gate run: which scenarios to capture for this change. Pure: the changed files
//! and the project's routes are inputs, so the same change always plans the same scenarios.

use std::path::Path;

use atlas_core::affected::{self, ActionInputs};

use super::regressions::Regression;
use super::scenario::{Scenario, Why};

/// A scenario and the reason it is planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub id: String,
    pub scenario: Scenario,
    pub why: Why,
}

/// `base_url` + `route`: `http://localhost:5173` and `/docs` make `http://localhost:5173/docs`. A
/// route that already is a URL is taken as it is. `Err` for a route that is not a path
/// (a glob or `GET /api` style entry is not a page).
pub fn url_of(base_url: &str, route: &str) -> Result<String, String> {
    let route = route.trim();
    if route.starts_with("http://") || route.starts_with("https://") {
        return Ok(route.to_string());
    }
    if !route.starts_with('/') || route.contains(['*', ' ']) {
        return Err(format!(
            "route '{route}' is not a page path (it must start with / and have no wildcard)"
        ));
    }
    Ok(format!("{}{route}", base_url.trim_end_matches('/')))
}

/// The routes the change touches, from the project's declared graph (`atlas.yaml` `affected:`,
/// `atlas_core::affected::resolve`, the same answer as the MCP tool `atlas_affected`).
pub fn routes_of(root: &Path, files: &[String]) -> Result<Vec<String>, String> {
    let (cfg, _) = atlas_tui::config::load_or_detect(root).map_err(|e| format!("{e:#}"))?;
    let actions: Vec<ActionInputs> = cfg
        .actions
        .iter()
        .filter_map(|a| {
            a.cache.as_ref().map(|c| ActionInputs {
                label: a.label.clone(),
                inputs: c.inputs.clone(),
            })
        })
        .collect();
    let declared = cfg.affected.as_ref().map(|a| a.declared());
    affected::resolve(declared.as_ref(), &actions, root, files).map(|(_, i)| i.routes)
}

/// The scenarios for `routes` (default matrix each) plus every regression, deduplicated by id
/// (a regression whose URL is also a touched route is planned once, as a regression: it keeps
/// its own widths and themes).
pub fn plan(
    base_url: &str,
    routes: &[String],
    regressions: &[Regression],
) -> Result<Vec<Planned>, String> {
    let mut out: Vec<Planned> = vec![];
    let mut push = |scenario: Scenario, why: Why| -> Result<(), String> {
        let id = scenario.id()?;
        if let Some(p) = out.iter_mut().find(|p| p.id == id) {
            if why == Why::Regression {
                p.why = why;
            }
        } else {
            out.push(Planned { id, scenario, why });
        }
        Ok(())
    };
    for r in regressions {
        push(r.scenario.clone(), Why::Regression)?;
    }
    for route in routes {
        let url = url_of(base_url, route)?;
        // Validate here so a bad base_url fails the plan, not a capture later.
        let s = Scenario::for_url(&url);
        s.spec()?;
        push(s, Why::Route)?;
    }
    Ok(out)
}
