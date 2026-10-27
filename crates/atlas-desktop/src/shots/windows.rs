//! Which windows the capture overlay offers in window mode, and which ones every capture leaves out. Pure: the
//! macOS side turns `SCShareableContent` into [`Win`]s and asks these rules.

use super::geom::Rect;

/// A window as ScreenCaptureKit reports it, reduced to what the rules read.
#[derive(Debug, Clone, PartialEq)]
pub struct Win {
    pub id: u32,
    pub frame: Rect,
    pub title: String,
    pub pid: i32,
    /// `NSWindow.level` of the window: 0 for ordinary windows; the menu bar, the Dock, the desktop and overlays use
    /// other layers.
    pub layer: i64,
    pub on_screen: bool,
    pub app: String,
}

/// Smallest window worth offering: a 1-pixel tooltip shadow or an invisible helper window is not a target.
pub const MIN_SIDE: f64 = 40.0;

/// The windows Window mode may pick: ordinary (layer 0), on screen, big enough, not one of Atlas's own capture
/// surfaces (`own_pid` and `own_ids`: the overlay, the quick card, the HUD, the dock pill are in no screenshot), and
/// intersecting `display`. Order is kept (front to back).
pub fn pickable(wins: &[Win], own_pid: i32, own_ids: &[u32], display: Rect) -> Vec<Win> {
    wins.iter()
        .filter(|w| {
            w.layer == 0
                && w.on_screen
                && w.frame.w >= MIN_SIDE
                && w.frame.h >= MIN_SIDE
                && !own_ids.contains(&w.id)
                && !(w.pid == own_pid && is_capture_surface(w))
                && w.frame.intersect(&display).is_some()
        })
        .cloned()
        .collect()
}

/// Atlas's windows a screenshot must never contain are identified by id by the caller; this additionally drops any
/// untitled helper window of our own process (the transparent overlay panels have no title).
fn is_capture_surface(w: &Win) -> bool {
    w.title.is_empty()
}

/// The ids of every window of this process that a screen capture must exclude: all untitled helper windows and the
/// ids the caller knows (overlay, quick, hud, dock). The main window stays capturable (it is a normal window the user
/// may want to screenshot) unless `exclude_main` is set.
pub fn exclusions(
    wins: &[Win],
    own_pid: i32,
    own_ids: &[u32],
    main_id: Option<u32>,
    exclude_main: bool,
) -> Vec<u32> {
    let mut out: Vec<u32> = wins
        .iter()
        .filter(|w| w.pid == own_pid && (own_ids.contains(&w.id) || is_capture_surface(w)))
        .map(|w| w.id)
        .collect();
    if exclude_main && let Some(m) = main_id {
        out.push(m);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `kCGDesktopIconWindowLevel`: the layer Finder draws the desktop icons on (a full-display window per screen).
pub const DESKTOP_ICON_LAYER: i64 = -2_147_483_603;

/// Ids of the windows that ARE the desktop icons: any window on the desktop-icon layer. A capture that excludes
/// them shows the wallpaper bare, without Atlas touching Finder's `CreateDesktop` setting or restarting Finder.
pub fn desktop_icon_ids(wins: &[Win]) -> Vec<u32> {
    wins.iter()
        .filter(|w| w.layer == DESKTOP_ICON_LAYER)
        .map(|w| w.id)
        .collect()
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    fn win(id: u32, layer: i64) -> Win {
        Win {
            id,
            frame: Rect::new(0.0, 0.0, 100.0, 100.0),
            title: String::new(),
            pid: 1,
            layer,
            on_screen: true,
            app: "Finder".into(),
        }
    }

    #[test]
    fn only_the_desktop_icon_layer_is_hidden_and_ordinary_windows_stay() {
        let wins = [
            win(1, 0),
            win(2, DESKTOP_ICON_LAYER),
            win(3, -2_147_483_624),
            win(4, DESKTOP_ICON_LAYER),
        ];
        // -2147483624 is the wallpaper layer: the wallpaper must stay
        assert_eq!(desktop_icon_ids(&wins), [2, 4]);
        assert!(desktop_icon_ids(&[]).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Rect = Rect::new(0.0, 0.0, 1512.0, 982.0);

    fn w(id: u32, pid: i32, title: &str, layer: i64, frame: Rect) -> Win {
        Win {
            id,
            frame,
            title: title.into(),
            pid,
            layer,
            on_screen: true,
            app: "App".into(),
        }
    }

    const BIG: Rect = Rect::new(100.0, 100.0, 800.0, 600.0);

    #[test]
    fn only_ordinary_visible_big_windows_on_the_display_are_pickable() {
        let wins = [
            w(1, 10, "Safari", 0, BIG),
            w(2, 11, "Menu Bar", 24, Rect::new(0.0, 0.0, 1512.0, 30.0)),
            w(3, 12, "Dock", 20, BIG),
            w(4, 13, "tiny", 0, Rect::new(10.0, 10.0, 20.0, 20.0)),
            w(5, 14, "elsewhere", 0, Rect::new(3000.0, 0.0, 800.0, 600.0)),
            Win {
                on_screen: false,
                ..w(6, 15, "hidden", 0, BIG)
            },
        ];
        let got: Vec<u32> = pickable(&wins, 999, &[], D).iter().map(|w| w.id).collect();
        assert_eq!(got, [1]);
    }

    #[test]
    fn order_is_kept_front_to_back() {
        let wins = [
            w(9, 1, "a", 0, BIG),
            w(3, 2, "b", 0, BIG),
            w(7, 3, "c", 0, BIG),
        ];
        let got: Vec<u32> = pickable(&wins, 999, &[], D).iter().map(|w| w.id).collect();
        assert_eq!(got, [9, 3, 7]);
    }

    /// The incident this prevents: the overlay highlighting (and the screenshot containing) Atlas's own capture
    /// panels. The known ids and every untitled helper window of our process are out; a titled Atlas window (the
    /// main window) is a normal target.
    #[test]
    fn atlas_capture_surfaces_are_never_pickable_but_its_main_window_is() {
        let me = 500;
        let wins = [
            w(1, me, "", 0, BIG),
            w(2, me, "Atlas Dock", 0, BIG),
            w(3, me, "Atlas", 0, BIG),
            w(4, 77, "", 0, BIG),
        ];
        let got: Vec<u32> = pickable(&wins, me, &[2], D).iter().map(|w| w.id).collect();
        assert_eq!(got, [3, 4], "another app's untitled window stays pickable");
    }

    #[test]
    fn exclusions_cover_known_ids_and_untitled_helpers_of_our_process_only() {
        let me = 500;
        let wins = [
            w(1, me, "", 0, BIG),
            w(2, me, "Atlas Dock", 0, BIG),
            w(3, me, "Atlas", 0, BIG),
            w(4, 77, "", 0, BIG),
        ];
        assert_eq!(exclusions(&wins, me, &[2], Some(3), false), [1, 2]);
        assert_eq!(exclusions(&wins, me, &[2], Some(3), true), [1, 2, 3]);
        assert_eq!(exclusions(&wins, me, &[], None, true), [1]);
        assert!(
            exclusions(&[], me, &[2], Some(3), true).len() == 1,
            "main id is excluded even if unlisted"
        );
    }
}
