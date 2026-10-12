//! Inline images: the Kitty graphics protocol (APC `G`) and sixel (DCS `q`).
//! Decoding and the image/placement store live here; `vt` anchors placements to
//! absolute grid lines and the renderer draws them as textured quads.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::Arc;

use base64::Engine;

/// Decoded RGBA8 pixels. `gen` changes when the pixels under an id are replaced.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Arc<Vec<u8>>,
    pub generation: u64,
}

/// An image shown on the grid. `line` counts from the top of the scrollback.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub image: u32,
    pub id: u32,
    pub line: i64,
    pub col: u16,
    /// Cells covered (cursor movement, scroll-off).
    pub cols: u16,
    pub rows: u16,
    /// Drawn size in cells (fractional: natural-size images keep their pixel size).
    pub size: (f32, f32),
    /// Source rectangle in image pixels.
    pub src: [u32; 4],
    pub z: i32,
    /// Placed on the alternate screen (dropped when it exits).
    pub alt: bool,
}

/// Total decoded bytes kept (Kitty's default quota).
const QUOTA: usize = 320 << 20;
/// Largest single image accepted.
const MAX_SIDE: u32 = 10_000;
const MAX_BYTES: usize = 100 << 20;

#[derive(Default)]
pub struct Store {
    pub images: HashMap<u32, Image>,
    pub placements: Vec<Placement>,
    /// Insertion order for eviction.
    order: Vec<u32>,
    bytes: usize,
    next_generation: u64,
    /// Ids handed out to images transmitted without one (sixel, `I=`).
    next_auto: u32,
    /// Chunked Kitty transmission in progress (`m=1`).
    pending: Option<(Cmd, Vec<u8>)>,
    /// `I=` image number → newest id transmitted with it.
    numbers: HashMap<u32, u32>,
    /// Sixel images: no client id, freed once no placement shows them.
    transient: HashSet<u32>,
}

/// Where and how big the cursor-relative placement is, so `vt` can move the cursor.
pub struct Placed {
    pub cols: u16,
    pub rows: u16,
    /// Kitty `C=1`: leave the cursor alone.
    pub keep_cursor: bool,
}

pub struct Outcome {
    /// Reply for the pty (`ESC _ G … ESC \`).
    pub reply: Option<String>,
    /// A new placement at the cursor; `vt` fills in line/col and moves the cursor.
    pub place: Option<(Placement, Placed)>,
}

#[derive(Clone, Debug, Default)]
struct Cmd {
    action: u8,
    format: u32,
    medium: u8,
    width: u32,
    height: u32,
    id: u32,
    number: u32,
    placement: u32,
    more: bool,
    compressed: bool,
    quiet: u8,
    src: [u32; 4],
    cols: u16,
    rows: u16,
    keep_cursor: bool,
    z: i32,
    delete: u8,
}

impl Cmd {
    fn parse(keys: &str) -> Self {
        let mut c = Cmd {
            action: b't',
            format: 32,
            medium: b'd',
            delete: b'a',
            ..Cmd::default()
        };
        for kv in keys.split(',') {
            let Some((k, v)) = kv.split_once('=') else {
                continue;
            };
            let n = || v.parse::<u32>().unwrap_or(0);
            let ch = v.bytes().next().unwrap_or(0);
            match k {
                "a" => c.action = ch,
                "f" => c.format = n(),
                "t" => c.medium = ch,
                "s" => c.width = n(),
                "v" => c.height = n(),
                "i" => c.id = n(),
                "I" => c.number = n(),
                "p" => c.placement = n(),
                "m" => c.more = v == "1",
                "o" => c.compressed = v == "z",
                "q" => c.quiet = n() as u8,
                "x" => c.src[0] = n(),
                "y" => c.src[1] = n(),
                "w" => c.src[2] = n(),
                "h" => c.src[3] = n(),
                "c" => c.cols = n().min(u16::MAX as u32) as u16,
                "r" => c.rows = n().min(u16::MAX as u32) as u16,
                "C" => c.keep_cursor = v == "1",
                "z" => c.z = v.parse().unwrap_or(0),
                "d" => c.delete = ch,
                _ => {}
            }
        }
        c
    }

