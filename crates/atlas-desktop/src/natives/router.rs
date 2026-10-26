//! Who owns a mouse event over the window: the page or a native view. Pure (no AppKit): `mac` feeds it the events
//! and the frames and carries out the decision.
//!
//! Native views sit BELOW the page (the page is transparent over their slot), so AppKit's hit test always lands on
//! the page. The router turns that around: over an interactive native view's frame, and outside every rect the page
//! says it paints over it (`natives_hit`: menus, dialogs, a modal's scrim), the event belongs to the native view.
//!
//!  - A press picks the owner and **pins** it: the drag and the release go to the same owner wherever the pointer is
//!    (a text selection dragged across a dialog stays the page's; a drag begun in a page crossing a dialog stays
//!    the native view's), until the button is up.
//!  - A scroll goes to whoever owns the point, never pinned (a wheel over a menu scrolls the menu).
//!  - A move is never consumed: the page always sees it (hover panels and tooltips must see the pointer leave onto a
//!    native view); the native view under it is told too.

use super::registry::Rect;

/// What an event is, as far as ownership goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Down,
    Drag,
    Up,
    Move,
    Scroll,
}

/// A native view the router may hand input to: its pointer and its frame in window points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub view: usize,
    pub frame: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Page,
    Native(usize),
}

/// What to do with one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub owner: Owner,
    /// Take the event away from AppKit's own dispatch (the page below must not also get it). Moves are never
    /// consumed.
    pub consume: bool,
}

#[derive(Debug, Default)]
pub struct Router {
    pin: Option<Owner>,
}

/// The owner of the point (`x`, `y`) with nothing pinned: the topmost (last) target whose frame holds it, unless an
/// overlay rect of the page covers the point.
pub fn owner_at(targets: &[Target], overlays: &[Rect], x: f64, y: f64) -> Owner {
    if overlays.iter().any(|o| o.contains(x, y)) {
        return Owner::Page;
    }
    targets
        .iter()
        .rev()
        .find(|t| t.frame.contains(x, y))
        .map_or(Owner::Page, |t| Owner::Native(t.view))
}

