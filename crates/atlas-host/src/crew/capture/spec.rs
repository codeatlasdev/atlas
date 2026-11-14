//! What a capture asks for and what the runner answers: the spec (one URL seen in a matrix of
//! engine × width × theme), its validation, the runner's report, and the rule that every requested
//! combination must be in it. Pure values, no I/O: `mod.rs` runs the runner.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Widths a capture visits when none are asked for: a phone and a desktop.
pub const DEFAULT_WIDTHS: [u32; 2] = [390, 1440];
/// The most combinations (engine × width × theme) one call may ask for.
pub const MAX_COMBOS: usize = 16;
/// The most steps one call may run before the screenshot.
pub const MAX_STEPS: usize = 20;
/// Narrowest and widest viewport accepted (a 0 px or 100 000 px page is a mistake, not a test).
const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 200..=4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Chromium,
    Webkit,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chromium => "chromium",
            Self::Webkit => "webkit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// One thing to do on the page before the screenshot. Selectors and text only: nothing here runs
/// script, so a capture cannot be turned into code execution in the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Step {
    Click { selector: String },
    Fill { selector: String, text: String },
    Press { key: String },
    WaitFor { selector: String },
    WaitMs { ms: u32 },
}

/// The capture as the agent asked for it, defaults filled in by [`Spec::from_args`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Spec {
    pub url: String,
    pub widths: Vec<u32>,
    pub themes: Vec<Theme>,
    pub engines: Vec<Engine>,
    pub slow_network: bool,
    pub full_page: bool,
    pub steps: Vec<Step>,
    /// Also write the element boxes of the page next to each PNG (`*.dom.json`), for `locate`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dom: bool,
    /// Also run the UI detectors on each page and write their findings next to each PNG
    /// (`*.findings.json`, see `detectors.mjs`). The UI gate asks for it; a plain capture does not.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub checks: bool,
}

/// One (engine, width, theme) cell of the matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Combo {
    pub engine: Engine,
    pub width: u32,
    pub theme: Theme,
}

impl std::fmt::Display for Combo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}px/{}",
            self.engine.as_str(),
            self.width,
            self.theme.as_str()
        )
    }
}

impl Spec {
    /// Parse the MCP tool's arguments: missing fields take their defaults, unknown values are errors.
    pub fn from_args(args: &Value) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            url: String,
            widths: Option<Vec<u32>>,
            themes: Option<Vec<Theme>>,
            engines: Option<Vec<Engine>>,
            slow_network: Option<bool>,
            full_page: Option<bool>,
            steps: Option<Vec<Step>>,
            dom: Option<bool>,
            checks: Option<bool>,
        }
        let raw: Raw = serde_json::from_value(args.clone())
            .map_err(|e| format!("capture: bad arguments: {e}"))?;
        let spec = Self {
            url: raw.url.trim().to_string(),
            widths: raw.widths.unwrap_or_else(|| DEFAULT_WIDTHS.to_vec()),
            themes: raw
                .themes
                .unwrap_or_else(|| vec![Theme::Light, Theme::Dark]),
            engines: raw
                .engines
                .unwrap_or_else(|| vec![Engine::Chromium, Engine::Webkit]),
            slow_network: raw.slow_network.unwrap_or(false),
            full_page: raw.full_page.unwrap_or(true),
            steps: raw.steps.unwrap_or_default(),
            dom: raw.dom.unwrap_or(false),
            checks: raw.checks.unwrap_or(false),
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Every cell the runner must report, sorted and without repeats.
    pub fn combos(&self) -> Vec<Combo> {
        let mut set = BTreeSet::new();
        for &engine in &self.engines {
            for &width in &self.widths {
                for &theme in &self.themes {
                    set.insert(Combo {
                        engine,
                        width,
                        theme,
                    });
                }
            }
        }
        set.into_iter().collect()
    }

    fn validate(&self) -> Result<(), String> {
        check_url(&self.url)?;
        if self.widths.is_empty() || self.themes.is_empty() || self.engines.is_empty() {
            return Err("capture: widths, themes and engines cannot be empty".into());
        }
        if let Some(w) = self.widths.iter().find(|w| !WIDTH_RANGE.contains(w)) {
            return Err(format!(
                "capture: width {w} is outside {}..={} px",
                WIDTH_RANGE.start(),
                WIDTH_RANGE.end()
            ));
        }
        let n = self.combos().len();
        if n > MAX_COMBOS {
            return Err(format!(
                "capture: {n} combinations asked (engines × widths × themes), at most {MAX_COMBOS}: split it into several calls"
            ));
        }
        if self.steps.len() > MAX_STEPS {
            return Err(format!(
                "capture: {} steps, at most {MAX_STEPS}",
                self.steps.len()
            ));
        }
        Ok(())
    }
}

