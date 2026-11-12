//! Paints the setup window into a pixmap: the same on every OS, so the tests look at real pixels.
//!
//! The mark is the Atlas globe, tile by tile, from `assets/logo-animated.svg` (`LOGO_TILES`): each
//! tile pops in with the SVG's own stagger and overshoot, then the whole mark waves in opacity like
//! the app's. Text is filled from `skrifa` outlines (`ui::font`): Segoe UI when Windows has it, else the
//! bundled JetBrains Mono, so it is crisp at any DPI scale without a text stack.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Point, RadialGradient, Rect as SkRect, SpreadMode, Stroke, Transform,
};

use crate::cli::Mode;
use crate::state::{Phase, Snapshot};
use crate::ui::font::Fonts;
use crate::ui::layout::{self, Button, HEIGHT, RADIUS, WIDTH};

include!(concat!(env!("OUT_DIR"), "/logo.rs"));

/// Colors of `packaging/windows/installer-sidebar.svg` (the brand art the NSIS wizard used).
mod color {
    pub const BG_TOP: [u8; 3] = [0x0E, 0x15, 0x30];
    pub const BG_MID: [u8; 3] = [0x0A, 0x0F, 0x22];
    pub const BG_BOTTOM: [u8; 3] = [0x07, 0x0A, 0x16];
    pub const ACCENT: [u8; 3] = [0x3D, 0x5B, 0xFF];
    pub const ACCENT_LIGHT: [u8; 3] = [0xBF, 0xD0, 0xFF];
    pub const ACCENT_DEEP: [u8; 3] = [0x1B, 0x33, 0xF0];
    pub const TEXT: [u8; 3] = [0xFF, 0xFF, 0xFF];
    pub const MUTED: [u8; 3] = [0x9F, 0xB0, 0xE8];
    pub const DIM: [u8; 3] = [0x5C, 0x6A, 0x99];
    pub const ERROR: [u8; 3] = [0xFF, 0x6B, 0x7A];
    pub const OK: [u8; 3] = [0x5B, 0xE3, 0xA0];
}

fn rgba(c: [u8; 3], a: f32) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// An anti-aliased paint with a flat color.
fn solid(c: Color) -> Paint<'static> {
    let mut p = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    p.set_color(c);
    p
}

/// An anti-aliased paint with a gradient.
fn shaded(shader: tiny_skia::Shader<'static>) -> Paint<'static> {
    Paint {
        anti_alias: true,
        shader,
        ..Paint::default()
    }
}

/// What to draw this frame.
pub struct Frame<'a> {
    pub snap: &'a Snapshot,
    /// The progress shown (eased toward `snap.progress`).
    pub shown: f32,
    /// Seconds since the window opened: drives the animation.
    pub t: f32,
    /// Seconds since the phase became `Done` (0 before).
    pub done_t: f32,
    pub hover: Option<Button>,
    pub close_hover: bool,
    /// Device pixels per logical pixel.
    pub scale: f32,
    pub fonts: &'a Fonts,
}

/// Cubic-bezier `(0.2, 0.9, 0.3, 1.25)` of the SVG's `pop`, solved for `y` at progress `x`.
fn pop_ease(x: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.2f32, 0.9f32, 0.3f32, 1.25f32);
    let bez = |t: f32, a: f32, b: f32| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3)
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if bez(mid, x1, x2) < x {
            lo = mid
        } else {
            hi = mid
        }
    }
    bez((lo + hi) / 2.0, y1, y2)
}

/// `(scale, opacity)` of tile `i` at time `t`: pop in over 0.6 s after its delay, then the wave
/// (opacity dips to .78 for a beat every 3.2 s, offset per tile, like `@keyframes wave`).
pub fn tile_state(delay: f32, t: f32) -> (f32, f32) {
    let k = ((t - delay) / 0.6).clamp(0.0, 1.0);
    if t < delay {
        return (0.55, 0.0);
    }
    let e = pop_ease(k);
    let scale = 0.55 + 0.45 * e;
    let mut opacity = k.min(1.0);
    let wave_start = 1.8 + delay * 2.0;
    if t > wave_start {
        let ph = ((t - wave_start) % 3.2) / 3.2;
        // 0..70 % and 100 %: 1.0; dips to .78 at 35 %.
        if ph < 0.7 {
            let d = 1.0 - (ph / 0.35 - 1.0).abs();
            opacity *= 1.0 - 0.22 * d.max(0.0);
        }
    }
    (scale, opacity)
}

/// Logical px of soft shadow around the card in the real window.
pub const SHADOW: f32 = 22.0;

