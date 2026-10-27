//! The capture session: what a hotkey press starts and how it ends, as a pure state machine. The macOS layer feeds it
//! events (hotkey, overlay commit/cancel, capture finished) and runs the effects it returns.
//!
//! One session at a time: pressing a capture hotkey while the overlay is up switches its mode instead of stacking a
//! second overlay (CleanShot behaviour), and while a recording runs only stop/pause/cancel apply.

use super::geom::Rect;
use super::prefs::Hotkey;

/// What the overlay is picking. `All` is the CleanShot-style HUD: the page switches sub-mode itself and the commit
/// says what to do with it ([`Action`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pick {
    Region,
    Window,
    Record,
    Gif,
    /// Drag a region and read its text.
    Text,
    All,
}

/// What a committed selection becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Capture,
    Record,
    Gif,
    Text,
}

impl Action {
    /// What a plain (non-HUD) overlay of `pick` does with a selection.
    pub fn of_pick(pick: Pick) -> Action {
        match pick {
            Pick::Record => Action::Record,
            Pick::Gif => Action::Gif,
            Pick::Text => Action::Text,
            Pick::Region | Pick::Window | Pick::All => Action::Capture,
        }
    }
}

/// Where a recording is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Countdown,
    Recording,
    Paused,
    Saving,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    /// The crosshair overlay is up on every display.
    Picking(Pick),
    /// A still is being taken (the overlay is already gone).
    Capturing,
    /// A recording of `rect` on `display` (`None` = the whole display); `gif`: convert when it stops.
    Recording {
        phase: Phase,
        display: u32,
        rect: Option<Rect>,
        gif: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A capture hotkey (or tray/CLI start).
    Hotkey(Hotkey),
    Commit(Commit),
    /// "Capture previous area": the layer above found the stored region; no overlay.
    Previous {
        rect: Rect,
        display: u32,
    },
    /// Esc or right click in the overlay.
    Cancel,
    /// The still is saved (or failed).
    CaptureDone,
    /// The countdown ended.
    CountdownDone,
    Pause,
    Resume,
    Stop,
    /// The file is final.
    Saved,
    /// A recording could not start or died.
    Failed,
}

/// What the overlay committed. `action` is what the HUD (or the plain mode) wants done with it.
#[derive(Debug, Clone, PartialEq)]
pub enum Commit {
    Region {
        rect: Rect,
        display: u32,
        action: Action,
    },
    Window {
        id: u32,
        action: Action,
    },
    /// A whole display (the HUD's screen button).
    Screen {
        display: u32,
        action: Action,
    },
}

