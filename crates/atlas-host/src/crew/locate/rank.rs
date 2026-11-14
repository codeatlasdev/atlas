//! The page's element boxes (what `runner.mjs` writes when a capture asks for `dom: true`) and the
//! ranking of the ones a region of a screenshot points at. Pure values and arithmetic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One element of the page: what it is, what it says and where it sits, in CSS px of the page at
/// the capture's width (the origin is the top-left of the whole page, not of the viewport).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Element {
    pub tag: String,
    #[serde(default)]
    pub role: Option<String>,
    /// The accessible name (what a screen reader says).
    #[serde(default)]
    pub name: Option<String>,
    /// Visible text, clipped by the runner.
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub testid: Option<String>,
    /// Every other `data-*` attribute, name without the `data-` prefix.
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    /// A stable CSS selector for the element.
    pub selector: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Nesting depth in the DOM (`<body>` is 0).
    #[serde(default)]
    pub depth: u32,
    /// True for the page's main landmark (`<main>` / `role=main`).
    #[serde(default)]
    pub main: bool,
}

/// The runner's DOM report of one capture cell.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DomReport {
    pub url: String,
    pub width: u32,
    pub page_w: f64,
    pub page_h: f64,
    pub elements: Vec<Element>,
}

/// A region of the page, in CSS px: `[x, y, w, h]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn from_array(a: [f64; 4]) -> Result<Self, String> {
        if a.iter().any(|v| !v.is_finite()) || a[2] <= 0.0 || a[3] <= 0.0 {
            return Err("locate: rect is [x, y, w, h] with a positive width and height".into());
        }
        Ok(Self {
            x: a[0],
            y: a[1],
            w: a[2],
            h: a[3],
        })
    }

    fn area(self) -> f64 {
        self.w * self.h
    }

    /// The area two rectangles share.
    fn overlap(self, o: Rect) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        if w <= 0.0 || h <= 0.0 { 0.0 } else { w * h }
    }
}

/// An element that scored, with the reasons that make the score readable.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub element: Element,
    pub score: f64,
}

/// What an element is worth as an answer, apart from where it is: a `data-testid` is the author
/// naming it, role + accessible name is what a user sees it as, text is the weakest handle that
/// still finds it in the source.
fn specificity(e: &Element) -> f64 {
    if e.testid.as_deref().is_some_and(|t| !t.is_empty()) {
        1.0
    } else if e.role.as_deref().is_some_and(|r| !r.is_empty())
        && e.name.as_deref().is_some_and(|n| !n.is_empty())
    {
        0.7
    } else if e.text.as_deref().is_some_and(|t| !t.trim().is_empty()) {
        0.4
    } else {
        0.1
    }
}

/// The elements a region points at, best first.
///
/// With a `rect`, an element counts when its box intersects it. Its score is the fraction of the
/// element covered by the region times the fraction of the region the element fills (so a button
/// the region is drawn around beats the page-wide container that merely contains it), weighted by
/// [`specificity`] and nudged up by depth (a deeper element is a narrower answer). Without a
/// `rect` the question is "what is this page": the main landmark first, then the biggest named
/// elements.
pub fn rank(dom: &DomReport, rect: Option<Rect>, limit: usize) -> Vec<Ranked> {
    let max_depth = dom
        .elements
        .iter()
        .map(|e| e.depth)
        .max()
        .unwrap_or(0)
        .max(1) as f64;
    let mut out: Vec<Ranked> = vec![];
    for e in &dom.elements {
        if e.w <= 0.0 || e.h <= 0.0 {
            continue;
        }
        let eb = Rect {
            x: e.x,
            y: e.y,
            w: e.w,
            h: e.h,
        };
        let depth_bonus = 1.0 + 0.2 * (e.depth as f64 / max_depth);
        let score = match rect {
            Some(r) => {
                let inter = eb.overlap(r);
                if inter <= 0.0 {
                    continue;
                }
                let of_element = inter / eb.area();
                let of_region = inter / r.area();
                (of_element * of_region).sqrt() * specificity(e) * depth_bonus
            }
            None => {
                let page = (dom.page_w * dom.page_h).max(1.0);
                let size = (eb.area() / page).clamp(0.0, 1.0);
                let main = if e.main { 3.0 } else { 0.0 };
                (main + size.sqrt() * specificity(e)) * depth_bonus
            }
        };
        if score > 0.0 {
            out.push(Ranked {
                element: e.clone(),
                score,
            });
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.element.selector.cmp(&b.element.selector))
    });
    out.truncate(limit);
    out
}

