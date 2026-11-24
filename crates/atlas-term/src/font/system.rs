//! System font discovery for the terminal's text faces. The font directories are scanned
//! once (lazily); only the `name`/`OS/2`/`fvar`/`avar` tables of each file are read (the
//! macOS font folders hold hundreds of MB, one file is 190 MB of emoji). A chosen face's
//! file is then read whole into an `Arc` shared per file (weakly cached, so it is freed
//! when the terminal switches family).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, Weak};

use swash::{Attributes, FontRef, NormalizedCoord, StringId, Style, tag_from_bytes};

use crate::vt::{BOLD, ITALIC};

/// Font files bigger than this are never loaded as a text face (CJK/emoji collections).
const MAX_FILE: u64 = 48 << 20;
/// Tables copied into the scan stub.
const SCAN_TABLES: [&[u8; 4]; 4] = [b"name", b"OS/2", b"fvar", b"avar"];

/// A text face of the terminal: shared font data + the variation coordinates that select
/// its style. The bytes are an `Arc` shared by every face of the same file; they are freed
/// once no `Font` uses them any more.
#[derive(Clone)]
pub struct TextFace {
    data: Arc<[u8]>,
    /// Offset of the face's table directory in `data` (`.ttc` collections).
    offset: u32,
    /// Normalized variation coordinates (empty for static fonts).
    pub coords: Vec<NormalizedCoord>,
}

impl TextFace {
    pub fn font(&self) -> FontRef<'_> {
        FontRef::from_offset(&self.data, self.offset).expect("validated when loaded")
    }
}

/// The four faces (`BOLD | ITALIC` bits index them) of a family, plus its name.
pub struct Family {
    pub name: String,
    pub faces: [TextFace; 4],
}

/// One discovered font file (face `index` of the file).
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub index: u32,
    /// Family names (name ids 16 and 1, lowercase).
    pub families: Vec<String>,
    /// The display form of the first family name.
    pub display: String,
    pub weight: u16,
    pub italic: bool,
    /// `wght` axis range, when the font is variable in weight.
    pub wght: Option<(f32, f32)>,
    /// `ital` / `slnt` axis present.
    pub ital_axis: bool,
    pub slnt_axis: bool,
}

/// What the requested families resolved to.
pub struct Resolved {
    /// The candidate that resolved to an installed font (`None` = the bundled font).
    pub family: Option<Family>,
    /// Index into the request of the candidate that matched.
    pub matched: Option<usize>,
}

static SCAN: LazyLock<Vec<Entry>> = LazyLock::new(|| scan(&font_dirs(), FONT_DEPTH));