    fn reply(&self, msg: &str) -> Option<String> {
        let ok = msg == "OK";
        if (self.id == 0 && self.number == 0) || self.quiet >= 2 || (ok && self.quiet == 1) {
            return None;
        }
        let mut s = String::from("\x1b_G");
        if self.id != 0 {
            s.push_str(&format!("i={}", self.id));
        }
        if self.number != 0 {
            s.push_str(&format!(
                "{}I={}",
                if self.id != 0 { "," } else { "" },
                self.number
            ));
        }
        if self.placement != 0 {
            s.push_str(&format!(",p={}", self.placement));
        }
        s.push_str(&format!(";{msg}\x1b\\"));
        Some(s)
    }
}

impl Store {
    fn auto_id(&mut self) -> u32 {
        // High ids, away from the small ones programs pick.
        self.next_auto = self.next_auto.wrapping_add(1).max(1);
        u32::MAX - self.next_auto
    }

    pub fn insert(&mut self, id: u32, w: u32, h: u32, rgba: Vec<u8>) {
        self.remove_image(id);
        self.next_generation += 1;
        self.bytes += rgba.len();
        self.order.push(id);
        self.images.insert(
            id,
            Image {
                w,
                h,
                rgba: Arc::new(rgba),
                generation: self.next_generation,
            },
        );
        // Evict the oldest images (and their placements) over the quota.
        while self.bytes > QUOTA && self.order.len() > 1 {
            let old = self.order[0];
            self.remove_image(old);
            self.placements.retain(|p| p.image != old);
        }
    }

    fn remove_image(&mut self, id: u32) {
        if let Some(img) = self.images.remove(&id) {
            self.bytes -= img.rgba.len();
            self.order.retain(|&i| i != id);
            self.transient.remove(&id);
        }
    }

    /// Drop placements made on the alternate screen (it was exited).
    pub fn clear_alt(&mut self) {
        self.placements.retain(|p| !p.alt);
        self.gc();
    }

    /// Images without placements and without an explicit client id are unreachable.
    fn gc(&mut self) {
        let used: HashSet<u32> = self.placements.iter().map(|p| p.image).collect();
        let dead: Vec<u32> = self
            .images
            .keys()
            .copied()
            .filter(|id| self.transient.contains(id) && !used.contains(id))
            .collect();
        for id in dead {
            self.remove_image(id);
        }
    }

    /// One APC `G` payload (`keys;base64data`).
    pub fn kitty(&mut self, payload: &[u8]) -> Outcome {
        let none = Outcome {
            reply: None,
            place: None,
        };
        let s = String::from_utf8_lossy(payload);
        let (keys, data) = s.split_once(';').unwrap_or((&s, ""));
        let mut cmd = Cmd::parse(keys);
        // Continuation chunk: keys come from the first chunk.
        if let Some((first, mut buf)) = self.pending.take() {
            buf.extend_from_slice(data.as_bytes());
            if cmd.more {
                if buf.len() > MAX_BYTES * 2 {
                    return Outcome {
                        reply: first.reply("EFBIG:too large"),
                        place: None,
                    };
                }
                self.pending = Some((first, buf));
                return none;
            }
            let quiet = cmd.quiet.max(first.quiet);
            cmd = first;
            cmd.quiet = quiet;
            return self.run(cmd, &buf);
        }
        if cmd.more {
            self.pending = Some((cmd, data.as_bytes().to_vec()));
            return none;
        }
        self.run(cmd, data.as_bytes())
    }

