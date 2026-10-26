//! Every native view of every kind, by `(kind, tab)`, and the page's hit map.
//!
//! This is the one place a kind is known to Rust: a terminal registers its `NSView*` when it opens, a browser its
//! `WKWebView*`, a future kind the same, and the router looks them up here without a match on the kind. Pure (pointers
//! are plain `usize`, rects plain numbers): the AppKit side is `mac`.

use serde::Deserialize;
use std::collections::HashMap;

/// A rect in window points, top-left origin, y down (what `getBoundingClientRect` gives times the layout zoom).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// A rect with finite numbers and a non-empty area; anything else is a rect measured mid-layout.
    pub fn usable(&self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            && self.w > 0.0
            && self.h > 0.0
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// What the page publishes with `natives_hit`: the rects it paints over native views and owns input in (open menus,
/// dialogs, a modal's whole-window scrim, the edge peek). Sent only when it changes.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct HitMap {
    #[serde(default)]
    pub rects: Vec<Rect>,
}

/// One registered native view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    pub ptr: usize,
    /// Does the view take mouse input itself? A browser page does (the router hands it clicks and scrolls over its
    /// slot); a GPU terminal does not (its input is DOM events on the slot, so the page keeps it).
    pub interactive: bool,
}

#[derive(Default)]
pub struct Registry {
    views: HashMap<(String, String), View>,
    overlays: Vec<Rect>,
}

impl Registry {
    /// Register `ptr` for `(kind, tab)`, replacing the view already there (a terminal rebuild swaps its `NSView`).
    /// A destroyed view's pointer may be reused by the next one: whatever still holds it is stale.
    pub fn register(&mut self, kind: &str, tab: &str, ptr: usize, interactive: bool) {
        self.views.retain(|_, v| v.ptr != ptr);
        self.views.insert(
            (kind.to_string(), tab.to_string()),
            View { ptr, interactive },
        );
    }

    pub fn unregister(&mut self, kind: &str, tab: &str) -> Option<usize> {
        self.views
            .remove(&(kind.to_string(), tab.to_string()))
            .map(|v| v.ptr)
    }

    /// The views that take input themselves, in no particular order.
    pub fn interactive(&self) -> Vec<View> {
        self.views
            .values()
            .filter(|v| v.interactive)
            .copied()
            .collect()
    }

    /// Record the page's hit map; unusable rects (NaN, empty) are dropped. `true` when it changed.
    pub fn set_hit_map(&mut self, map: HitMap) -> bool {
        let rects: Vec<Rect> = map.rects.into_iter().filter(Rect::usable).collect();
        if rects == self.overlays {
            return false;
        }
        self.overlays = rects;
        true
    }

    pub fn overlays(&self) -> &[Rect] {
        &self.overlays
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ptr(r: &Registry, kind: &str, tab: &str) -> Option<usize> {
        r.views
            .get(&(kind.to_string(), tab.to_string()))
            .map(|v| v.ptr)
    }

    #[test]
    fn a_view_is_found_by_kind_and_tab_and_gone_after_unregister() {
        let mut r = Registry::default();
        r.register("terminal", "t1", 0x10, false);
        r.register("browser", "b1", 0x20, true);
        assert_eq!(ptr(&r, "terminal", "t1"), Some(0x10));
        assert_eq!(ptr(&r, "browser", "b1"), Some(0x20));
        assert_eq!(ptr(&r, "browser", "t1"), None);
        assert_eq!(r.unregister("terminal", "t1"), Some(0x10));
        assert_eq!(ptr(&r, "terminal", "t1"), None);
        assert_eq!(r.unregister("terminal", "t1"), None);
    }

    #[test]
    fn the_same_tab_id_under_two_kinds_is_two_views() {
        let mut r = Registry::default();
        r.register("terminal", "x", 1, false);
        r.register("browser", "x", 2, true);
        assert_eq!(ptr(&r, "terminal", "x"), Some(1));
        assert_eq!(ptr(&r, "browser", "x"), Some(2));
    }

    /// A terminal rebuild swaps its NSView under the same tab: the tab keeps one entry, on the new pointer.
    #[test]
    fn replacing_a_view_moves_the_tab_to_the_new_pointer() {
        let mut r = Registry::default();
        r.register("terminal", "t", 0x10, false);
        r.register("terminal", "t", 0x30, false);
        assert_eq!(ptr(&r, "terminal", "t"), Some(0x30));
        assert_eq!(r.interactive().len(), 0);
        assert_eq!(r.views.len(), 1);
    }

    /// A destroyed view's pointer may be handed to the next view: the stale entry that still holds it must go, or
    /// the router would send input to whatever lives at that address now.
    #[test]
    fn a_reused_pointer_evicts_the_stale_entry_that_held_it() {
        let mut r = Registry::default();
        r.register("browser", "old", 0x40, true);
        r.register("browser", "new", 0x40, true);
        assert_eq!(ptr(&r, "browser", "old"), None);
        assert_eq!(ptr(&r, "browser", "new"), Some(0x40));
    }

    #[test]
    fn only_interactive_views_are_offered_to_the_router() {
        let mut r = Registry::default();
        r.register("terminal", "t", 1, false);
        r.register("browser", "b", 2, true);
        let v = r.interactive();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].ptr, 2);
    }

    #[test]
    fn the_hit_map_drops_unusable_rects_and_reports_whether_it_changed() {
        let mut r = Registry::default();
        let map = HitMap {
            rects: vec![
                Rect::new(10.0, 10.0, 50.0, 40.0),
                Rect::new(10.0, 10.0, 0.0, 30.0),     // empty
                Rect::new(f64::NAN, 0.0, 10.0, 10.0), // measured mid-layout
                Rect::new(0.0, 0.0, f64::INFINITY, 5.0),
            ],
        };
        assert!(r.set_hit_map(map.clone()));
        assert_eq!(r.overlays(), &[Rect::new(10.0, 10.0, 50.0, 40.0)]);
        assert!(!r.set_hit_map(map), "the same map again is no change");
        assert!(r.set_hit_map(HitMap::default()));
        assert!(r.overlays().is_empty());
    }

    /// Contract with the page (`lib/native/hit.ts`): the very bytes its test asserts `invoke("natives_hit", …)` is
    /// given. Tauri reads the arguments as an object keyed by the command's parameter names, so `natives_hit(hit)`
    /// is `{"hit": {"rects": [...]}}`: a renamed parameter or field on either side turns one of the two tests red.
    #[test]
    fn the_hit_map_the_page_sends_deserializes_to_the_rects_it_means() {
        #[derive(Deserialize)]
        struct Args {
            hit: HitMap,
        }
        let wire = include_str!("../../../../apps/desktop/src/lib/native/hit.contract.json");
        let map = serde_json::from_str::<Args>(wire).unwrap().hit;
        assert_eq!(
            map.rects,
            vec![
                Rect::new(300.0, 198.0, 300.0, 200.0),
                Rect::new(0.0, 0.0, 1280.0, 820.0)
            ]
        );
        let none: HitMap = serde_json::from_str("{}").unwrap();
        assert!(none.rects.is_empty());
    }
}