impl Router {
    pub fn decide(
        &mut self,
        kind: Kind,
        targets: &[Target],
        overlays: &[Rect],
        x: f64,
        y: f64,
    ) -> Decision {
        let at = || owner_at(targets, overlays, x, y);
        let owner = match kind {
            Kind::Down => {
                let o = at();
                self.pin = Some(o);
                o
            }
            Kind::Drag => self.pin.unwrap_or_else(at),
            Kind::Up => self.pin.take().unwrap_or_else(at),
            Kind::Move | Kind::Scroll => at(),
        };
        // A pinned native view that was destroyed mid-drag is not a target any more: the page takes the rest.
        let owner = match owner {
            Owner::Native(v) if !targets.iter().any(|t| t.view == v) => Owner::Page,
            o => o,
        };
        Decision {
            owner,
            consume: kind != Kind::Move && matches!(owner, Owner::Native(_)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROWSER: usize = 7;
    const PAGE_AT: (f64, f64) = (50.0, 20.0); // chrome above the slot
    const SLOT: Rect = Rect::new(100.0, 100.0, 600.0, 400.0);
    const MENU: Rect = Rect::new(300.0, 150.0, 120.0, 90.0);

    fn targets() -> Vec<Target> {
        vec![Target {
            view: BROWSER,
            frame: SLOT,
        }]
    }

    fn native() -> Owner {
        Owner::Native(BROWSER)
    }

    #[test]
    fn a_free_slot_belongs_to_the_native_view_and_chrome_outside_it_to_the_page() {
        let mut r = Router::default();
        let d = r.decide(Kind::Down, &targets(), &[], 200.0, 200.0);
        assert_eq!(
            d,
            Decision {
                owner: native(),
                consume: true
            }
        );
        r.decide(Kind::Up, &targets(), &[], 200.0, 200.0);
        let d = r.decide(Kind::Down, &targets(), &[], PAGE_AT.0, PAGE_AT.1);
        assert_eq!(
            d,
            Decision {
                owner: Owner::Page,
                consume: false
            }
        );
    }

    /// The point of the hit map: the page paints a menu over the slot, so a click on the menu is the page's, and
    /// one in the slot right beside it (its shadow) is the native view's.
    #[test]
    fn a_click_on_an_overlay_is_the_pages_and_one_beside_it_is_the_native_views() {
        let mut r = Router::default();
        let on = r.decide(Kind::Down, &targets(), &[MENU], 310.0, 160.0);
        assert_eq!(on.owner, Owner::Page);
        assert!(
            !on.consume,
            "the page gets it through AppKit's own dispatch"
        );
        r.decide(Kind::Up, &targets(), &[MENU], 310.0, 160.0);
        let beside = r.decide(Kind::Down, &targets(), &[MENU], 299.0, 160.0);
        assert_eq!(beside.owner, native());
    }

    #[test]
    fn an_overlay_closed_means_the_slot_is_the_native_views_again_where_it_was() {
        let mut r = Router::default();
        assert_eq!(
            r.decide(Kind::Scroll, &targets(), &[MENU], 310.0, 160.0)
                .owner,
            Owner::Page
        );
        assert_eq!(
            r.decide(Kind::Scroll, &targets(), &[], 310.0, 160.0).owner,
            native()
        );
    }

    /// A modal publishes its scrim as one whole-window rect: the browser behind it gets nothing at all.
    #[test]
    fn a_whole_window_overlay_takes_every_point_of_every_slot() {
        let mut r = Router::default();
        let modal = Rect::new(0.0, 0.0, 1280.0, 820.0);
        for (x, y) in [(110.0, 110.0), (690.0, 490.0), (400.0, 300.0)] {
            assert_eq!(
                r.decide(Kind::Down, &targets(), &[modal], x, y).owner,
                Owner::Page
            );
            r.decide(Kind::Up, &targets(), &[modal], x, y);
        }
    }

    /// A drag begun in the native view that crosses an overlay, and is released over it, stays the native view's.
    #[test]
    fn a_drag_started_in_the_native_view_stays_its_own_across_an_overlay_and_to_the_release() {
        let mut r = Router::default();
        assert_eq!(
            r.decide(Kind::Down, &targets(), &[MENU], 200.0, 200.0)
                .owner,
            native()
        );
        for (x, y) in [(250.0, 180.0), (320.0, 170.0), (900.0, 700.0)] {
            let d = r.decide(Kind::Drag, &targets(), &[MENU], x, y);
            assert_eq!(
                d,
                Decision {
                    owner: native(),
                    consume: true
                }
            );
        }
        let up = r.decide(Kind::Up, &targets(), &[MENU], 320.0, 170.0);
        assert_eq!(
            up,
            Decision {
                owner: native(),
                consume: true
            }
        );
        // pin gone: the next press is decided by where it lands
        assert_eq!(
            r.decide(Kind::Down, &targets(), &[MENU], 320.0, 170.0)
                .owner,
            Owner::Page
        );
    }

    /// A drag begun on the overlay that leaves into the slot stays the page's (a slider, a text selection in a dialog).
    #[test]
    fn a_drag_started_on_an_overlay_stays_the_pages_when_it_leaves_into_the_slot() {
        let mut r = Router::default();
        assert_eq!(
            r.decide(Kind::Down, &targets(), &[MENU], 310.0, 160.0)
                .owner,
            Owner::Page
        );
        let d = r.decide(Kind::Drag, &targets(), &[MENU], 200.0, 400.0);
        assert_eq!(
            d,
            Decision {
                owner: Owner::Page,
                consume: false
            }
        );
        let up = r.decide(Kind::Up, &targets(), &[MENU], 200.0, 400.0);
        assert_eq!(up.owner, Owner::Page);
    }

    /// A drag begun on page chrome (the sidebar, a tab strip) that enters the slot is the page's too.
    #[test]
    fn a_drag_started_on_chrome_stays_the_pages_over_a_slot() {
        let mut r = Router::default();
        r.decide(Kind::Down, &targets(), &[], PAGE_AT.0, PAGE_AT.1);
        assert_eq!(
            r.decide(Kind::Drag, &targets(), &[], 300.0, 300.0).owner,
            Owner::Page
        );
        r.decide(Kind::Up, &targets(), &[], 300.0, 300.0);
    }

    /// A move is never consumed (the page must see the pointer leave, to close hover panels and tooltips), and the
    /// native view under it is told as well.
    #[test]
    fn a_move_over_the_native_view_is_forwarded_to_it_but_never_taken_from_the_page() {
        let mut r = Router::default();
        let d = r.decide(Kind::Move, &targets(), &[], 200.0, 200.0);
        assert_eq!(
            d,
            Decision {
                owner: native(),
                consume: false
            }
        );
        let d = r.decide(Kind::Move, &targets(), &[MENU], 310.0, 160.0);
        assert_eq!(
            d,
            Decision {
                owner: Owner::Page,
                consume: false
            }
        );
    }

    /// A wheel over a menu scrolls the menu even though the pointer is over the slot; it is never pinned by an
    /// earlier press.
    #[test]
    fn a_scroll_follows_the_point_not_the_last_press() {
        let mut r = Router::default();
        r.decide(Kind::Down, &targets(), &[MENU], 200.0, 200.0);
        r.decide(Kind::Up, &targets(), &[MENU], 200.0, 200.0);
        assert_eq!(
            r.decide(Kind::Scroll, &targets(), &[MENU], 310.0, 160.0)
                .owner,
            Owner::Page
        );
        let s = r.decide(Kind::Scroll, &targets(), &[MENU], 200.0, 200.0);
        assert_eq!(
            s,
            Decision {
                owner: native(),
                consume: true
            }
        );
    }

    /// A view parked off screen has its frame off screen: no point of the window is inside it.
    #[test]
    fn a_parked_view_owns_nothing() {
        let mut r = Router::default();
        let parked = [Target {
            view: BROWSER,
            frame: Rect::new(-20000.0, -20000.0, 600.0, 400.0),
        }];
        assert_eq!(
            r.decide(Kind::Down, &parked, &[], 200.0, 200.0).owner,
            Owner::Page
        );
    }

    /// The pinned view was destroyed mid-drag (⌘W on the tab): its pointer is not a target any more, and the rest
    /// of the drag must not be sent to a dead `NSView`.
    #[test]
    fn a_pinned_view_that_goes_away_mid_drag_hands_the_rest_to_the_page() {
        let mut r = Router::default();
        assert_eq!(
            r.decide(Kind::Down, &targets(), &[], 200.0, 200.0).owner,
            native()
        );
        let d = r.decide(Kind::Drag, &[], &[], 220.0, 220.0);
        assert_eq!(
            d,
            Decision {
                owner: Owner::Page,
                consume: false
            }
        );
        assert_eq!(
            r.decide(Kind::Up, &[], &[], 220.0, 220.0).owner,
            Owner::Page
        );
    }

    #[test]
    fn the_topmost_of_two_overlapping_views_wins() {
        let a = Target {
            view: 1,
            frame: Rect::new(0.0, 0.0, 100.0, 100.0),
        };
        let b = Target {
            view: 2,
            frame: Rect::new(50.0, 50.0, 100.0, 100.0),
        };
        assert_eq!(owner_at(&[a, b], &[], 60.0, 60.0), Owner::Native(2));
        assert_eq!(owner_at(&[a, b], &[], 10.0, 10.0), Owner::Native(1));
    }

    /// Edges: a frame is half-open, so two panes side by side never both claim the seam.
    #[test]
    fn a_frame_is_half_open_so_neighbours_do_not_share_a_seam() {
        let left = Target {
            view: 1,
            frame: Rect::new(0.0, 0.0, 100.0, 100.0),
        };
        let right = Target {
            view: 2,
            frame: Rect::new(100.0, 0.0, 100.0, 100.0),
        };
        assert_eq!(owner_at(&[left, right], &[], 99.9, 50.0), Owner::Native(1));
        assert_eq!(owner_at(&[left, right], &[], 100.0, 50.0), Owner::Native(2));
        assert_eq!(owner_at(&[left, right], &[], 200.0, 50.0), Owner::Page);
    }
}