    fn run(&mut self, mut cmd: Cmd, data: &[u8]) -> Outcome {
        let err = |cmd: &Cmd, m: &str| Outcome {
            reply: cmd.reply(m),
            place: None,
        };
        match cmd.action {
            b'd' => {
                self.delete(&cmd);
                Outcome {
                    reply: None,
                    place: None,
                }
            }
            b'p' => {
                let id = self.resolve(&cmd);
                if !self.images.contains_key(&id) {
                    return err(&cmd, "ENOENT:no such image");
                }
                cmd.id = id;
                self.place(cmd)
            }
            b't' | b'T' | b'q' => {
                let (w, h, rgba) = match decode(&cmd, data) {
                    Ok(v) => v,
                    Err(e) => return err(&cmd, &e),
                };
                if cmd.action == b'q' {
                    return err(&cmd, "OK");
                }
                if cmd.id == 0 {
                    cmd.id = self.auto_id();
                    if cmd.number != 0 {
                        self.numbers.insert(cmd.number, cmd.id);
                    }
                }
                self.insert(cmd.id, w, h, rgba);
                if cmd.action == b'T' {
                    self.place(cmd)
                } else {
                    err(&cmd, "OK")
                }
            }
            _ => err(&cmd, "EINVAL:unknown action"),
        }
    }

    /// `I=` (image number) refers to the newest image transmitted with it.
    fn resolve(&self, cmd: &Cmd) -> u32 {
        match (cmd.id, cmd.number) {
            (0, n) if n != 0 => self.numbers.get(&n).copied().unwrap_or(0),
            (id, _) => id,
        }
    }

    fn place(&mut self, cmd: Cmd) -> Outcome {
        let img = &self.images[&cmd.id];
        let [x, y, w, h] = cmd.src;
        let x = x.min(img.w);
        let y = y.min(img.h);
        let w = if w == 0 { img.w - x } else { w.min(img.w - x) };
        let h = if h == 0 { img.h - y } else { h.min(img.h - y) };
        let p = Placement {
            image: cmd.id,
            id: cmd.placement,
            line: 0,
            col: 0,
            cols: cmd.cols,
            rows: cmd.rows,
            src: [x, y, w, h],
            size: (0.0, 0.0),
            z: cmd.z,
            alt: false,
        };
        Outcome {
            reply: cmd.reply("OK"),
            place: Some((
                p,
                Placed {
                    cols: cmd.cols,
                    rows: cmd.rows,
                    keep_cursor: cmd.keep_cursor,
                },
            )),
        }
    }

    fn delete(&mut self, cmd: &Cmd) {
        let free = cmd.delete.is_ascii_uppercase();
        match cmd.delete.to_ascii_lowercase() {
            b'i' => {
                self.placements
                    .retain(|p| p.image != cmd.id || (cmd.placement != 0 && p.id != cmd.placement));
                if free {
                    self.remove_image(cmd.id);
                }
            }
            // `a` (default): every placement.
            _ => {
                self.placements.clear();
                if free {
                    let ids: Vec<u32> = self.images.keys().copied().collect();
                    for id in ids {
                        self.remove_image(id);
                    }
                }
            }
        }
        self.gc();
    }

    /// A decoded sixel image placed at the cursor (`vt` anchors it).
    pub fn sixel(&mut self, data: &[u8]) -> Option<(Placement, Placed)> {
        let (w, h, rgba) = decode_sixel(data)?;
        let id = self.auto_id();
        self.insert(id, w, h, rgba);
        self.transient.insert(id);
        Some((
            Placement {
                image: id,
                id: 0,
                line: 0,
                col: 0,
                cols: 0,
                rows: 0,
                src: [0, 0, w, h],
                size: (0.0, 0.0),
                z: 0,
                alt: false,
            },
            Placed {
                cols: 0,
                rows: 0,
                keep_cursor: false,
            },
        ))
    }
}

