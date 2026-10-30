//! The torn-frame oracle of `scripts/term-agent-bench.ts` reads a column of cell backgrounds: a whole producer frame
//! has one colour on every row, and two producer frames mixed on one screen show two. That contract is only worth
//! anything if the REAL terminal parses the generator's real bytes into exactly that, so this test runs
//! `scripts/term-agent-gen.ts --dump` (the generator's own frames, not a copy of them here) and feeds them to `Vt`.
//! Skipped, loudly, when `bun` is not installed: the repo's own gate always has it.

use std::path::{Path, PathBuf};
use std::process::Command;

use alacritty_terminal::vte::ansi::Rgb;
use atlas_term::colors::{Palette, Rgba};
use atlas_term::{Frame, Vt};

const COLS: u16 = 40;
const ROWS: u16 = 12;

fn pal() -> Palette {
    Palette::new(
        Rgb {
            r: 200,
            g: 200,
            b: 200,
        },
        Rgb {
            r: 10,
            g: 10,
            b: 10,
        },
        None,
    )
}

/// Run the generator's `--dump` into a fresh dir and return it (`None` = no bun here).
fn dump(test: &str) -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir =
        std::env::temp_dir().join(format!("atlas-agent-frames-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = Command::new("bun")
        .arg(root.join("scripts/term-agent-gen.ts"))
        .arg("--dump")
        .arg(&dir)
        .output()
        .ok()?;
    assert!(
        out.status.success(),
        "term-agent-gen --dump failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(dir)
}

/// The background of each row's first cell: the probe column the bench samples.
fn probe(vt: &Vt) -> Vec<Rgba> {
    let mut f = Frame {
        focused: true,
        blink_on: true,
        ..Frame::default()
    };
    vt.snapshot(&pal(), &mut f);
    (0..f.rows as usize)
        .map(|r| f.bg[r * f.cols as usize])
        .collect()
}

fn screen_after(chunks: &[&[u8]]) -> Vec<Rgba> {
    let vt = Vt::new(COLS, ROWS, |_| {});
    // A fresh alternate screen, as the generator starts.
    vt.feed(b"\x1b[?1049h\x1b[2J");
    for c in chunks {
        vt.feed(c);
    }
    probe(&vt)
}

const PATTERNS: [&str; 5] = [
    "ink-2026",
    "ink-no-2026",
    "ratatui-diff",
    "omp-boxes",
    "slow-2026",
];

fn distinct(col: &[Rgba]) -> usize {
    // Rows that show a frame colour (a saturated one); the clear colour is the theme background.
    let clear = pal().bg(&Default::default());
    let mut seen: Vec<Rgba> = Vec::new();
    for c in col {
        if *c != [clear.r, clear.g, clear.b, 255] && !seen.contains(c) {
            seen.push(*c);
        }
    }
    seen.len()
}

#[test]
fn a_whole_generator_frame_shows_one_colour_on_every_row_and_half_of_the_next_shows_two() {
    let Some(dir) = dump("tear") else {
        eprintln!("SKIPPED: bun is not installed, the generator cannot be dumped");
        return;
    };
    for id in PATTERNS {
        let f0 = std::fs::read(dir.join(format!("{id}.0"))).unwrap();
        let f1 = std::fs::read(dir.join(format!("{id}.1"))).unwrap();

        let whole = screen_after(&[&f0]);
        assert_eq!(whole.len(), ROWS as usize);
        assert_eq!(
            distinct(&whole),
            1,
            "{id}: frame 0 must paint one colour down the probe column: {whole:?}"
        );

        // Frame 1 completes over frame 0: still one colour, and not the same one (the id moved).
        let both = screen_after(&[&f0, &f1]);
        assert_eq!(distinct(&both), 1, "{id}: frame 1 over frame 0: {both:?}");
        assert_ne!(
            both, whole,
            "{id}: the colour must change with the frame id, or the oracle cannot tell frames apart"
        );

        // What a renderer shows when it draws in the middle of frame 1: both ids on one screen. 2026 hides this
        // from the terminal (the parser buffers until ESU), so the cut is taken inside the update here.
        let cut = f1.len() / 2;
        let torn = {
            let vt = Vt::new(COLS, ROWS, |_| {});
            vt.feed(b"\x1b[?1049h\x1b[2J");
            vt.feed(&f0);
            // Strip the 2026 wrapper of the cut part: the test is about what an unsynchronised draw shows.
            let part: Vec<u8> = String::from_utf8_lossy(&f1[..cut])
                .replace("\x1b[?2026h", "")
                .into_bytes();
            vt.feed(&part);
            probe(&vt)
        };
        assert_eq!(
            distinct(&torn),
            2,
            "{id}: half of frame 1 over frame 0 must show two ids: {torn:?}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

// DEC 2026: with the update open, the grid keeps showing the previous complete frame; when it closes, the new one.
// This is what keeps `ink-2026`, `ratatui-diff`, `omp-boxes` and `slow-2026` from ever being torn on screen, and why
// the bench expects 0 torn captures from them.
#[test]
fn a_2026_update_shows_only_whole_frames_to_the_renderer() {
    let Some(dir) = dump("sync") else {
        eprintln!("SKIPPED: bun is not installed, the generator cannot be dumped");
        return;
    };
    for id in ["ink-2026", "ratatui-diff", "omp-boxes", "slow-2026"] {
        let f0 = std::fs::read(dir.join(format!("{id}.0"))).unwrap();
        let f1 = std::fs::read(dir.join(format!("{id}.1"))).unwrap();
        let vt = Vt::new(COLS, ROWS, |_| {});
        vt.feed(b"\x1b[?1049h\x1b[2J");
        vt.feed(&f0);
        let before = probe(&vt);
        vt.feed(&f1[..f1.len() / 2]);
        assert_eq!(
            probe(&vt),
            before,
            "{id}: a half-fed 2026 frame must not reach the grid"
        );
        vt.feed(&f1[f1.len() / 2..]);
        assert_ne!(probe(&vt), before, "{id}: the completed frame shows");
        assert_eq!(distinct(&probe(&vt)), 1, "{id}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