/// The card plus a soft drop shadow, `(WIDTH + 2·SHADOW) × (HEIGHT + 2·SHADOW)` logical px: what the
/// layered window shows (the DWM shadow does not apply to per-pixel-alpha windows).
pub fn paint_window(frame: &Frame) -> Pixmap {
    let s = frame.scale;
    let m = SHADOW;
    let (w, h) = (
        ((WIDTH + 2.0 * m) * s).round() as u32,
        ((HEIGHT + 2.0 * m) * s).round() as u32,
    );
    let mut out = Pixmap::new(w, h).expect("window pixmap");
    let tf = Transform::from_scale(s, s);
    // Stacked translucent rounded rects, growing outward and fading: a cheap, smooth falloff.
    let steps = 11;
    for i in 0..steps {
        let grow = m * (1.0 - i as f32 / steps as f32);
        let a = 0.012 + 0.010 * (i as f32 / steps as f32);
        let p = solid(Color::from_rgba8(0, 0, 0, (a * 255.0) as u8));
        let path = rounded(
            m - grow,
            m - grow + 5.0,
            WIDTH + 2.0 * grow,
            HEIGHT + 2.0 * grow,
            RADIUS + grow,
        );
        out.fill_path(&path, &p, FillRule::Winding, tf, None);
    }
    let card = paint(frame);
    out.draw_pixmap(
        (m * s).round() as i32,
        (m * s).round() as i32,
        card.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    out
}

/// Paint one frame. The pixmap is `WIDTH*scale × HEIGHT*scale`; pixels outside the rounded window
/// stay transparent (the window is per-pixel alpha).
pub fn paint(frame: &Frame) -> Pixmap {
    let s = frame.scale;
    let (w, h) = ((WIDTH * s).round() as u32, (HEIGHT * s).round() as u32);
    let mut pm = Pixmap::new(w, h).expect("window pixmap");
    let tf = Transform::from_scale(s, s);

    // Everything is clipped to the rounded window.
    let clip = rounded(0.0, 0.0, WIDTH, HEIGHT, RADIUS);
    let mut mask = Mask::new(w, h).expect("mask");
    mask.fill_path(&clip, FillRule::Winding, true, tf);

    background(&mut pm, tf, &mask);
    logo(&mut pm, frame, tf, &mask);
    texts(&mut pm, frame, tf);
    progress_bar(&mut pm, frame, tf, &mask);
    for b in layout::buttons(frame.snap) {
        button(&mut pm, frame, &b, tf, &mask);
    }
    close_box(&mut pm, frame, tf, &mask);

    // Hairline border, on top.
    let p = solid(rgba(color::TEXT, 0.09));
    let border = rounded(0.5, 0.5, WIDTH - 1.0, HEIGHT - 1.0, RADIUS - 0.5);
    pm.stroke_path(
        &border,
        &p,
        &Stroke {
            width: 1.0,
            ..Stroke::default()
        },
        tf,
        None,
    );
    pm
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
    let mut pb = PathBuilder::new();
    let k = r * 0.552_284_8;
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().expect("rounded rect")
}

fn background(pm: &mut Pixmap, tf: Transform, mask: &Mask) {
    let full = PathBuilder::from_rect(SkRect::from_xywh(0.0, 0.0, WIDTH, HEIGHT).expect("rect"));
    let p = shaded(
        LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(WIDTH * 0.35, HEIGHT),
            vec![
                GradientStop::new(0.0, rgba(color::BG_TOP, 1.0)),
                GradientStop::new(0.6, rgba(color::BG_MID, 1.0)),
                GradientStop::new(1.0, rgba(color::BG_BOTTOM, 1.0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .expect("bg gradient"),
    );
    pm.fill_path(&full, &p, FillRule::Winding, tf, Some(mask));

    // Faint 12 px grid, as in the NSIS sidebar art.
    let mut grid = PathBuilder::new();
    let mut x = 0.0;
    while x <= WIDTH {
        grid.move_to(x + 0.5, 0.0);
        grid.line_to(x + 0.5, HEIGHT);
        x += 12.0;
    }
    let mut y = 0.0;
    while y <= HEIGHT {
        grid.move_to(0.0, y + 0.5);
        grid.line_to(WIDTH, y + 0.5);
        y += 12.0;
    }
    let gp = solid(rgba(color::TEXT, 0.022));
    if let Some(path) = grid.finish() {
        pm.stroke_path(
            &path,
            &gp,
            &Stroke {
                width: 1.0,
                ..Stroke::default()
            },
            tf,
            Some(mask),
        );
    }

    // The glow behind the mark.
    let glow = shaded(
        RadialGradient::new(
            Point::from_xy(WIDTH / 2.0, 88.0),
            Point::from_xy(WIDTH / 2.0, 88.0),
            190.0,
            vec![
                GradientStop::new(0.0, rgba(color::ACCENT, 0.42)),
                GradientStop::new(1.0, rgba(color::ACCENT, 0.0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .expect("glow"),
    );
    pm.fill_path(&full, &glow, FillRule::Winding, tf, Some(mask));
}

/// Parse SVG path data (`M C L Z`, absolute; what the logo uses) into a path.
fn svg_path(d: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut nums: Vec<f32> = Vec::new();
    let mut cmd = ' ';
    let flush = |pb: &mut PathBuilder, cmd: char, n: &mut Vec<f32>| {
        match cmd {
            'M' if n.len() >= 2 => pb.move_to(n[0], n[1]),
            'L' if n.len() >= 2 => pb.line_to(n[0], n[1]),
            'C' if n.len() >= 6 => pb.cubic_to(n[0], n[1], n[2], n[3], n[4], n[5]),
            _ => {}
        }
        n.clear();
    };
    let mut tok = String::new();
    let push_tok = |tok: &mut String, nums: &mut Vec<f32>| {
        if !tok.is_empty() {
            if let Ok(v) = tok.parse::<f32>() {
                nums.push(v);
            }
            tok.clear();
        }
    };
    for ch in d.chars() {
        match ch {
            'M' | 'L' | 'C' | 'Z' => {
                push_tok(&mut tok, &mut nums);
                flush(&mut pb, cmd, &mut nums);
                cmd = ch;
                if ch == 'Z' {
                    pb.close();
                    cmd = ' ';
                }
            }
            ' ' | ',' => push_tok(&mut tok, &mut nums),
            '-' => {
                push_tok(&mut tok, &mut nums);
                tok.push('-');
            }
            _ => tok.push(ch),
        }
        // Implicit repetition of C/L: flush every full argument group.
        let need = match cmd {
            'C' => 6,
            'L' | 'M' => 2,
            _ => usize::MAX,
        };
        if nums.len() == need {
            flush(&mut pb, cmd, &mut nums);
            if cmd == 'M' {
                cmd = 'L';
            }
        }
    }
    push_tok(&mut tok, &mut nums);
    let need = match cmd {
        'C' => 6,
        'L' | 'M' => 2,
        _ => usize::MAX,
    };
    if nums.len() == need {
        flush(&mut pb, cmd, &mut nums);
    }
    pb.finish()
}

fn logo(pm: &mut Pixmap, f: &Frame, tf: Transform, mask: &Mask) {
    let r = layout::LOGO;
    // SVG viewBox `-10 -10 448 447`: fit into the logo box.
    let k = r.w / 448.0;
    let breathe = if f.snap.phase == Phase::Working {
        1.0 + 0.012 * (f.t * 1.6).sin()
    } else {
        1.0
    };
    let done_pulse = if f.snap.phase == Phase::Done {
        1.0 + 0.06 * (1.0 - (f.done_t / 0.5).min(1.0)) * (f.done_t * 14.0).sin().abs()
    } else {
        1.0
    };
    let gradient = |alpha: f32| {
        shaded(
            LinearGradient::new(
                Point::from_xy(40.0, 20.0),
                Point::from_xy(390.0, 410.0),
                vec![
                    GradientStop::new(0.0, rgba(color::ACCENT_LIGHT, alpha)),
                    GradientStop::new(0.45, rgba(color::ACCENT, alpha)),
                    GradientStop::new(1.0, rgba(color::ACCENT_DEEP, alpha)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            )
            .expect("logo gradient"),
        )
    };
    for (d, delay) in LOGO_TILES {
        let (sc, op) = tile_state(*delay, f.t);
        if op <= 0.003 {
            continue;
        }
        let Some(path) = svg_path(d) else { continue };
        let b = path.bounds();
        let (cx, cy) = (b.left() + b.width() / 2.0, b.top() + b.height() / 2.0);
        // logo space → window space, scaling each tile about its own centre.
        let (ox, oy) = (r.x - (-10.0) * k, r.y - (-10.0) * k);
        let whole = Transform::from_translate(ox, oy)
            .pre_scale(k * breathe * done_pulse, k * breathe * done_pulse);
        let tile = Transform::from_translate(cx, cy)
            .pre_scale(sc, sc)
            .pre_translate(-cx, -cy);
        let t = tf.pre_concat(whole).pre_concat(tile);
        pm.fill_path(&path, &gradient(op), FillRule::Winding, t, Some(mask));
    }
}

fn texts(pm: &mut Pixmap, f: &Frame, tf: Transform) {
    let snap = f.snap;
    let (title, title_color) = match (snap.phase, snap.mode) {
        // The same title the clipboard text carries (state.status), so what is read is what is copied.
        (Phase::Failed, _) => (snap.status.as_str(), color::ERROR),
        (Phase::Done, _) => (snap.status.as_str(), color::OK),
        (Phase::AskData, _) => ("Keep your Atlas data?", color::TEXT),
        (Phase::ConfirmRemove, _) => ("Delete your Atlas data?", color::TEXT),
        _ => (snap.status.as_str(), color::TEXT),
    };
    text_center(pm, f, tf, title, 176.0, Ink::new(20.0, true, title_color));
    match snap.phase {
        Phase::Failed => {
            let measure = |s: &str| f.fonts.measure(s, 12.5, false);
            for (i, line) in failure_lines(snap, &measure).iter().enumerate() {
                text_center(
                    pm,
                    f,
                    tf,
                    line,
                    BODY_Y + i as f32 * BODY_STEP,
                    Ink::new(12.0, false, color::MUTED),
                );
            }
        }
        Phase::AskData | Phase::ConfirmRemove => {
            let measure = |s: &str| f.fonts.measure(s, 12.0, false);
            let text = if snap.phase == Phase::AskData {
                ASK_TEXT
            } else {
                CONFIRM_TEXT
            };
            for (i, line) in layout::wrap(text, WIDTH - 72.0, QUESTION_LINES, &measure)
                .iter()
                .enumerate()
            {
                text_center(
                    pm,
                    f,
                    tf,
                    line,
                    BODY_Y + i as f32 * BODY_STEP,
                    Ink::new(12.0, false, color::MUTED),
                );
            }
        }
        Phase::Done => {
            let line = if snap.note.is_empty() {
                "Done."
            } else {
                snap.note.as_str()
            };
            let measure = |s: &str| f.fonts.measure(s, 13.0, false);
            for (i, l) in layout::wrap(line, WIDTH - 72.0, 2, &measure)
                .iter()
                .enumerate()
            {
                text_center(
                    pm,
                    f,
                    tf,
                    l,
                    204.0 + i as f32 * 17.0,
                    Ink::new(13.0, false, color::MUTED),
                );
            }
        }
        Phase::Working | Phase::Cancelled => {
            let line = if !snap.note.is_empty() {
                snap.note.as_str()
            } else if snap.mode == Mode::Install {
                "This takes a few seconds."
            } else {
                ""
            };
            text_center(
                pm,
                f,
                tf,
                line,
                204.0,
                Ink::new(13.0, false, color::MUTED).faded(0.9),
            );
        }
    }
    text_center(
        pm,
        f,
        tf,
        "atlas.codeatlas.com.br",
        HEIGHT - 11.0,
        Ink::new(10.0, false, color::DIM),
    );
}

/// Lines of the failure screen's body. The reason (first paragraph of the details) may take
/// [`REASON_LINES`] and is cut with `…` beyond that (Copy details has all of it); the log and the
/// next step always show whole.
const REASON_LINES: usize = 2;
const TAIL_LINES: usize = 3;

fn failure_lines(snap: &Snapshot, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let details = if snap.details.is_empty() {
        snap.status.as_str()
    } else {
        snap.details.as_str()
    };
    let (reason, tail) = details.split_once('\n').unwrap_or((details, ""));
    let mut lines = layout::wrap(reason, WIDTH - 80.0, REASON_LINES, measure);
    lines.extend(layout::wrap(tail, WIDTH - 80.0, TAIL_LINES, measure));
    lines
}
const QUESTION_LINES: usize = 4;
/// Baseline of the first body line and the step between lines on the text screens (buttons at 262).
const BODY_Y: f32 = 196.0;
const BODY_STEP: f32 = 14.0;

/// The data question. It names what deleting removes: the data folder, the worktrees Atlas made
/// (which can hold uncommitted work) and the `atlas` command; and what it never touches.
pub const ASK_TEXT: &str = "Deleting it removes %USERPROFILE%\\.atlas: settings, your project list, worktrees Atlas created (with any uncommitted changes) and the atlas command. Your project folders are never touched.";
/// The second step, before anything is deleted.
pub const CONFIRM_TEXT: &str = "This deletes %USERPROFILE%\\.atlas, including worktrees Atlas created and any uncommitted changes in them, and the atlas command. It cannot be undone.";

/// How a line of text looks.
#[derive(Clone, Copy)]
struct Ink {
    size: f32,
    bold: bool,
    color: [u8; 3],
    alpha: f32,
}

impl Ink {
    const fn new(size: f32, bold: bool, color: [u8; 3]) -> Self {
        Self {
            size,
            bold,
            color,
            alpha: 1.0,
        }
    }

    const fn faded(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }
}

/// Draw `text` centred on the window's middle, baseline at `y` (logical px).
fn text_center(pm: &mut Pixmap, f: &Frame, tf: Transform, text: &str, y: f32, ink: Ink) {
    let w = f.fonts.measure(text, ink.size, ink.bold);
    text_at(pm, f, tf, text, ((WIDTH - w) / 2.0, y), ink);
}

/// Draw `text` with its baseline origin at `at` (logical px). The glyph outlines are scaled to
/// device pixels and filled by the same anti-aliased rasterizer as everything else.
fn text_at(pm: &mut Pixmap, f: &Frame, tf: Transform, text: &str, at: (f32, f32), ink: Ink) {
    let (x, y) = at;
    let s = tf.sx;
    let mut paint = solid(rgba(ink.color, ink.alpha));
    paint.anti_alias = true;
    for (path, dx) in f.fonts.glyphs(text, ink.size * s, ink.bold) {
        let t = Transform::from_translate(x * s + dx, y * s);
        pm.fill_path(&path, &paint, FillRule::Winding, t, None);
    }
}

fn progress_bar(pm: &mut Pixmap, f: &Frame, tf: Transform, mask: &Mask) {
    if matches!(
        f.snap.phase,
        Phase::Failed | Phase::AskData | Phase::ConfirmRemove
    ) {
        return;
    }
    let r = layout::BAR;
    let track = rounded(r.x, r.y, r.w, r.h, r.h / 2.0);
    let tp = solid(rgba(color::TEXT, 0.09));
    pm.fill_path(&track, &tp, FillRule::Winding, tf, Some(mask));

    let frac = f.shown.clamp(0.0, 1.0);
    // Never narrower than the bar is tall, or the capsule degenerates.
    let fw = (r.w * frac).max(if frac > 0.0 { r.h } else { 0.0 });
    if fw <= 0.0 {
        return;
    }
    let fill = rounded(r.x, r.y, fw, r.h, r.h / 2.0);
    let p = shaded(
        LinearGradient::new(
            Point::from_xy(r.x, 0.0),
            Point::from_xy(r.x + r.w, 0.0),
            vec![
                GradientStop::new(0.0, rgba(color::ACCENT_DEEP, 1.0)),
                GradientStop::new(0.55, rgba(color::ACCENT, 1.0)),
                GradientStop::new(1.0, rgba(color::ACCENT_LIGHT, 1.0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .expect("bar gradient"),
    );
    pm.fill_path(&fill, &p, FillRule::Winding, tf, Some(mask));

    // A soft glow at the head while it moves.
    if f.snap.phase == Phase::Working {
        let hx = r.x + fw;
        let g = shaded(
            RadialGradient::new(
                Point::from_xy(hx, r.y + r.h / 2.0),
                Point::from_xy(hx, r.y + r.h / 2.0),
                14.0,
                vec![
                    GradientStop::new(0.0, rgba(color::ACCENT_LIGHT, 0.55)),
                    GradientStop::new(1.0, rgba(color::ACCENT_LIGHT, 0.0)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            )
            .expect("head glow"),
        );
        if let Some(rect) = SkRect::from_xywh(hx - 14.0, r.y - 14.0, 28.0, r.h + 28.0) {
            pm.fill_rect(rect, &g, tf, Some(mask));
        }
    }
}

fn button(pm: &mut Pixmap, f: &Frame, b: &layout::Placed, tf: Transform, mask: &Mask) {
    let r = b.rect;
    let hot = f.hover == Some(b.button);
    let path = rounded(r.x, r.y, r.w, r.h, 9.0);
    let p = if b.primary {
        shaded(
            LinearGradient::new(
                Point::from_xy(r.x, r.y),
                Point::from_xy(r.x, r.y + r.h),
                vec![
                    GradientStop::new(0.0, rgba(color::ACCENT, if hot { 1.0 } else { 0.92 })),
                    GradientStop::new(1.0, rgba(color::ACCENT_DEEP, if hot { 1.0 } else { 0.92 })),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            )
            .expect("button gradient"),
        )
    } else {
        solid(rgba(color::TEXT, if hot { 0.14 } else { 0.07 }))
    };
    pm.fill_path(&path, &p, FillRule::Winding, tf, Some(mask));
    if !b.primary {
        let sp = solid(rgba(color::TEXT, 0.10));
        pm.stroke_path(
            &rounded(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0, 8.5),
            &sp,
            &Stroke {
                width: 1.0,
                ..Stroke::default()
            },
            tf,
            Some(mask),
        );
    }
    let size = 13.0;
    let tw = f.fonts.measure(b.label, size, true);
    text_at(
        pm,
        f,
        tf,
        b.label,
        (r.x + (r.w - tw) / 2.0, r.y + r.h / 2.0 + size * 0.35),
        Ink::new(size, true, color::TEXT),
    );
}

fn close_box(pm: &mut Pixmap, f: &Frame, tf: Transform, mask: &Mask) {
    let r = layout::CLOSE;
    if f.close_hover {
        let hp = solid(rgba(color::ERROR, 0.85));
        // Top-right corner follows the window's own radius.
        pm.fill_path(
            &rounded(r.x, r.y, r.w, r.h, 7.0),
            &hp,
            FillRule::Winding,
            tf,
            Some(mask),
        );
    }
    let (cx, cy, d) = (r.x + r.w / 2.0, r.y + r.h / 2.0, 4.5);
    let mut pb = PathBuilder::new();
    pb.move_to(cx - d, cy - d);
    pb.line_to(cx + d, cy + d);
    pb.move_to(cx + d, cy - d);
    pb.line_to(cx - d, cy + d);
    let p = solid(rgba(color::TEXT, if f.close_hover { 1.0 } else { 0.55 }));
    if let Some(path) = pb.finish() {
        pm.stroke_path(
            &path,
            &p,
            &Stroke {
                width: 1.4,
                line_cap: tiny_skia::LineCap::Round,
                ..Stroke::default()
            },
            tf,
            Some(mask),
        );
    }
}

/// Premultiplied RGBA → the BGRA a Windows DIB wants (alpha kept: the window is layered).
pub fn to_bgra(pm: &Pixmap) -> Vec<u8> {
    pm.data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|px| [px[2], px[1], px[0], px[3]])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Snapshot;

    fn snap(phase: Phase, progress: f32) -> Snapshot {
        Snapshot {
            mode: Mode::Install,
            phase,
            status: "Installing Atlas…".into(),
            note: String::new(),
            progress,
            details: String::new(),
        }
    }

    fn frame<'a>(s: &'a Snapshot, fonts: &'a Fonts, t: f32, scale: f32) -> Frame<'a> {
        Frame {
            snap: s,
            shown: s.progress,
            t,
            done_t: 0.0,
            hover: None,
            close_hover: false,
            scale,
            fonts,
        }
    }

    fn px(pm: &Pixmap, x: f32, y: f32, scale: f32) -> (u8, u8, u8, u8) {
        let p = pm.pixel((x * scale) as u32, (y * scale) as u32).unwrap();
        (p.red(), p.green(), p.blue(), p.alpha())
    }

    #[test]
    fn the_logo_tiles_come_from_the_animated_svg() {
        assert!(LOGO_TILES.len() >= 30, "{}", LOGO_TILES.len());
        for (d, delay) in LOGO_TILES {
            assert!(svg_path(d).is_some(), "unparsable tile: {d}");
            assert!((0.0..2.0).contains(delay));
        }
    }

    #[test]
    fn a_tile_pops_in_after_its_delay_then_waves_without_vanishing() {
        let (s0, o0) = tile_state(0.5, 0.2);
        assert_eq!((s0, o0), (0.55, 0.0), "not there before its delay");
        let (s1, o1) = tile_state(0.5, 1.2);
        assert!(
            (s1 - 1.0).abs() < 0.02 && (o1 - 1.0).abs() < 1e-3,
            "{s1} {o1}"
        );
        // Across many wave cycles opacity stays within the SVG's .78..1 band.
        for i in 0..400 {
            let (_, o) = tile_state(0.3, 2.0 + i as f32 * 0.05);
            assert!((0.77..=1.0001).contains(&o), "{o}");
        }
    }

    #[test]
    fn pixels_outside_the_rounded_corner_are_transparent_and_inside_are_opaque() {
        let fonts = Fonts::bundled();
        let s = snap(Phase::Working, 0.5);
        for scale in [1.0, 2.0] {
            let pm = paint(&frame(&s, &fonts, 3.0, scale));
            assert_eq!(
                (pm.width(), pm.height()),
                ((WIDTH * scale) as u32, (HEIGHT * scale) as u32)
            );
            assert_eq!(
                px(&pm, 0.0, 0.0, scale).3,
                0,
                "the corner is cut by the radius"
            );
            assert_eq!(
                px(&pm, WIDTH / 2.0, HEIGHT - 40.0, scale).3,
                255,
                "the body is opaque"
            );
        }
    }

    #[test]
    fn the_progress_fill_is_as_wide_as_the_progress_and_never_wider() {
        let fonts = Fonts::bundled();
        let r = layout::BAR;
        let y = r.y + r.h / 2.0;
        let filled = |p: f32| {
            let s = snap(Phase::Working, p);
            let pm = paint(&frame(&s, &fonts, 3.0, 1.0));
            // The fill is the saturated blue; the track is a faint white. Count bluish columns.
            (r.x as u32..(r.x + r.w) as u32)
                .filter(|x| {
                    let (rr, _, b, _) = px(&pm, *x as f32, y, 1.0);
                    b > 180 && b as i32 - rr as i32 > 60
                })
                .count() as f32
        };
        let (a, b, full) = (filled(0.25), filled(0.5), filled(1.0));
        assert!(a > 0.0 && a < b && b < full, "{a} {b} {full}");
        assert!(
            (b / full - 0.5).abs() < 0.06,
            "half the progress, half the bar: {}",
            b / full
        );
        assert!(full <= r.w + 1.0);
    }

    #[test]
    fn the_mark_is_drawn_once_tiles_have_popped_in_and_absent_at_time_zero() {
        let fonts = Fonts::bundled();
        let s = snap(Phase::Working, 0.2);
        let lit = |t: f32| {
            let pm = paint(&frame(&s, &fonts, t, 1.0));
            let r = layout::LOGO;
            let mut n = 0;
            for y in (r.y as u32..(r.y + r.h) as u32).step_by(2) {
                for x in (r.x as u32..(r.x + r.w) as u32).step_by(2) {
                    let (rr, _, b, _) = px(&pm, x as f32, y as f32, 1.0);
                    // Logo blues (#3D5BFF family) are far brighter in blue than the dark backdrop.
                    if b > 200 && (b as i32 - rr as i32) > 40 {
                        n += 1;
                    }
                }
            }
            n
        };
        assert!(lit(3.0) > 300, "the globe is visible: {}", lit(3.0));
        assert!(lit(0.0) < lit(3.0) / 4, "it builds up from nothing");
    }

    #[test]
    fn a_failure_paints_the_error_color_and_hides_the_bar() {
        let fonts = Fonts::bundled();
        let mut s = snap(Phase::Failed, 0.4);
        s.status = "Atlas could not be installed".into();
        s.details =
            "The Atlas installer failed (exit code 2).\nLog: C:\\Temp\\atlas-setup.log".into();
        let pm = paint(&frame(&s, &fonts, 3.0, 1.0));
        let reddish = (0..WIDTH as u32).any(|x| {
            (150..180).any(|y| {
                let (r, g, b, _) = px(&pm, x as f32, y as f32, 1.0);
                r > 200 && g < 150 && b < 160
            })
        });
        assert!(reddish, "the title is drawn in the error color");
        // The bar's row, outside every button: a Working frame has the faint track there, a failure
        // has nothing but the backdrop.
        let outside_buttons: Vec<u32> = (layout::BAR.x as u32
            ..(layout::BAR.x + layout::BAR.w) as u32)
            .filter(|x| {
                !layout::buttons(&s)
                    .iter()
                    .any(|b| b.rect.contains(*x as f32, layout::BAR.y + 1.0))
            })
            .collect();
        assert!(!outside_buttons.is_empty());
        let working = paint(&frame(&snap(Phase::Working, 0.0), &fonts, 3.0, 1.0));
        let row = |pm: &Pixmap| -> u32 {
            outside_buttons
                .iter()
                .map(|x| px(pm, *x as f32, layout::BAR.y + 1.0, 1.0).0 as u32)
                .sum()
        };
        assert!(
            row(&working) > row(&pm),
            "the Working frame shows the bar track, the failure does not"
        );
    }

    #[test]
    fn the_data_question_names_what_deleting_removes() {
        // B1: the screen says what is deleted (worktrees with uncommitted changes, the atlas
        // command), that project folders are never touched, and uses Windows paths.
        assert!(ASK_TEXT.contains("%USERPROFILE%\\.atlas"));
        assert!(ASK_TEXT.contains("worktrees Atlas created (with any uncommitted changes)"));
        assert!(ASK_TEXT.contains("atlas command"));
        assert!(ASK_TEXT.contains("Your project folders are never touched."));
        assert!(CONFIRM_TEXT.contains("cannot be undone"));
        assert!(
            !ASK_TEXT.contains("~/"),
            "Windows-facing text uses %USERPROFILE%"
        );
        // And it all fits the window at the font the installer really uses.
        let fonts = Fonts::bundled();
        let measure = |s: &str| fonts.measure(s, 12.0, false);
        for t in [ASK_TEXT, CONFIRM_TEXT] {
            let lines = layout::wrap(t, WIDTH - 72.0, 99, &measure);
            assert!(
                lines.len() <= QUESTION_LINES,
                "{} lines: {lines:?}",
                lines.len()
            );
        }
    }

    #[test]
    fn a_failure_shows_the_reason_the_log_path_and_the_next_step_untruncated() {
        let fonts = Fonts::bundled();
        let reason = "The installer finished but C:\\Users\\ana\\AppData\\Local\\Atlas\\atlas-desktop.exe is missing.";
        let mut s = snap(Phase::Failed, 0.4);
        s.details = format!(
            "{reason}\nLog: C:\\Users\\ana\\AppData\\Local\\Temp\\atlas-setup.log\n{}",
            crate::install::NEXT_STEP
        );
        let measure = |t: &str| fonts.measure(t, 12.0, false);
        let lines = failure_lines(&s, &measure);
        assert!(lines.len() <= REASON_LINES + TAIL_LINES, "{lines:#?}");
        // The log path and the way out are whole, never cut; only the long reason may be.
        assert!(lines.iter().any(|l| l.contains("atlas-setup.log")));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("run Atlas-Setup.exe again"))
        );
        assert!(
            lines.last().unwrap().ends_with("open an issue."),
            "the next step is whole: {lines:#?}"
        );
    }

    #[test]
    fn the_window_adds_a_shadow_margin_around_an_opaque_card() {
        let fonts = Fonts::bundled();
        let s = snap(Phase::Working, 0.5);
        let pm = paint_window(&frame(&s, &fonts, 3.0, 1.0));
        assert_eq!(
            (pm.width(), pm.height()),
            (
                ((WIDTH + 2.0 * SHADOW) as u32),
                ((HEIGHT + 2.0 * SHADOW) as u32)
            )
        );
        let below = px(&pm, WIDTH / 2.0 + SHADOW, HEIGHT + SHADOW + 4.0, 1.0);
        assert!(
            below.3 > 0 && below.3 < 120,
            "soft shadow under the card: {below:?}"
        );
        assert_eq!(px(&pm, 1.0, 1.0, 1.0).3, 0, "far corner stays clear");
        assert_eq!(
            px(&pm, WIDTH / 2.0 + SHADOW, HEIGHT - 40.0 + SHADOW, 1.0).3,
            255
        );
    }

    /// Design review aid: `ATLAS_SETUP_DUMP=<dir> cargo test -p atlas-setup dump_frames` writes one raw RGBA
    /// file per screen (`<name>_<w>x<h>.rgba`); without the variable it does nothing.
    #[test]
    fn dump_frames() {
        let Some(dir) = std::env::var_os("ATLAS_SETUP_DUMP") else {
            return;
        };
        let fonts = match std::env::var_os("WINDIR") {
            Some(w) => Fonts::from_dir(&std::path::Path::new(&w).join("Fonts")),
            None => Fonts::bundled(),
        };
        let mut failed = snap(Phase::Failed, 0.4);
        failed.status = "Atlas could not be installed".into();
        failed.details = format!(
            "The Atlas installer failed (exit code 2).\nLog: C:\\Users\\ana\\AppData\\Local\\Temp\\atlas-setup.log\n{}",
            crate::install::NEXT_STEP
        );
        let mut ask = snap(Phase::AskData, 0.0);
        ask.mode = Mode::Uninstall;
        let mut confirm = snap(Phase::ConfirmRemove, 0.0);
        confirm.mode = Mode::Uninstall;
        let mut done = snap(Phase::Done, 1.0);
        done.status = "Atlas is ready".into();
        done.note = "Opening Atlas…".into();
        for (name, s, t, scale) in [
            ("early", snap(Phase::Working, 0.05), 0.5, 1.0),
            ("working", snap(Phase::Working, 0.55), 4.0, 1.0),
            ("working2x", snap(Phase::Working, 0.55), 4.0, 2.0),
            ("done", done, 6.0, 1.0),
            ("failed", failed, 6.0, 1.0),
            ("ask", ask, 6.0, 1.0),
            ("confirm", confirm, 6.0, 1.0),
        ] {
            let pm = paint_window(&frame(&s, &fonts, t, scale));
            let name = format!("{name}_{}x{}.rgba", pm.width(), pm.height());
            std::fs::write(std::path::Path::new(&dir).join(name), pm.data()).unwrap();
        }
    }

    #[test]
    fn bgra_swaps_red_and_blue_keeping_alpha() {
        let mut pm = Pixmap::new(1, 1).unwrap();
        pm.pixels_mut()[0] = tiny_skia::PremultipliedColorU8::from_rgba(10, 20, 30, 255).unwrap();
        assert_eq!(to_bgra(&pm), [30, 20, 10, 255]);
    }
}