/// http(s) only, with a host. localhost and LAN addresses are allowed on purpose: the thing to look
/// at is usually a dev server on the crew server itself. `file:`, `javascript:`, `data:` and the
/// like are refused: they read local files or run script instead of looking at a page.
fn check_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| {
            format!("capture: only http:// and https:// URLs are accepted, got '{url}'")
        })?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    if host.is_empty() || host.starts_with(':') {
        return Err(format!("capture: '{url}' has no host"));
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("capture: the URL has spaces or control characters".into());
    }
    Ok(())
}

/// One entry of the runner's report: a combination and the PNG it wrote, or why it failed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Shot {
    pub engine: Engine,
    pub width: u32,
    pub theme: Theme,
    #[serde(default)]
    pub path: Option<String>,
    /// The element boxes of the page, when the spec asked for `dom`.
    #[serde(default)]
    pub dom: Option<String>,
    /// The detectors' findings, when the spec asked for `checks`.
    #[serde(default)]
    pub findings: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Shot {
    pub fn combo(&self) -> Combo {
        Combo {
            engine: self.engine,
            width: self.width,
            theme: self.theme,
        }
    }
}

/// One checked cell of a report: the combination, its PNG and, when asked for, its DOM and
/// findings files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub combo: Combo,
    pub png: String,
    pub dom: Option<String>,
    pub findings: Option<String>,
}

/// The runner's JSON report.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Report {
    pub shots: Vec<Shot>,
}

/// Parse the runner's stdout. The runner prints one JSON document; anything else is an error that
/// quotes what came out.
pub fn parse_report(stdout: &str) -> Result<Report, String> {
    serde_json::from_str(stdout.trim()).map_err(|e| {
        let head: String = stdout.chars().take(300).collect();
        format!("capture: the runner's report is not valid ({e}); it printed: {head}")
    })
}

