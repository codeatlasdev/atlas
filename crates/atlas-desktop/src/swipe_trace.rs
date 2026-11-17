//! Replays the trace fixtures (`apps/desktop/src/lib/shell/__fixtures__/traces/*.json`: synthesized from the
//! trackpad physics, plus real recordings from `atlas desktop debug gesture-record`) through the real
//! [`Engine`], at the pace the events were recorded, with a 120 Hz tick during settles, and asserts what a person
//! making that motion means. The fixtures are the exact JSON the frontend suite and the in-app replay read.
//!
//! Test-only: the replay lives here so the physics and decisions are provable without a window or a trackpad.

use serde::Deserialize;

use crate::swipe_engine::{Engine, Hit, Sample, Scroll, Stage, Step};

#[derive(Deserialize)]
pub struct Expect {
    pub zone: String,
    pub steps: i8,
    #[serde(default)]
    pub untouched: bool,
    #[serde(default)]
    pub edge: Option<String>,
    #[serde(default)]
    pub gestures: Option<u64>,
    /// What scrolls horizontally under the pointer when the fingers land (an editor, a diff): which way it still can.
    #[serde(default)]
    pub scroller: Option<Scroller>,
}

#[derive(Deserialize)]
pub struct Scroller {
    pub left: bool,
    pub right: bool,
}

#[derive(Deserialize)]
pub struct Trace {
    pub name: String,
    pub expect: Expect,
    pub events: Vec<Sample>,
}

/// What the engine is told about the pointer for a trace: the zone it hits and what scrolls under it.
#[derive(Clone, Copy)]
pub struct Scene {
    pub hit: Option<Hit>,
    pub scroll: Scroll,
}

impl Trace {
    pub fn scene(&self) -> Scene {
        Scene {
            hit: hit_for(&self.expect.zone, self.expect.edge.as_deref()),
            scroll: self
                .expect
                .scroller
                .as_ref()
                .map_or(Scroll::default(), |s| Scroll {
                    left: s.left,
                    right: s.right,
                }),
        }
    }
}

/// Everything the engine told its consumer over a whole trace.
#[derive(Default, Debug)]
pub struct Outcome {
    /// Distinct claimed gestures (each `seq` once).
    pub gestures: u64,
    /// Commits in order: (seq, dir).
    pub commits: Vec<(u64, i8)>,
    /// Frames per gesture, in delivery order.
    pub frames: Vec<(u64, f64, Stage)>,
    pub swallowed: usize,
    pub passed: usize,
    /// Momentum events of a claimed gesture that were NOT swallowed (the page would scroll after the swipe).
    pub leaked_tail: usize,
    pub haptics: usize,
}

impl Outcome {
    /// Net page steps after the whole trace: each commit's direction applies exactly once.
    pub fn net(&self) -> i32 {
        self.commits.iter().map(|(_, d)| i32::from(*d)).sum()
    }

    /// Largest |amount| any frame reported.
    pub fn max_amount(&self) -> f64 {
        self.frames.iter().map(|f| f.1.abs()).fold(0.0, f64::max)
    }
}

/// The hit the zone table resolves for a trace: any swipeable zone pages by [`PAGE_TRAVEL`] of finger travel.
pub fn hit_for(zone: &str, edge: Option<&str>) -> Option<Hit> {
    if !matches!(zone, "space" | "tab" | "history") {
        return None;
    }
    Some(Hit {
        prev: edge != Some("first"),
        next: edge != Some("last"),
        width: crate::swipe_engine::PAGE_TRAVEL,
    })
}

fn apply(out: &mut Outcome, st: &Step, seen: &mut u64) {
    if st.swallow {
        out.swallowed += 1;
    } else {
        out.passed += 1;
    }
    if let Some(f) = st.frame {
        if f.seq > *seen {
            *seen = f.seq;
            out.gestures += 1;
        }
        out.frames.push((f.seq, f.amount, f.stage));
    }
    if let Some(c) = st.commit {
        out.commits.push((c.seq, c.dir));
    }
    if st.threshold.is_some() {
        out.haptics += 1;
    }
}

/// Replay `events` at their own timestamps, ticking the engine every 8.33 ms (120 Hz) in between.
pub fn replay(events: &[Sample], scene: Scene) -> Outcome {
    replay_on(&mut Engine::new(), events, scene)
}