/// How deep below each font directory the scan looks. macOS lists its folders one by one; Linux packages put fonts in
/// `/usr/share/fonts/<package>/…`, sometimes a level further (`opentype/<family>/`).
const FONT_DEPTH: u32 = if cfg!(target_os = "macos") { 0 } else { 3 };

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        v.extend(home.map(|h| h.join("Library/Fonts")));
        v.extend(
            [
                "/Library/Fonts",
                "/System/Library/Fonts",
                "/System/Library/Fonts/Supplemental",
            ]
            .map(PathBuf::from),
        );
    } else {
        // fontconfig's own default roots, user fonts first so they win a name clash.
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
        v.extend(data.map(|d| d.join("fonts")));
        v.extend(home.map(|h| h.join(".fonts")));
        v.extend(["/usr/local/share/fonts", "/usr/share/fonts"].map(PathBuf::from));
    }
    v
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Read the sfnt table directory at `base` and build a stub font with only the scan
/// tables (zero-padded to 4 bytes), valid for `FontRef::from_index(stub, 0)`.
fn stub_font(f: &mut File, base: u64, len: u64) -> Option<Vec<u8>> {
    let mut head = [0u8; 12];
    f.seek(SeekFrom::Start(base)).ok()?;
    f.read_exact(&mut head).ok()?;
    let sfnt = be32(&head, 0)?;
    if ![
        0x0001_0000,
        tag_from_bytes(b"OTTO"),
        tag_from_bytes(b"true"),
    ]
    .contains(&sfnt)
    {
        return None;
    }
    let n = u16::from_be_bytes([head[4], head[5]]) as usize;
    let mut dir = vec![0u8; n * 16];
    f.read_exact(&mut dir).ok()?;
    let mut found: Vec<(u32, Vec<u8>)> = Vec::new();
    for rec in dir.as_chunks::<16>().0 {
        let tag = be32(rec, 0)?;
        if !SCAN_TABLES.iter().any(|t| tag_from_bytes(t) == tag) {
            continue;
        }
        let (off, size) = (be32(rec, 8)? as u64, be32(rec, 12)? as u64);
        if off.checked_add(size)? > len || size > (4 << 20) {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        f.seek(SeekFrom::Start(off)).ok()?;
        f.read_exact(&mut data).ok()?;
        found.push((tag, data));
    }
    found.sort_by_key(|(t, _)| *t);
    let mut out = Vec::new();
    out.extend_from_slice(&sfnt.to_be_bytes());
    out.extend_from_slice(&(found.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0u8; 6]); // searchRange / entrySelector / rangeShift: unused
    let mut off = 12 + found.len() * 16;
    for (tag, data) in &found {
        out.extend_from_slice(&tag.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum: unchecked
        out.extend_from_slice(&(off as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        off += data.len().next_multiple_of(4);
    }
    for (_, data) in &found {
        out.extend_from_slice(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    Some(out)
}

/// Metadata of every face in `path` (a font or a `.ttc` collection).
fn scan_file(path: &Path) -> Vec<Entry> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = f.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    if len > MAX_FILE {
        return Vec::new();
    }
    let mut head = [0u8; 16];
    if f.read_exact(&mut head).is_err() {
        return Vec::new();
    }
    let offsets: Vec<u64> = if be32(&head, 0) == Some(tag_from_bytes(b"ttcf")) {
        let n = be32(&head, 8).unwrap_or(0).min(64) as usize;
        let mut offs = vec![0u8; n * 4];
        if f.seek(SeekFrom::Start(12)).is_err() || f.read_exact(&mut offs).is_err() {
            return Vec::new();
        }
        offs.as_chunks::<4>()
            .0
            .iter()
            .filter_map(|c| be32(c, 0).map(u64::from))
            .collect()
    } else {
        vec![0]
    };
    offsets
        .into_iter()
        .enumerate()
        .filter_map(|(i, base)| {
            let stub = stub_font(&mut f, base, len)?;
            entry(path, i as u32, &stub)
        })
        .collect()
}

/// Family names, style and axes of the font in `data` (a scan stub or a whole file).
fn entry(path: &Path, index: u32, data: &[u8]) -> Option<Entry> {
    let font = FontRef::from_index(data, 0)?;
    let names = font.localized_strings();
    let mut families: Vec<String> = Vec::new();
    let mut display = String::new();
    for id in [StringId::TypographicFamily, StringId::Family] {
        // Every platform/language record of the id: any of them may be what the user typed.
        for s in names.filter(|s| s.id() == id && s.is_decodable()) {
            let name = s.to_string();
            let key = name.trim().to_lowercase();
            if key.is_empty() || families.contains(&key) {
                continue;
            }
            if display.is_empty() {
                display = name.trim().to_string();
            }
            families.push(key);
        }
    }
    if families.is_empty() {
        return None;
    }
    let attrs: Attributes = font.attributes();
    let axis = |tag: &[u8; 4]| font.variations().find_by_tag(tag_from_bytes(tag));
    let wght = axis(b"wght").map(|v| (v.min_value(), v.max_value()));
    Some(Entry {
        path: path.to_path_buf(),
        index,
        families,
        display,
        weight: attrs.weight().0,
        italic: !matches!(attrs.style(), Style::Normal),
        wght,
        ital_axis: axis(b"ital").is_some(),
        slnt_axis: axis(b"slnt").is_some(),
    })
}

/// Every font file under `dir`, `depth` levels of subdirectories deep (0 = `dir` alone), each directory's files sorted.
fn font_files(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    let mut files: Vec<&PathBuf> = paths
        .iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc"))
        })
        .collect();
    files.sort();
    out.extend(files.into_iter().cloned());
    if depth > 0 {
        let mut subs: Vec<&PathBuf> = paths.iter().filter(|p| p.is_dir()).collect();
        subs.sort();
        for s in subs {
            font_files(s, depth - 1, out);
        }
    }
}

fn scan(dirs: &[PathBuf], depth: u32) -> Vec<Entry> {
    let mut files = Vec::new();
    for dir in dirs {
        font_files(dir, depth, &mut files);
    }
    files.iter().flat_map(|p| scan_file(p)).collect()
}

/// Entries whose family matches `name` (case-insensitive, whitespace-trimmed).
pub fn matching<'a>(entries: &'a [Entry], name: &str) -> Vec<&'a Entry> {
    let key = name.trim().to_lowercase();
    entries
        .iter()
        .filter(|e| e.families.contains(&key))
        .collect()
}

/// `(tag, value)` variation settings applied to a picked face.
pub type Vars = Vec<(&'static str, f32)>;

/// Which of `entries` (one family) best serves `style` (`BOLD | ITALIC` bits), and the
/// `(tag, value)` variation settings to apply to it. Static files are preferred when their
/// style matches; variable fonts are driven by `wght` (400 / 700) and `ital`/`slnt`.
pub fn pick<'a>(entries: &[&'a Entry], style: u8) -> Option<(&'a Entry, Vars)> {
    let want_bold = style & BOLD != 0;
    let want_italic = style & ITALIC != 0;
    let target = if want_bold { 700 } else { 400 };
    let mut best: Option<(i32, &Entry, Vars)> = None;
    for &e in entries {
        let mut vars = Vec::new();
        let mut score = 0i32;
        // Weight.
        match e.wght {
            Some((lo, hi)) => {
                vars.push(("wght", (target as f32).clamp(lo, hi)));
                // Variable in weight: reaches the target exactly (within its range).
                score += 10;
            }
            None => {
                score -= (e.weight as i32 - target).abs() / 10;
                if e.weight as i32 == target {
                    score += 20;
                }
            }
        }
        // Slant.
        if want_italic {
            if e.italic {
                score += 30;
            } else if e.ital_axis {
                vars.push(("ital", 1.0));
                score += 25;
            } else if e.slnt_axis {
                vars.push(("slnt", -12.0));
                score += 20;
            } else {
                score -= 30;
            }
        } else if e.italic {
            score -= 30;
        }
        if best.as_ref().is_none_or(|(s, ..)| score > *s) {
            best = Some((score, e, vars));
        }
    }
    best.map(|(_, e, v)| (e, v))
}