/// The rule of the tool: every requested combination is in the report with a PNG. A missing
/// combination, a failed one, or a path-less entry is an error that names each of them. With `dom`
/// asked for, a cell without its DOM file is a failed cell too, and so is one without its findings
/// file when `checks` was asked for (a gate that could not look must not pass). Returns every cell,
/// in matrix order.
pub fn check_report(spec: &Spec, report: &Report) -> Result<Vec<Cell>, String> {
    let mut found: std::collections::BTreeMap<Combo, &Shot> = Default::default();
    for s in &report.shots {
        found.insert(s.combo(), s);
    }
    let mut missing = vec![];
    let mut failed = vec![];
    let mut out = vec![];
    for c in spec.combos() {
        match found.get(&c) {
            None => missing.push(c.to_string()),
            Some(s) => match (&s.path, &s.error) {
                (Some(p), None) if !p.is_empty() => {
                    let dom = s.dom.clone().filter(|d| !d.is_empty());
                    let findings = s.findings.clone().filter(|d| !d.is_empty());
                    if spec.dom && dom.is_none() {
                        failed.push(format!("{c}: no DOM report written"));
                    } else if spec.checks && findings.is_none() {
                        failed.push(format!("{c}: no findings written"));
                    } else {
                        out.push(Cell {
                            combo: c,
                            png: p.clone(),
                            dom,
                            findings,
                        });
                    }
                }
                (_, Some(e)) => failed.push(format!("{c}: {e}")),
                _ => failed.push(format!("{c}: no screenshot written")),
            },
        }
    }
    if missing.is_empty() && failed.is_empty() {
        return Ok(out);
    }
    let mut msg = String::from("capture incomplete.");
    if !missing.is_empty() {
        msg.push_str(&format!(" Missing combinations: {}.", missing.join(", ")));
    }
    if !failed.is_empty() {
        msg.push_str(&format!(" Failed: {}.", failed.join("; ")));
    }
    Err(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec(args: Value) -> Result<Spec, String> {
        Spec::from_args(&args)
    }

    #[test]
    fn defaults_are_the_two_widths_both_themes_both_engines_full_page() {
        let s = spec(json!({"url": "http://localhost:5173/"})).unwrap();
        assert_eq!(s.widths, vec![390, 1440]);
        assert_eq!(s.themes, vec![Theme::Light, Theme::Dark]);
        assert_eq!(s.engines, vec![Engine::Chromium, Engine::Webkit]);
        assert!(s.full_page && !s.slow_network && s.steps.is_empty());
        assert_eq!(s.combos().len(), 8);
    }

    #[test]
    fn only_http_and_https_with_a_host_are_accepted() {
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ftp://example.com/",
            "example.com",
            "http://",
            "https:///path",
            "http://:8080/",
            "http://exa mple.com/",
        ] {
            let e = spec(json!({"url": bad})).unwrap_err();
            assert!(e.contains("capture:"), "{bad}: {e}");
        }
        for ok in [
            "http://localhost:3000",
            "https://example.com/a?b=c#d",
            "http://192.168.1.20:8080/x",
            "http://[::1]:3000/",
            "http://user:pw@intranet/",
        ] {
            spec(json!({"url": ok})).unwrap_or_else(|e| panic!("{ok}: {e}"));
        }
    }

    #[test]
    fn too_many_combinations_is_refused_with_the_count() {
        let e = spec(json!({"url": "http://x/", "widths": [300, 390, 600, 800, 1024, 1440, 1920, 2560, 3000]}))
            .unwrap_err();
        assert!(
            e.contains("36 combinations") && e.contains("at most 16"),
            "{e}"
        );
    }

    #[test]
    fn repeated_values_do_not_count_twice() {
        let s = spec(json!({"url": "http://x/", "widths": [390, 390], "themes": ["dark", "dark"]}))
            .unwrap();
        assert_eq!(s.combos().len(), 2, "two engines × one width × one theme");
    }

    #[test]
    fn empty_lists_unknown_values_and_odd_widths_are_refused() {
        assert!(spec(json!({"url": "http://x/", "widths": []})).is_err());
        assert!(spec(json!({"url": "http://x/", "engines": ["firefox"]})).is_err());
        assert!(spec(json!({"url": "http://x/", "themes": ["sepia"]})).is_err());
        assert!(
            spec(json!({"url": "http://x/", "widths": [50]}))
                .unwrap_err()
                .contains("width 50")
        );
        assert!(
            spec(json!({"url": "http://x/", "extra": 1})).is_err(),
            "unknown field"
        );
        assert!(spec(json!({})).is_err(), "url is required");
    }

    #[test]
    fn steps_are_typed_and_capped() {
        let s = spec(json!({"url": "http://x/", "steps": [
            {"do": "click", "selector": "#menu"},
            {"do": "fill", "selector": "input", "text": "hi"},
            {"do": "press", "key": "Enter"},
            {"do": "wait_for", "selector": ".ready"},
            {"do": "wait_ms", "ms": 200}
        ]}))
        .unwrap();
        assert_eq!(s.steps.len(), 5);
        assert!(
            spec(json!({"url": "http://x/", "steps": [{"do": "eval", "code": "1"}]})).is_err(),
            "no script steps"
        );
        let many: Vec<Value> = (0..21)
            .map(|_| json!({"do": "press", "key": "a"}))
            .collect();
        assert!(
            spec(json!({"url": "http://x/", "steps": many}))
                .unwrap_err()
                .contains("21 steps")
        );
    }

    fn shot(engine: &str, width: u32, theme: &str) -> Value {
        json!({"engine": engine, "width": width, "theme": theme, "path": format!("/c/{engine}-{width}-{theme}.png")})
    }

    fn full() -> Vec<Value> {
        let mut v = vec![];
        for e in ["chromium", "webkit"] {
            for w in [390, 1440] {
                for t in ["light", "dark"] {
                    v.push(shot(e, w, t));
                }
            }
        }
        v
    }

    #[test]
    fn a_full_report_lists_every_cell_with_its_file() {
        let s = spec(json!({"url": "http://x/"})).unwrap();
        let r = parse_report(&json!({"shots": full()}).to_string()).unwrap();
        let got = check_report(&s, &r).unwrap();
        assert_eq!(got.len(), 8);
        assert!(
            got.iter()
                .any(|c| c.combo.to_string() == "webkit/390px/dark"
                    && c.png == "/c/webkit-390-dark.png"
                    && c.dom.is_none())
        );
    }

    #[test]
    fn dom_is_off_by_default_and_not_sent_to_the_runner_unless_asked() {
        let s = spec(json!({"url": "http://x/"})).unwrap();
        assert!(!s.dom);
        assert!(
            serde_json::to_value(&s).unwrap().get("dom").is_none(),
            "the runner contract of a plain capture is unchanged"
        );
        let s = spec(json!({"url": "http://x/", "dom": true})).unwrap();
        assert_eq!(serde_json::to_value(&s).unwrap()["dom"], true);
    }

    #[test]
    fn with_dom_every_cell_must_name_its_dom_file() {
        let s = spec(json!({"url": "http://x/", "dom": true, "engines": ["webkit"], "widths": [390], "themes": ["light", "dark"]})).unwrap();
        let mut light = shot("webkit", 390, "light");
        light["dom"] = json!("/c/webkit-390-light.dom.json");
        // dark has a PNG but no DOM report.
        let dark = shot("webkit", 390, "dark");
        let r = parse_report(&json!({"shots": [light.clone(), dark]}).to_string()).unwrap();
        let e = check_report(&s, &r).unwrap_err();
        assert!(
            e.contains("webkit/390px/dark: no DOM report written"),
            "{e}"
        );

        let mut dark = shot("webkit", 390, "dark");
        dark["dom"] = json!("/c/webkit-390-dark.dom.json");
        let r = parse_report(&json!({"shots": [light, dark]}).to_string()).unwrap();
        let got = check_report(&s, &r).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|c| c.dom.is_some()));
    }

    #[test]
    fn a_missing_combination_is_an_error_naming_it() {
        let s = spec(json!({"url": "http://x/"})).unwrap();
        let mut shots = full();
        shots.retain(|v| !(v["engine"] == "webkit" && v["width"] == 390 && v["theme"] == "dark"));
        shots.retain(|v| {
            !(v["engine"] == "chromium" && v["width"] == 1440 && v["theme"] == "light")
        });
        let r = parse_report(&json!({"shots": shots}).to_string()).unwrap();
        let e = check_report(&s, &r).unwrap_err();
        assert!(
            e.contains("webkit/390px/dark") && e.contains("chromium/1440px/light"),
            "{e}"
        );
        assert!(e.contains("Missing"), "{e}");
    }

    #[test]
    fn a_failed_cell_or_one_without_a_file_is_an_error_naming_it() {
        let s = spec(json!({"url": "http://x/", "engines": ["webkit"], "widths": [390]})).unwrap();
        let r = parse_report(
            &json!({"shots": [
                {"engine": "webkit", "width": 390, "theme": "light", "error": "net::ERR_CONNECTION_REFUSED"},
                {"engine": "webkit", "width": 390, "theme": "dark"}
            ]})
            .to_string(),
        )
        .unwrap();
        let e = check_report(&s, &r).unwrap_err();
        assert!(
            e.contains("webkit/390px/light: net::ERR_CONNECTION_REFUSED"),
            "{e}"
        );
        assert!(
            e.contains("webkit/390px/dark: no screenshot written"),
            "{e}"
        );
    }

    #[test]
    fn extra_cells_in_the_report_are_ignored_and_garbage_is_an_error() {
        let s = spec(json!({"url": "http://x/", "engines": ["webkit"], "widths": [390], "themes": ["light"]})).unwrap();
        let r = parse_report(&json!({"shots": full()}).to_string()).unwrap();
        assert_eq!(check_report(&s, &r).unwrap().len(), 1);
        let e = parse_report("Error: browserType.launch: Executable doesn't exist").unwrap_err();
        assert!(e.contains("Executable doesn't exist"), "{e}");
    }
}