/// "Ambiguous": the best two candidates are within 10% of each other, so a guess could be wrong
/// and the agent should ask the user.
pub fn is_ambiguous(scores: &[f64]) -> bool {
    match scores {
        [a, b, ..] if *a > 0.0 => (a - b) / a <= 0.10,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn el(
        selector: &str,
        tag: &str,
        (x, y, w, h): (f64, f64, f64, f64),
        depth: u32,
        testid: Option<&str>,
        role_name: Option<(&str, &str)>,
        text: Option<&str>,
    ) -> Element {
        Element {
            tag: tag.into(),
            role: role_name.map(|(r, _)| r.into()),
            name: role_name.map(|(_, n)| n.into()),
            text: text.map(Into::into),
            testid: testid.map(Into::into),
            data: Default::default(),
            selector: selector.into(),
            x,
            y,
            w,
            h,
            depth,
            main: tag == "main",
        }
    }

    /// A 1440x2000 page: a header, a main with a hero and a card holding a button, and a footer.
    fn page() -> DomReport {
        DomReport {
            url: "http://localhost:5173/pricing".into(),
            width: 1440,
            page_w: 1440.0,
            page_h: 2000.0,
            elements: vec![
                el(
                    "body",
                    "body",
                    (0.0, 0.0, 1440.0, 2000.0),
                    0,
                    None,
                    None,
                    None,
                ),
                el(
                    "header",
                    "header",
                    (0.0, 0.0, 1440.0, 80.0),
                    1,
                    None,
                    Some(("banner", "")),
                    None,
                ),
                el(
                    "main",
                    "main",
                    (0.0, 80.0, 1440.0, 1700.0),
                    1,
                    None,
                    Some(("main", "")),
                    None,
                ),
                el(
                    "h1",
                    "h1",
                    (200.0, 120.0, 600.0, 60.0),
                    2,
                    None,
                    Some(("heading", "Pricing")),
                    Some("Pricing"),
                ),
                el(
                    "[data-testid=\"plan-card\"]",
                    "div",
                    (200.0, 400.0, 400.0, 300.0),
                    2,
                    Some("plan-card"),
                    None,
                    None,
                ),
                el(
                    "[data-testid=\"buy-button\"]",
                    "button",
                    (240.0, 620.0, 160.0, 48.0),
                    3,
                    Some("buy-button"),
                    Some(("button", "Buy now")),
                    Some("Buy now"),
                ),
                el(
                    "footer",
                    "footer",
                    (0.0, 1780.0, 1440.0, 220.0),
                    1,
                    None,
                    Some(("contentinfo", "")),
                    None,
                ),
            ],
        }
    }

    #[test]
    fn a_rect_over_a_button_picks_the_button() {
        let r = Rect::from_array([235.0, 615.0, 170.0, 58.0]).unwrap();
        let got = rank(&page(), Some(r), 5);
        assert_eq!(
            got[0].element.selector, "[data-testid=\"buy-button\"]",
            "{got:?}"
        );
        assert!(!is_ambiguous(
            &got.iter().map(|g| g.score).collect::<Vec<_>>()
        ));
    }

    #[test]
    fn a_rect_around_a_card_picks_the_card_not_the_page_wide_containers() {
        let r = Rect::from_array([190.0, 390.0, 420.0, 320.0]).unwrap();
        let got = rank(&page(), Some(r), 5);
        assert_eq!(
            got[0].element.testid.as_deref(),
            Some("plan-card"),
            "{got:?}"
        );
    }

    #[test]
    fn elements_the_rect_does_not_touch_are_not_candidates() {
        let r = Rect::from_array([235.0, 615.0, 170.0, 58.0]).unwrap();
        let got = rank(&page(), Some(r), 10);
        assert!(
            got.iter()
                .all(|g| g.element.selector != "footer" && g.element.selector != "h1")
        );
        let far = Rect::from_array([5000.0, 5000.0, 10.0, 10.0]).unwrap();
        assert!(rank(&page(), Some(far), 5).is_empty());
    }

    #[test]
    fn without_a_rect_the_main_landmark_leads() {
        let got = rank(&page(), None, 5);
        assert_eq!(got[0].element.selector, "main", "{got:?}");
    }

    #[test]
    fn two_equal_candidates_are_ambiguous() {
        // Two identical cards side by side; the rect covers exactly half of each.
        let mut d = page();
        d.elements = vec![
            el(
                "[data-testid=\"a\"]",
                "div",
                (0.0, 0.0, 100.0, 100.0),
                1,
                Some("a"),
                None,
                None,
            ),
            el(
                "[data-testid=\"b\"]",
                "div",
                (100.0, 0.0, 100.0, 100.0),
                1,
                Some("b"),
                None,
                None,
            ),
        ];
        let r = Rect::from_array([50.0, 0.0, 100.0, 100.0]).unwrap();
        let got = rank(&d, Some(r), 5);
        assert_eq!(got.len(), 2);
        assert!(
            is_ambiguous(&got.iter().map(|g| g.score).collect::<Vec<_>>()),
            "{got:?}"
        );
    }

    #[test]
    fn ambiguity_is_the_ten_percent_line() {
        assert!(is_ambiguous(&[1.0, 0.91]));
        assert!(is_ambiguous(&[1.0, 0.90]));
        assert!(!is_ambiguous(&[1.0, 0.89]));
        assert!(!is_ambiguous(&[1.0]));
        assert!(!is_ambiguous(&[]));
        assert!(!is_ambiguous(&[0.0, 0.0]));
    }

    #[test]
    fn specificity_orders_testid_over_role_and_name_over_text() {
        // Same box, so only specificity differs.
        let b = (0.0, 0.0, 100.0, 40.0);
        let mut d = page();
        d.elements = vec![
            el("t", "span", b, 1, None, None, Some("Hello")),
            el(
                "r",
                "button",
                b,
                1,
                None,
                Some(("button", "Hello")),
                Some("Hello"),
            ),
            el("i", "div", b, 1, Some("hello-box"), None, None),
        ];
        let r = Rect::from_array([0.0, 0.0, 100.0, 40.0]).unwrap();
        let order: Vec<_> = rank(&d, Some(r), 5)
            .into_iter()
            .map(|g| g.element.selector)
            .collect();
        assert_eq!(order, ["i", "r", "t"]);
    }

    #[test]
    fn a_rect_must_have_a_positive_size() {
        assert!(Rect::from_array([0.0, 0.0, 0.0, 10.0]).is_err());
        assert!(Rect::from_array([0.0, 0.0, 10.0, -1.0]).is_err());
        assert!(Rect::from_array([f64::NAN, 0.0, 10.0, 10.0]).is_err());
    }

    #[test]
    fn the_dom_report_reads_what_the_runner_writes() {
        let json = r#"{"url":"http://x/","width":390,"page_w":390,"page_h":800,"elements":[
            {"tag":"button","role":"button","name":"Go","text":"Go","testid":"go","data":{"k":"v"},
             "selector":"[data-testid=\"go\"]","x":10,"y":20,"w":100,"h":40,"depth":2,"main":false}]}"#;
        let d: DomReport = serde_json::from_str(json).unwrap();
        assert_eq!(d.elements[0].data["k"], "v");
        assert_eq!(d.elements[0].testid.as_deref(), Some("go"));
    }
}