/// Files currently held by a `Font`: one shared copy per path while any face uses it.
static LOADED: LazyLock<Mutex<HashMap<PathBuf, Weak<[u8]>>>> = LazyLock::new(Default::default);

/// The whole file, read once and shared while in use.
fn load(path: &Path) -> Option<Arc<[u8]>> {
    let mut g = LOADED.lock().ok()?;
    g.retain(|_, w| w.strong_count() > 0);
    if let Some(d) = g.get(path).and_then(Weak::upgrade) {
        return Some(d);
    }
    if std::fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    let data: Arc<[u8]> = std::fs::read(path).ok()?.into();
    g.insert(path.to_path_buf(), Arc::downgrade(&data));
    Some(data)
}

/// Resolve `entries`' family `name` into four faces, or `None` when a face cannot be loaded.
pub fn build_family(entries: &[Entry], name: &str) -> Option<Family> {
    let matches = matching(entries, name);
    let first = matches.first()?;
    let mut faces: Vec<TextFace> = Vec::with_capacity(4);
    for style in 0..4u8 {
        let (e, vars) = pick(&matches, style)?;
        let data = load(&e.path)?;
        let offset = FontRef::from_index(&data, e.index as usize)?.offset;
        let coords = if vars.is_empty() {
            Vec::new()
        } else {
            FontRef::from_offset(&data, offset)?
                .variations()
                .normalized_coords(vars.iter().copied())
                .collect()
        };
        faces.push(TextFace {
            data,
            offset,
            coords,
        });
    }
    Some(Family {
        name: first.display.clone(),
        faces: faces.try_into().ok()?,
    })
}

/// Resolve the first of `names` that is an installed font family.
pub fn resolve(names: &[String]) -> Resolved {
    resolve_in(&SCAN, names)
}

fn resolve_in(entries: &[Entry], names: &[String]) -> Resolved {
    for (i, n) in names.iter().enumerate() {
        if let Some(family) = build_family(entries, n) {
            return Resolved {
                family: Some(family),
                matched: Some(i),
            };
        }
    }
    Resolved {
        family: None,
        matched: None,
    }
}

#[cfg(test)]
mod tests;