/// Replay on a given engine: the app keeps ONE engine for its whole life, so what a previous gesture left behind
/// (the origin after a settle, a generation counter) is part of every later gesture.
pub fn replay_on(e: &mut Engine, events: &[Sample], scene: Scene) -> Outcome {
    let mut out = Outcome::default();
    let mut seen = e.seq();
    let mut clock = events.first().map_or(0.0, |s| s.t);
    for s in events {
        let tail = s.momentum != crate::swipe_engine::Momentum::None;
        while clock + 1000.0 / 120.0 < s.t {
            clock += 1000.0 / 120.0;
            let st = e.tick(clock);
            apply(&mut out, &st, &mut seen);
        }
        clock = clock.max(s.t);
        let ours = e.seq() > 0 && e.busy();
        let st = e.feed(s, |_, _| scene.scroll, |_, _| scene.hit);
        if tail && ours && !st.swallow {
            out.leaked_tail += 1;
        }
        apply(&mut out, &st, &mut seen);
    }
    // Let the last settle finish.
    for _ in 0..240 {
        clock += 1000.0 / 120.0;
        let st = e.tick(clock);
        apply(&mut out, &st, &mut seen);
        if !e.busy() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn traces() -> Vec<Trace> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/shell/__fixtures__/traces");
        let mut v: Vec<Trace> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(Result::ok)
            .filter(|f| f.path().extension().is_some_and(|x| x == "json"))
            .map(|f| {
                let raw = std::fs::read_to_string(f.path()).unwrap();
                serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {e}", f.path().display()))
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    #[test]
    fn the_fixture_suite_is_present() {
        assert!(
            traces().len() >= 20,
            "regenerate with `bun scripts/gesture-fixtures.ts`"
        );
    }

    /// The whole suite through the real engine: one assertion per trace, all failures reported together.
    #[test]
    fn every_trace_ends_on_exactly_the_page_the_person_meant() {
        let mut bad = vec![];
        for t in traces() {
            let out = replay(&t.events, t.scene());
            let net = out.net();
            if net != i32::from(t.expect.steps) {
                bad.push(format!(
                    "{}: wanted {:+} steps, engine did {net:+} ({:?})",
                    t.name, t.expect.steps, out.commits
                ));
            }
            let want_g = t.expect.gestures.unwrap_or(u64::from(t.expect.steps != 0));
            if (t.expect.steps != 0 || t.expect.gestures.is_some()) && out.gestures != want_g {
                bad.push(format!(
                    "{}: wanted {want_g} gesture(s), engine claimed {}",
                    t.name, out.gestures
                ));
            }
            if t.expect.untouched && (out.gestures != 0 || out.swallowed != 0) {
                bad.push(format!(
                    "{}: must stay with the page's own scrolling but claimed {} events",
                    t.name, out.swallowed
                ));
            }
            if t.expect.edge.is_some() && out.max_amount() > 0.151 {
                bad.push(format!(
                    "{}: rubber band went to {:.3} pages",
                    t.name,
                    out.max_amount()
                ));
            }
        }
        assert!(bad.is_empty(), "\n{}", bad.join("\n"));
    }

    /// Incident (live bench): every trace passed on a fresh engine but the app reuses one, and the second gesture
    /// after a committed swipe started from the previous page's end. The whole suite, back to back on ONE engine,
    /// in two different orders, must still end on the page each trace means.
    #[test]
    fn the_suite_back_to_back_on_one_engine_gives_the_same_decisions_as_each_alone() {
        let ts = traces();
        for order in [
            ts.iter().collect::<Vec<_>>(),
            ts.iter().rev().collect::<Vec<_>>(),
        ] {
            let mut e = Engine::new();
            let mut t0 = 0.0;
            for t in order {
                // Shift the trace to after the previous one so the clock only moves forward.
                let shifted: Vec<Sample> = t
                    .events
                    .iter()
                    .map(|s| Sample { t: s.t + t0, ..*s })
                    .collect();
                t0 = shifted.last().map_or(t0, |s| s.t) + 2000.0;
                let alone = replay(&t.events, t.scene());
                let chained = replay_on(&mut e, &shifted, t.scene());
                assert_eq!(
                    chained.net(),
                    alone.net(),
                    "{}: decided differently after other gestures ({:?} vs {:?})",
                    t.name,
                    chained.commits,
                    alone.commits
                );
            }
        }
    }

    #[test]
    fn the_momentum_tail_of_a_swipe_never_reaches_the_page() {
        for t in traces() {
            let out = replay(&t.events, t.scene());
            assert_eq!(
                out.leaked_tail, 0,
                "{}: the page scrolled under the swipe's inertia",
                t.name
            );
        }
    }

    #[test]
    fn no_trace_ever_moves_more_than_one_page_per_gesture() {
        for t in traces() {
            let out = replay(&t.events, t.scene());
            for (seq, dir) in &out.commits {
                assert!(
                    dir.abs() <= 1,
                    "{}: gesture {seq} moved {dir} pages",
                    t.name
                );
            }
            let per_seq = |s: u64| out.commits.iter().filter(|(q, _)| *q == s).count();
            for (seq, _) in &out.commits {
                assert_eq!(
                    per_seq(*seq),
                    1,
                    "{}: gesture {seq} decided more than once",
                    t.name
                );
            }
        }
    }

    #[test]
    fn no_settle_overshoots_the_page_it_commits_to() {
        for t in traces() {
            let out = replay(&t.events, t.scene());
            // Per gesture: from its commit on, |amount| must approach the target monotonically.
            for (seq, dir) in &out.commits {
                let target = f64::from(*dir);
                let mut prev: Option<f64> = None;
                for (s, amount, stage) in &out.frames {
                    if s != seq || *stage != Stage::Settle {
                        continue;
                    }
                    if let Some(p) = prev {
                        assert!(
                            (target - amount).abs() <= (target - p).abs() + 1e-9,
                            "{}: gesture {seq} backed away from {target}: {p} → {amount}",
                            t.name
                        );
                    }
                    prev = Some(*amount);
                    assert!(
                        (*amount - target).abs() <= 1.0 + 0.151,
                        "{}: gesture {seq} left the page: {amount}",
                        t.name
                    );
                }
            }
        }
    }
}