/// Transmission medium + format → RGBA8.
fn decode(cmd: &Cmd, data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let raw = b64
        .decode(
            data.iter()
                .copied()
                .filter(|b| !b.is_ascii_whitespace())
                .collect::<Vec<_>>(),
        )
        .map_err(|_| "EINVAL:bad base64".to_string())?;
    let mut bytes = match cmd.medium {
        b'd' => raw,
        b'f' | b't' => {
            let path = String::from_utf8(raw).map_err(|_| "EINVAL:bad path".to_string())?;
            read_file(&path, cmd.medium == b't')?
        }
        _ => return Err("EINVAL:unsupported transmission medium".into()),
    };
    if cmd.compressed {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(&bytes[..])
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|_| "EINVAL:bad zlib data".to_string())?;
        bytes = out;
    }
    let (w, h) = (cmd.width, cmd.height);
    let rgba = match cmd.format {
        100 => return decode_png(&bytes),
        32 | 24 => {
            if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
                return Err("EINVAL:bad size".into());
            }
            let bpp = (cmd.format / 8) as usize;
            let need = w as usize * h as usize * bpp;
            if bytes.len() < need {
                return Err("ENODATA:insufficient image data".into());
            }
            if bpp == 4 {
                bytes.truncate(need);
                bytes
            } else {
                rgb_to_rgba(&bytes[..need])
            }
        }
        _ => return Err("EINVAL:unsupported format".into()),
    };
    Ok((w, h, rgba))
}

/// `t=f` / `t=t`: a regular file; temp files (`t=t`) are deleted after reading.
fn read_file(path: &str, temp: bool) -> Result<Vec<u8>, String> {
    let p = std::path::Path::new(path);
    let meta = std::fs::metadata(p).map_err(|_| "ENOENT:no such file".to_string())?;
    if !meta.is_file() || meta.len() > MAX_BYTES as u64 {
        return Err("EINVAL:not a regular file".into());
    }
    let bytes = std::fs::read(p).map_err(|_| "EBADF:unreadable".to_string())?;
    // Only delete what the protocol says is a temp file.
    if temp && path.contains("tty-graphics-protocol") {
        let _ = std::fs::remove_file(p);
    }
    Ok(bytes)
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let bad = |_| "EBADPNG:bad png".to_string();
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(bad)?;
    let (w, h) = (r.info().width, r.info().height);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err("EINVAL:bad size".into());
    }
    let mut buf = vec![0; r.output_buffer_size().ok_or("EINVAL:too large")?];
    let info = r.next_frame(&mut buf).map_err(bad)?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => rgb_to_rgba(&buf),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("EBADPNG:unexpanded palette".into()),
    };
    Ok((w, h, rgba))
}

fn rgb_to_rgba(b: &[u8]) -> Vec<u8> {
    b.as_chunks::<3>()
        .0
        .iter()
        .flat_map(|&[r, g, b]| [r, g, b, 255])
        .collect()
}

/// VT340 default sixel palette (percent RGB).
const VT340: [[u8; 3]; 16] = [
    [0, 0, 0],
    [20, 20, 80],
    [80, 13, 13],
    [20, 80, 20],
    [80, 20, 80],
    [20, 80, 80],
    [80, 80, 20],
    [53, 53, 53],
    [26, 26, 26],
    [33, 33, 60],
    [60, 26, 26],
    [33, 60, 33],
    [60, 33, 60],
    [33, 60, 60],
    [60, 60, 33],
    [80, 80, 80],
];

fn pct(v: u32) -> u8 {
    (v.min(100) * 255 / 100) as u8
}

/// HLS (sixel's hue 0 = blue) → RGB, components in percent / degrees.
fn hls(h: u32, l: u32, s: u32) -> [u8; 3] {
    let (h, l, s) = (
        ((h + 240) % 360) as f32,
        l.min(100) as f32 / 100.0,
        s.min(100) as f32 / 100.0,
    );
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let f = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [f(r), f(g), f(b)]
}