/// The region a commit selected, for "capture previous area". Windows and screens have none.
pub fn region_of(c: &Commit) -> Option<(Rect, u32)> {
    match c {
        Commit::Region { rect, display, .. } => Some((*rect, *display)),
        Commit::Window { .. } | Commit::Screen { .. } => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Show the overlay on every display in this mode.
    ShowOverlay(Pick),
    /// Switch the visible overlay's mode without rebuilding it.
    SetOverlayMode(Pick),
    HideOverlay,
    /// Take the still now (the overlay is already hidden, which is why a frame of it never appears).
    Still(Still),
    /// Take the still like [`Effect::Still`], then read its text instead of saving a screenshot.
    Text(Still),
    StartCountdown,
    StartRecording {
        display: u32,
        rect: Option<Rect>,
        gif: bool,
    },
    PauseRecording,
    ResumeRecording,
    StopRecording,
    DiscardRecording,
    /// Nothing more to do: hide the HUD.
    HideHud,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Still {
    /// The display under the pointer, whole.
    Screen,
    /// A given display, whole.
    Display {
        display: u32,
    },
    Region {
        rect: Rect,
        display: u32,
    },
    Window {
        id: u32,
    },
}

fn pick_of(h: Hotkey) -> Option<Pick> {
    match h {
        Hotkey::AllInOne => Some(Pick::All),
        Hotkey::Region => Some(Pick::Region),
        Hotkey::Window => Some(Pick::Window),
        Hotkey::Ocr => Some(Pick::Text),
        Hotkey::Record => Some(Pick::Record),
        Hotkey::Gif => Some(Pick::Gif),
        // no overlay: handled by the layer above (previous area) or taken at once (fullscreen)
        Hotkey::Fullscreen | Hotkey::Previous => None,
    }
}

/// The effect that turns a selection into `action`'s first step: a still, a text read, or a recording countdown.
fn resolve(still: Still, action: Action) -> Option<(State, Vec<Effect>)> {
    let rec = |display, rect, gif| {
        (
            State::Recording {
                phase: Phase::Countdown,
                display,
                rect,
                gif,
            },
            vec![Effect::HideOverlay, Effect::StartCountdown],
        )
    };
    Some(match (action, still) {
        (Action::Capture, s) => (
            State::Capturing,
            vec![Effect::HideOverlay, Effect::Still(s)],
        ),
        (Action::Text, s) => (State::Capturing, vec![Effect::HideOverlay, Effect::Text(s)]),
        (Action::Record | Action::Gif, Still::Region { rect, display }) => {
            rec(display, Some(rect), action == Action::Gif)
        }
        (Action::Record | Action::Gif, Still::Display { display }) => {
            rec(display, None, action == Action::Gif)
        }
        // a window is recorded or read as its frame, a region: the layer above resolves it
        (Action::Record | Action::Gif, Still::Window { .. } | Still::Screen) => return None,
    })
}

/// `(next state, effects)` for `event` in `state`.
pub fn step(state: &State, event: Event) -> (State, Vec<Effect>) {
    use Effect as E;
    use State as S;
    match (state, event) {
        // ── a recording is running: only its own controls apply, every capture hotkey is ignored
        (
            S::Recording {
                phase,
                display,
                rect,
                gif,
            },
            ev,
        ) => {
            let keep = |p: Phase| S::Recording {
                phase: p,
                display: *display,
                rect: *rect,
                gif: *gif,
            };
            match (phase, ev) {
                (Phase::Countdown, Event::CountdownDone) => (
                    keep(Phase::Recording),
                    vec![E::StartRecording {
                        display: *display,
                        rect: *rect,
                        gif: *gif,
                    }],
                ),
                (Phase::Countdown, Event::Cancel | Event::Stop) => (S::Idle, vec![E::HideHud]),
                (Phase::Recording, Event::Pause) => (keep(Phase::Paused), vec![E::PauseRecording]),
                (Phase::Paused, Event::Resume) => {
                    (keep(Phase::Recording), vec![E::ResumeRecording])
                }
                (Phase::Recording | Phase::Paused, Event::Stop) => {
                    (keep(Phase::Saving), vec![E::StopRecording])
                }
                (Phase::Recording | Phase::Paused, Event::Cancel) => {
                    (S::Idle, vec![E::DiscardRecording, E::HideHud])
                }
                (Phase::Saving, Event::Saved) => (S::Idle, vec![]),
                (_, Event::Failed) => (S::Idle, vec![E::HideHud]),
                // saving cannot be cancelled: the file is being finalised
                _ => (state.clone(), vec![]),
            }
        }
        // ── the overlay is up
        (S::Picking(p), Event::Hotkey(h)) => match pick_of(h) {
            // the same hotkey again closes it (toggle), another one switches the mode
            Some(next) if next == *p => (S::Idle, vec![E::HideOverlay]),
            Some(next) => (S::Picking(next), vec![E::SetOverlayMode(next)]),
            None if h == Hotkey::Fullscreen => {
                (S::Capturing, vec![E::HideOverlay, E::Still(Still::Screen)])
            }
            // `Previous` is resolved to `Event::Previous` before the machine sees it
            None => (state.clone(), vec![]),
        },
        (S::Picking(_), Event::Previous { rect, display }) => (
            S::Capturing,
            vec![E::HideOverlay, E::Still(Still::Region { rect, display })],
        ),
        (S::Picking(_), Event::Cancel) => (S::Idle, vec![E::HideOverlay]),
        (S::Picking(p), Event::Commit(c)) => {
            // a plain overlay always does its own thing; only the HUD takes the action from the page
            let pick_action = Action::of_pick(*p);
            let (still, action) = match c {
                Commit::Region {
                    rect,
                    display,
                    action,
                } => (Still::Region { rect, display }, action),
                Commit::Window { id, action } => (Still::Window { id }, action),
                Commit::Screen { display, action } => (Still::Display { display }, action),
            };
            let action = if *p == Pick::All { action } else { pick_action };
            // a window commit in a region/window overlay is a screenshot of it (Space switched the page's mode)
            let action = match (&still, action) {
                (Still::Window { .. }, Action::Record | Action::Gif | Action::Text) => {
                    return (state.clone(), vec![]);
                }
                _ => action,
            };
            resolve(still, action).unwrap_or_else(|| (state.clone(), vec![]))
        }
        // ── idle (and capturing: a hotkey while the pixels are being cut waits for that, a few milliseconds, because
        // a new session would replace the frozen frames the running capture is about to read; `CaptureDone` ends it
        // as soon as the pixels are in hand, before the encode and the write)
        (S::Idle, Event::Hotkey(h)) => match pick_of(h) {
            Some(p) => (S::Picking(p), vec![E::ShowOverlay(p)]),
            None if h == Hotkey::Fullscreen => (S::Capturing, vec![E::Still(Still::Screen)]),
            None => (state.clone(), vec![]),
        },
        (S::Idle, Event::Previous { rect, display }) => (
            S::Capturing,
            vec![E::Still(Still::Region { rect, display })],
        ),
        (S::Capturing, Event::CaptureDone) => (S::Idle, vec![]),
        (S::Capturing, Event::Failed) => (S::Idle, vec![]),
        // anything else is stale (a late commit after a cancel, a hotkey while saving a still)
        _ => (state.clone(), vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect::new(10.0, 20.0, 300.0, 200.0);

    fn run(events: Vec<Event>) -> (State, Vec<Vec<Effect>>) {
        let mut s = State::Idle;
        let mut all = Vec::new();
        for e in events {
            let (n, fx) = step(&s, e);
            s = n;
            all.push(fx);
        }
        (s, all)
    }

    #[test]
    fn a_region_hotkey_shows_the_overlay_and_a_drag_takes_one_still_and_returns_to_idle() {
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Region),
            Event::Commit(Commit::Region {
                rect: R,
                display: 1,
                action: Action::Capture,
            }),
            Event::CaptureDone,
        ]);
        assert_eq!(s, State::Idle);
        assert_eq!(fx[0], [Effect::ShowOverlay(Pick::Region)]);
        // the overlay is hidden BEFORE the still: a frame of the dimmed crosshair is never in the screenshot
        assert_eq!(
            fx[1],
            [
                Effect::HideOverlay,
                Effect::Still(Still::Region {
                    rect: R,
                    display: 1
                })
            ]
        );
        assert!(fx[2].is_empty());
    }

    #[test]
    fn the_fullscreen_hotkey_needs_no_overlay() {
        let (s, fx) = run(vec![Event::Hotkey(Hotkey::Fullscreen)]);
        assert_eq!(s, State::Capturing);
        assert_eq!(fx[0], [Effect::Still(Still::Screen)]);
    }

    #[test]
    fn esc_in_the_overlay_cancels_without_a_capture() {
        let (s, fx) = run(vec![Event::Hotkey(Hotkey::Region), Event::Cancel]);
        assert_eq!(s, State::Idle);
        assert_eq!(fx[1], [Effect::HideOverlay]);
    }

    /// Pressing the same hotkey again closes the overlay (a toggle); another capture hotkey switches the mode
    /// instead of stacking a second overlay.
    #[test]
    fn hotkeys_inside_the_overlay_toggle_or_switch_never_stack() {
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Region),
            Event::Hotkey(Hotkey::Region),
        ]);
        assert_eq!((s, &fx[1][..]), (State::Idle, &[Effect::HideOverlay][..]));
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Region),
            Event::Hotkey(Hotkey::Window),
        ]);
        assert_eq!(s, State::Picking(Pick::Window));
        assert_eq!(fx[1], [Effect::SetOverlayMode(Pick::Window)]);
        // the fullscreen key inside the overlay captures the screen right away
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Region),
            Event::Hotkey(Hotkey::Fullscreen),
        ]);
        assert_eq!(s, State::Capturing);
        assert_eq!(fx[1], [Effect::HideOverlay, Effect::Still(Still::Screen)]);
    }

    #[test]
    fn a_window_pick_captures_that_window() {
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Window),
            Event::Commit(Commit::Window {
                id: 42,
                action: Action::Capture,
            }),
        ]);
        assert_eq!(s, State::Capturing);
        assert_eq!(
            fx[1],
            [Effect::HideOverlay, Effect::Still(Still::Window { id: 42 })]
        );
        // Space in the region overlay switches it to window mode in the page; the commit is still a window one
        let (_, fx) = run(vec![
            Event::Hotkey(Hotkey::Region),
            Event::Commit(Commit::Window {
                id: 7,
                action: Action::Capture,
            }),
        ]);
        assert_eq!(
            fx[1],
            [Effect::HideOverlay, Effect::Still(Still::Window { id: 7 })]
        );
    }

    #[test]
    fn a_recording_runs_countdown_then_records_then_saves() {
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Record),
            Event::Commit(Commit::Region {
                rect: R,
                display: 2,
                action: Action::Capture,
            }),
            Event::CountdownDone,
            Event::Stop,
            Event::Saved,
        ]);
        assert_eq!(s, State::Idle);
        assert_eq!(fx[1], [Effect::HideOverlay, Effect::StartCountdown]);
        assert_eq!(
            fx[2],
            [Effect::StartRecording {
                display: 2,
                rect: Some(R),
                gif: false
            }]
        );
        assert_eq!(fx[3], [Effect::StopRecording]);
    }

    #[test]
    fn the_gif_hotkey_records_with_the_gif_flag() {
        let (_, fx) = run(vec![
            Event::Hotkey(Hotkey::Gif),
            Event::Commit(Commit::Region {
                rect: R,
                display: 1,
                action: Action::Capture,
            }),
            Event::CountdownDone,
        ]);
        assert_eq!(
            fx[2],
            [Effect::StartRecording {
                display: 1,
                rect: Some(R),
                gif: true
            }]
        );
    }

    #[test]
    fn pause_and_resume_only_apply_while_recording_and_paused() {
        let rec = State::Recording {
            phase: Phase::Recording,
            display: 1,
            rect: Some(R),
            gif: false,
        };
        let (paused, fx) = step(&rec, Event::Pause);
        assert!(matches!(
            paused,
            State::Recording {
                phase: Phase::Paused,
                ..
            }
        ));
        assert_eq!(fx, [Effect::PauseRecording]);
        // pausing a paused recording, resuming a running one: nothing
        assert_eq!(step(&paused, Event::Pause), (paused.clone(), vec![]));
        assert_eq!(step(&rec, Event::Resume), (rec.clone(), vec![]));
        let (back, fx) = step(&paused, Event::Resume);
        assert_eq!(back, rec);
        assert_eq!(fx, [Effect::ResumeRecording]);
        // a paused recording can be stopped (and saved)
        let (s, fx) = step(&paused, Event::Stop);
        assert!(matches!(
            s,
            State::Recording {
                phase: Phase::Saving,
                ..
            }
        ));
        assert_eq!(fx, [Effect::StopRecording]);
    }

    /// While recording, a capture hotkey must not open an overlay over the recorded region (it would be in the
    /// video) nor start a second capture.
    #[test]
    fn capture_hotkeys_are_ignored_while_recording_or_saving() {
        for phase in [
            Phase::Countdown,
            Phase::Recording,
            Phase::Paused,
            Phase::Saving,
        ] {
            let s = State::Recording {
                phase,
                display: 1,
                rect: None,
                gif: false,
            };
            for h in Hotkey::ALL {
                assert_eq!(
                    step(&s, Event::Hotkey(h)),
                    (s.clone(), vec![]),
                    "{phase:?} {h:?}"
                );
            }
        }
    }

    #[test]
    fn esc_during_the_countdown_or_recording_discards_and_never_saves() {
        let cd = State::Recording {
            phase: Phase::Countdown,
            display: 1,
            rect: None,
            gif: false,
        };
        let (s, fx) = step(&cd, Event::Cancel);
        assert_eq!((s, fx), (State::Idle, vec![Effect::HideHud]));
        let rec = State::Recording {
            phase: Phase::Recording,
            display: 1,
            rect: None,
            gif: false,
        };
        let (s, fx) = step(&rec, Event::Cancel);
        assert_eq!(s, State::Idle);
        assert_eq!(fx, [Effect::DiscardRecording, Effect::HideHud]);
        // saving cannot be cancelled: the file is being finalised
        let sv = State::Recording {
            phase: Phase::Saving,
            display: 1,
            rect: None,
            gif: false,
        };
        assert_eq!(step(&sv, Event::Cancel), (sv.clone(), vec![]));
    }

    /// A recording that fails to start (permission, disk) must free the session: otherwise every later hotkey is
    /// ignored until Atlas restarts.
    #[test]
    fn a_failed_recording_or_still_returns_to_idle() {
        for phase in [
            Phase::Countdown,
            Phase::Recording,
            Phase::Paused,
            Phase::Saving,
        ] {
            let s = State::Recording {
                phase,
                display: 1,
                rect: None,
                gif: false,
            };
            assert_eq!(step(&s, Event::Failed).0, State::Idle, "{phase:?}");
        }
        assert_eq!(step(&State::Capturing, Event::Failed).0, State::Idle);
    }

    /// Late events (a commit after Esc, a second CaptureDone) are no-ops, not panics or phantom captures.
    #[test]
    fn stale_events_do_nothing() {
        for e in [
            Event::Commit(Commit::Region {
                rect: R,
                display: 1,
                action: Action::Capture,
            }),
            Event::CaptureDone,
            Event::CountdownDone,
            Event::Stop,
            Event::Saved,
        ] {
            assert_eq!(
                step(&State::Idle, e.clone()),
                (State::Idle, vec![]),
                "{e:?}"
            );
        }
        // a hotkey while the pixels are being cut waits: a new overlay would replace the frames being read
        let (s, fx) = step(&State::Capturing, Event::Hotkey(Hotkey::Region));
        assert_eq!((s, fx), (State::Capturing, vec![]));
    }

    #[test]
    fn recording_a_picked_window_is_resolved_to_a_region_by_the_caller_not_started_blindly() {
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::Record),
            Event::Commit(Commit::Window {
                id: 5,
                action: Action::Capture,
            }),
        ]);
        assert_eq!(
            s,
            State::Picking(Pick::Record),
            "stays in the overlay until a region arrives"
        );
        assert!(fx[1].is_empty());
    }

    // ───────── CleanShot parity: all-in-one, text, previous area ─────────

    fn region(action: Action) -> Event {
        Event::Commit(Commit::Region {
            rect: R,
            display: 1,
            action,
        })
    }

    #[test]
    fn the_all_in_one_key_opens_one_overlay_that_any_commit_resolves() {
        let (s, fx) = run(vec![Event::Hotkey(Hotkey::AllInOne)]);
        assert_eq!(s, State::Picking(Pick::All));
        assert_eq!(fx[0], [Effect::ShowOverlay(Pick::All)]);
        // the HUD's choice travels in the commit
        let region_still = Still::Region {
            rect: R,
            display: 1,
        };
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::AllInOne),
            region(Action::Capture),
        ]);
        assert_eq!(s, State::Capturing);
        assert_eq!(
            fx[1],
            [Effect::HideOverlay, Effect::Still(region_still.clone())]
        );
        let (_, fx) = run(vec![Event::Hotkey(Hotkey::AllInOne), region(Action::Text)]);
        assert_eq!(fx[1], [Effect::HideOverlay, Effect::Text(region_still)]);
        for (action, gif) in [(Action::Record, false), (Action::Gif, true)] {
            let (s, fx) = run(vec![Event::Hotkey(Hotkey::AllInOne), region(action)]);
            assert_eq!(
                s,
                State::Recording {
                    phase: Phase::Countdown,
                    display: 1,
                    rect: Some(R),
                    gif
                }
            );
            assert_eq!(fx[1], [Effect::HideOverlay, Effect::StartCountdown]);
        }
    }

    #[test]
    fn the_hud_screen_button_captures_a_whole_display_and_can_record_it() {
        let screen = |action| Event::Commit(Commit::Screen { display: 2, action });
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::AllInOne),
            screen(Action::Capture),
        ]);
        assert_eq!(s, State::Capturing);
        assert_eq!(
            fx[1],
            [
                Effect::HideOverlay,
                Effect::Still(Still::Display { display: 2 })
            ]
        );
        let (s, _) = run(vec![
            Event::Hotkey(Hotkey::AllInOne),
            screen(Action::Record),
        ]);
        assert!(matches!(
            s,
            State::Recording {
                display: 2,
                rect: None,
                gif: false,
                ..
            }
        ));
    }

    /// A window cannot be recorded or read as text directly: the page turns it into its frame as a region.
    #[test]
    fn a_window_commit_only_captures() {
        for action in [Action::Record, Action::Gif, Action::Text] {
            let (s, fx) = run(vec![
                Event::Hotkey(Hotkey::AllInOne),
                Event::Commit(Commit::Window { id: 9, action }),
            ]);
            assert_eq!(s, State::Picking(Pick::All), "{action:?}");
            assert!(fx[1].is_empty());
        }
        let (_, fx) = run(vec![
            Event::Hotkey(Hotkey::AllInOne),
            Event::Commit(Commit::Window {
                id: 9,
                action: Action::Capture,
            }),
        ]);
        assert_eq!(
            fx[1],
            [Effect::HideOverlay, Effect::Still(Still::Window { id: 9 })]
        );
    }

    /// A plain overlay does its own thing whatever the page sent: a stale `capture` in a record overlay must not
    /// screenshot instead of record.
    #[test]
    fn a_plain_overlay_ignores_the_commits_action() {
        let (s, _) = run(vec![Event::Hotkey(Hotkey::Record), region(Action::Capture)]);
        assert!(matches!(s, State::Recording { gif: false, .. }));
        let (_, fx) = run(vec![Event::Hotkey(Hotkey::Region), region(Action::Gif)]);
        assert!(matches!(fx[1][1], Effect::Still(_)));
    }

    #[test]
    fn the_text_key_picks_a_region_and_reads_it() {
        let (s, fx) = run(vec![Event::Hotkey(Hotkey::Ocr), region(Action::Text)]);
        assert_eq!(s, State::Capturing);
        assert_eq!(fx[0], [Effect::ShowOverlay(Pick::Text)]);
        assert_eq!(
            fx[1],
            [
                Effect::HideOverlay,
                Effect::Text(Still::Region {
                    rect: R,
                    display: 1
                })
            ]
        );
    }

    #[test]
    fn all_in_one_toggles_and_other_keys_switch_it() {
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::AllInOne),
            Event::Hotkey(Hotkey::AllInOne),
        ]);
        assert_eq!((s, &fx[1][..]), (State::Idle, &[Effect::HideOverlay][..]));
        let (s, fx) = run(vec![
            Event::Hotkey(Hotkey::AllInOne),
            Event::Hotkey(Hotkey::Ocr),
        ]);
        assert_eq!(s, State::Picking(Pick::Text));
        assert_eq!(fx[1], [Effect::SetOverlayMode(Pick::Text)]);
    }

    /// "Capture previous area": `feed` resolves the stored rect, the machine takes the still at once.
    #[test]
    fn the_previous_area_is_captured_without_an_overlay() {
        let ev = Event::Previous {
            rect: R,
            display: 1,
        };
        let (s, fx) = run(vec![ev.clone()]);
        assert_eq!(s, State::Capturing);
        assert_eq!(
            fx[0],
            [Effect::Still(Still::Region {
                rect: R,
                display: 1
            })]
        );
        // inside an overlay it closes it first
        let (s, fx) = run(vec![Event::Hotkey(Hotkey::Region), ev.clone()]);
        assert_eq!(s, State::Capturing);
        assert_eq!(fx[1][0], Effect::HideOverlay);
        // not while a still or a recording is running
        assert_eq!(
            step(&State::Capturing, ev.clone()),
            (State::Capturing, vec![])
        );
        let rec = State::Recording {
            phase: Phase::Recording,
            display: 1,
            rect: None,
            gif: false,
        };
        assert_eq!(step(&rec, ev), (rec.clone(), vec![]));
        // the bare key (no stored area resolved) does nothing
        assert_eq!(
            step(&State::Idle, Event::Hotkey(Hotkey::Previous)),
            (State::Idle, vec![])
        );
    }

    /// Only a dragged region can be "the previous area".
    #[test]
    fn only_a_region_commit_is_remembered_as_the_previous_area() {
        let r = Commit::Region {
            rect: R,
            display: 4,
            action: Action::Capture,
        };
        assert_eq!(region_of(&r), Some((R, 4)));
        let w = Commit::Window {
            id: 1,
            action: Action::Capture,
        };
        assert_eq!(region_of(&w), None);
        let s = Commit::Screen {
            display: 1,
            action: Action::Capture,
        };
        assert_eq!(region_of(&s), None);
    }

    #[test]
    fn the_commit_action_arrives_as_the_lowercase_word_the_page_sends() {
        for (w, a) in [
            ("capture", Action::Capture),
            ("record", Action::Record),
            ("gif", Action::Gif),
            ("text", Action::Text),
        ] {
            assert_eq!(
                serde_json::from_value::<Action>(serde_json::json!(w)).unwrap(),
                a
            );
        }
        assert!(serde_json::from_value::<Action>(serde_json::json!("Capture")).is_err());
    }
}