/// Sixel data after the DCS intro (`P1;P2;P3 q …`) → RGBA8. Unset pixels are transparent.
pub fn decode_sixel(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let q = data.iter().position(|&b| b == b'q')?;
    let body = &data[q + 1..];
    let mut pal: Vec<[u8; 3]> = VT340
        .iter()
        .map(|c| [pct(c[0] as u32), pct(c[1] as u32), pct(c[2] as u32)])
        .collect();
    pal.resize(256, [255, 255, 255]);
    // Sparse writes into a growing canvas.
    let (mut w, mut h) = (0u32, 0u32);
    let mut px: Vec<u8> = Vec::new();
    let mut stride = 0u32;
    let grow = |px: &mut Vec<u8>,
                stride: &mut u32,
                need_w: u32,
                need_h: u32,
                w: &mut u32,
                h: &mut u32|
     -> bool {
        if need_w > MAX_SIDE || need_h > MAX_SIDE {
            return false;
        }
        if need_w > *stride {
            let ns = need_w.next_power_of_two().clamp(64, MAX_SIDE);
            let rows = px.len() / (*stride as usize * 4).max(1);
            let mut n = vec![0u8; ns as usize * 4 * rows];
            for r in 0..rows {
                let src = r * *stride as usize * 4;
                n[r * ns as usize * 4..][..*stride as usize * 4]
                    .copy_from_slice(&px[src..src + *stride as usize * 4]);
            }
            *px = n;
            *stride = ns;
        }
        let rows = px.len() / (*stride as usize * 4);
        if need_h as usize > rows {
            px.resize(need_h as usize * *stride as usize * 4, 0);
        }
        *w = (*w).max(need_w);
        *h = (*h).max(need_h);
        true
    };
    let (mut x, mut y) = (0u32, 0u32);
    let mut color = 0usize;
    let mut i = 0;
    let num = |i: &mut usize| -> Vec<u32> {
        let mut out = vec![0u32];
        while *i < body.len() {
            match body[*i] {
                d @ b'0'..=b'9' => {
                    let v = out.last_mut().unwrap();
                    *v = v.saturating_mul(10).saturating_add((d - b'0') as u32);
                }
                b';' => out.push(0),
                _ => break,
            }
            *i += 1;
        }
        out
    };
    while i < body.len() {
        let b = body[i];
        i += 1;
        let mut repeat = 1;
        let sixel = match b {
            b'"' => {
                let a = num(&mut i);
                // Raster attributes: preallocate the declared size.
                if let (Some(&pw), Some(&ph)) = (a.get(2), a.get(3))
                    && pw > 0
                    && ph > 0
                    && !grow(&mut px, &mut stride, pw, ph, &mut w, &mut h)
                {
                    return None;
                }
                continue;
            }
            b'#' => {
                let a = num(&mut i);
                color = a[0] as usize % 256;
                if a.len() >= 5 {
                    pal[color] = match a[1] {
                        1 => hls(a[2], a[3], a[4]),
                        _ => [pct(a[2]), pct(a[3]), pct(a[4])],
                    };
                }
                continue;
            }
            b'!' => {
                repeat = num(&mut i)[0].clamp(1, MAX_SIDE);
                let Some(&c) = body.get(i) else { break };
                i += 1;
                c
            }
            b'$' => {
                x = 0;
                continue;
            }
            b'-' => {
                x = 0;
                y += 6;
                continue;
            }
            c => c,
        };
        if !(0x3f..=0x7e).contains(&sixel) {
            continue;
        }
        let bits = sixel - 0x3f;
        if !grow(&mut px, &mut stride, x + repeat, y + 6, &mut w, &mut h) {
            return None;
        }
        let [r, g, bl] = pal[color];
        for dy in 0..6 {
            if bits & (1 << dy) == 0 {
                continue;
            }
            let row = (y + dy) as usize * stride as usize * 4;
            for dx in 0..repeat {
                let o = row + (x + dx) as usize * 4;
                px[o..o + 4].copy_from_slice(&[r, g, bl, 255]);
            }
        }
        x += repeat;
    }
    if w == 0 || h == 0 {
        return None;
    }
    // Crop the stride padding.
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for r in 0..h as usize {
        let s = r * stride as usize * 4;
        out.extend_from_slice(&px[s..s + w as usize * 4]);
    }
    Some((w, h, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(b: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b)
    }

    #[test]
    fn kitty_rgb_chunked_transmit_and_display() {
        let mut s = Store::default();
        let px = b64(&[255, 0, 0, 0, 255, 0]);
        let (a, b) = px.split_at(4);
        let o = s.kitty(format!("a=T,f=24,s=2,v=1,i=7,m=1;{a}").as_bytes());
        assert!(o.reply.is_none() && o.place.is_none());
        let o = s.kitty(format!("m=0;{b}").as_bytes());
        assert_eq!(o.reply.as_deref(), Some("\x1b_Gi=7;OK\x1b\\"));
        let (p, _) = o.place.unwrap();
        assert_eq!((p.image, p.src), (7, [0, 0, 2, 1]));
        assert_eq!(*s.images[&7].rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
    }

    #[test]
    fn kitty_errors_and_quiet() {
        let mut s = Store::default();
        let o = s.kitty(b"a=p,i=3");
        assert_eq!(
            o.reply.as_deref(),
            Some("\x1b_Gi=3;ENOENT:no such image\x1b\\")
        );
        assert!(s.kitty(b"a=p,i=3,q=2").reply.is_none());
        // Short data, query action: error reply, nothing stored.
        let o = s.kitty(format!("a=q,f=32,s=2,v=2,i=1;{}", b64(&[0; 4])).as_bytes());
        assert!(o.reply.unwrap().contains("ENODATA"));
        assert!(s.images.is_empty());
        // No id: no reply at all.
        assert!(s.kitty(b"a=p").reply.is_none());
    }

    #[test]
    fn kitty_png_and_delete() {
        let mut png_bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png_bytes, 1, 1);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[1, 2, 3])
                .unwrap();
        }
        let mut s = Store::default();
        let o = s.kitty(format!("a=T,f=100,i=9,q=1;{}", b64(&png_bytes)).as_bytes());
        assert!(o.reply.is_none());
        let (p, _) = o.place.unwrap();
        s.placements.push(p);
        assert_eq!(*s.images[&9].rgba, [1, 2, 3, 255]);
        s.kitty(b"a=d,d=I,i=9");
        assert!(s.placements.is_empty() && s.images.is_empty());
    }

    #[test]
    fn sixel_colors_repeat_and_bands() {
        // Color 1 = pure red (RGB %), 3 columns of the top pixel, next band one full column.
        let (w, h, px) = decode_sixel(b"0;1;0q#1;2;100;0;0#1!3@-~").unwrap();
        assert_eq!((w, h), (3, 12));
        assert_eq!(&px[..4], &[255, 0, 0, 255]);
        assert_eq!(&px[8..12], &[255, 0, 0, 255]);
        // Row 1 (bit 1) is unset: transparent.
        assert_eq!(&px[12..16], &[0, 0, 0, 0]);
        // Second band: rows 6..12 of column 0 are set.
        assert_eq!(px[(6 * 3) * 4 + 3], 255);
        assert_eq!(px[(11 * 3) * 4 + 3], 255);
        assert!(decode_sixel(b"q").is_none());
    }

    #[test]
    fn hls_primaries() {
        // Sixel hue 120 = red, 240 = green, 0 = blue.
        assert_eq!(hls(120, 50, 100), [255, 0, 0]);
        assert_eq!(hls(240, 50, 100), [0, 255, 0]);
        assert_eq!(hls(0, 50, 100), [0, 0, 255]);
    }
}
