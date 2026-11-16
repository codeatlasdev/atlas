//! The `locate` tool of a crewmate: given a screenshot (a path inside its work tree, where image
//! attachments are staged: `.atlas/attachments/<task>/<name>`), optionally a region of it and the
//! URL it was taken from, say which route file, component and selector draw that region.
//!
//! | file       | what it owns                                                                  |
//! |------------|-------------------------------------------------------------------------------|
//! | `route.rs` | URL path -> route files (SvelteKit, Next app and pages, Flutter `GoRoute`)    |
//! | `rank.rs`  | the page's element boxes and the ranking of the ones a region points at       |
//! | this file  | the request, the capture with `dom: true`, mapping an element to source files |
//!
//! Nothing here guesses from pixels: the boxes come from the real page (`runner.mjs` with
//! `dom: true`), and the source files from matching the URL and the element's own `data-testid` /
//! text against the work tree.

pub mod rank;
pub mod route;

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::capture::{self, Spec};
use super::spawn::layout;
use rank::{DomReport, Rect};

/// The widths an image is assumed to have been taken at when its own width says so; any other
/// image width falls back to the desktop one.
const KNOWN_WIDTHS: [u32; 2] = [390, 1440];
const DEFAULT_WIDTH: u32 = 1440;
/// Candidates returned, at most.
const MAX_CANDIDATES: usize = 5;
/// Files read while looking for a testid or text in the work tree, and the size of each.
const MAX_SCAN_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// The largest image `locate` reads (a full-page screenshot is a few MiB).
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
/// A text shorter than this is too common to point at a file.
const MIN_TEXT_LEN: usize = 3;
const SOURCE_EXTS: &[&str] = &[
    "svelte", "tsx", "jsx", "ts", "js", "vue", "dart", "html", "mdx", "astro",
];

/// The request as the agent sent it.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The image path as given (relative to the work tree, or absolute inside it).
    pub image: String,
    pub rect: Option<[f64; 4]>,
    pub url: Option<String>,
    pub width: Option<u32>,
}

impl Request {
    pub fn from_args(args: &Value) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            image: String,
            rect: Option<[f64; 4]>,
            url: Option<String>,
            width: Option<u32>,
        }
        let raw: Raw = serde_json::from_value(args.clone())
            .map_err(|e| format!("locate: bad arguments: {e}"))?;
        if raw.image.trim().is_empty() {
            return Err("locate: 'image' is the path of the screenshot in your work tree".into());
        }
        if let Some(r) = raw.rect {
            Rect::from_array(r)?;
        }
        Ok(Self {
            image: raw.image,
            rect: raw.rect,
            url: raw.url.map(|u| u.trim().to_string()),
            width: raw.width,
        })
    }
}

/// `image` resolved inside `work`: it must exist, be a file and stay inside the work tree once
/// symlinks are resolved (an agent cannot point `locate` at `/etc/passwd` or at another
/// crewmate's files).
pub fn resolve_image(work: &Path, image: &str) -> Result<PathBuf, String> {
    let root = work.canonicalize().map_err(|e| {
        format!(
            "locate: the work tree {} is not readable: {e}",
            work.display()
        )
    })?;
    let p = Path::new(image);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    };
    let real = joined
        .canonicalize()
        .map_err(|_| format!("locate: no image at {image} in your work tree"))?;
    if !real.starts_with(&root) {
        return Err(format!(
            "locate: {image} is outside your work tree; images are staged under .atlas/attachments/"
        ));
    }
    if !real.is_file() {
        return Err(format!("locate: {image} is not a file"));
    }
    Ok(real)
}

/// The pixel width of a PNG, JPEG, GIF or WebP, from its header; `None` for anything else.
pub fn image_width(bytes: &[u8]) -> Option<u32> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        return Some(u32::from_be_bytes(bytes[16..20].try_into().ok()?));
    }
    if bytes.starts_with(b"GIF8") && bytes.len() >= 10 {
        return Some(u16::from_le_bytes([bytes[6], bytes[7]]) as u32);
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16)? {
            b"VP8X" if bytes.len() >= 24 => {
                Some(1 + u32::from_le_bytes([bytes[24 - 6], bytes[24 - 5], bytes[24 - 4], 0]))
            }
            b"VP8 " if bytes.len() >= 30 => {
                Some((u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff) as u32)
            }
            b"VP8L" if bytes.len() >= 25 => {
                let b = u32::from_le_bytes(bytes[21..25].try_into().ok()?);
                Some((b & 0x3fff) + 1)
            }
            _ => None,
        };
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                return Some(u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32);
            }
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            i += 2 + len;
        }
    }
    None
}

/// The width the page is captured at: the one asked for, else the image's own when it is one of the
/// two Atlas captures at, else the desktop width.
pub fn capture_width(asked: Option<u32>, image_w: Option<u32>) -> u32 {
    asked
        .or(image_w.filter(|w| KNOWN_WIDTHS.contains(w)))
        .unwrap_or(DEFAULT_WIDTH)
}

/// One answer: where a region of the page is drawn in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub route_file: Option<String>,
    pub component_file: Option<String>,
    pub selector: String,
    pub testid: Option<String>,
    pub text: String,
    pub score: f64,
}

impl Candidate {
    pub fn to_json(&self) -> Value {
        json!({
            "route_file": self.route_file,
            "component_file": self.component_file,
            "selector": self.selector,
            "testid": self.testid,
            "text": self.text,
            "score": (self.score * 1000.0).round() / 1000.0,
        })
    }
}

/// The files of a work tree that mention `needle` (a testid or a text), best (shortest path) first.
/// `files` is the listing of the tree; only source-like files are read.
pub fn grep_sources(root: &Path, files: &[String], needle: &str) -> Vec<String> {
    let mut hits = vec![];
    for f in files {
        let ext = f.rsplit('.').next().unwrap_or("");
        if !SOURCE_EXTS.contains(&ext) {
            continue;
        }
        // Without following a link, at any component: the tree is the crewmate's and may change under us.
        if let Ok(src) = layout::read_beneath(root, f, MAX_FILE_BYTES)
            && src.contains(needle)
        {
            hits.push(f.clone());
        }
    }
    hits.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    hits
}

/// Candidates for `rect` on the page `dom` describes, with the source files they map to in `root`.
pub fn candidates(
    root: &Path,
    files: &[String],
    dom: &DomReport,
    rect: Option<Rect>,
) -> (Vec<Candidate>, bool) {
    let url_path = route::url_path(&dom.url);
    let routes = route::routes_for(root, files, &url_path);
    let route_file = routes.first().map(|r| r.file.clone());
    let ranked = rank::rank(dom, rect, MAX_CANDIDATES);
    let ambiguous = rank::is_ambiguous(&ranked.iter().map(|r| r.score).collect::<Vec<_>>());
    let route_name = route_file
        .as_deref()
        .map(|f| f.split(':').next().unwrap_or(f).to_string());
    let out = ranked
        .into_iter()
        .map(|r| {
            let e = &r.element;
            // The handle that finds the source: the author's testid, else the text the user reads.
            let needle = e.testid.clone().filter(|t| !t.is_empty()).or_else(|| {
                e.text
                    .clone()
                    .or_else(|| e.name.clone())
                    .map(|t| t.trim_end_matches('…').trim().to_string())
                    .filter(|t| t.chars().count() >= MIN_TEXT_LEN)
            });
            let component_file = needle.as_deref().and_then(|n| {
                grep_sources(root, files, n)
                    .into_iter()
                    // The route file itself is already reported as the route.
                    .find(|f| Some(f) != route_name.as_ref())
                    .or_else(|| route_name.clone())
            });
            Candidate {
                route_file: route_file.clone(),
                component_file,
                selector: e.selector.clone(),
                testid: e.testid.clone(),
                text: e
                    .text
                    .clone()
                    .or_else(|| e.name.clone())
                    .unwrap_or_default(),
                score: r.score,
            }
        })
        .collect();
    (out, ambiguous)
}

/// The work tree of crewmate `crewmate_id`: its project's checkout, its own `workdir`, or the
/// default `work/` folder, the same one the agent runs in.
pub fn work_tree(
    hostd_dir: &Path,
    crewmate: &super::store::Crewmate,
    iso: Option<&super::spawn::layout::Isolated>,
) -> Result<PathBuf, String> {
    super::project::current_work_dir(hostd_dir, crewmate, iso)
}

/// A capture error as the agent of `locate` should read it: the message starts with `capture:` (or
/// `capture incomplete`), which names a tool it did not call.
fn as_locate(e: &str) -> String {
    match e.strip_prefix("capture") {
        Some(rest) => format!("locate (page capture){rest}"),
        None => e.to_string(),
    }
}

/// What the `locate` hook route does. `Err` is the message the agent sees as the tool's error.
pub async fn handle(
    hostd_dir: &Path,
    store: &super::store::CrewStore,
    crewmate_id: &str,
    args: &Value,
) -> Result<Value, String> {
    let req = Request::from_args(args)?;
    let crewmate = store
        .load_crewmate(crewmate_id)
        .map_err(|e| format!("locate: crewmate {crewmate_id} is not readable: {e}"))?;
    let iso = store.isolated(crewmate_id);
    let work = work_tree(hostd_dir, &crewmate, iso.as_ref())?;
    let image = resolve_image(&work, &req.image)?;
    // Not `fs::read`: the tree is the crewmate's. No link followed, and no more than MAX_IMAGE_BYTES in memory.
    let rel = work
        .canonicalize()
        .ok()
        .and_then(|w| {
            image
                .strip_prefix(w)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .ok_or("locate: the image is outside your work tree")?;
    let bytes = layout::read_bytes_beneath(&work, &rel, MAX_IMAGE_BYTES)
        .map_err(|e| format!("locate: cannot read the image: {e}"))?;
    let image_w = image_width(&bytes);
    let Some(url) = req.url.clone() else {
        return Err(
            "locate: give the `url` the screenshot was taken from (a dev server on this machine \
             works): the element boxes come from the live page, not from the picture"
                .into(),
        );
    };
    let width = capture_width(req.width, image_w);
    let spec = Spec::from_args(&json!({
        "url": url, "widths": [width], "themes": ["light"], "engines": ["chromium"],
        "full_page": true, "dom": true,
    }))
    .map_err(|e| as_locate(&e))?;

    let scratch = capture::scratch_dir(hostd_dir);
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("locate: cannot make {}: {e}", scratch.display()))?;
    let _cleanup = capture::RemoveOnDrop(scratch.clone());
    let captured = capture::run(hostd_dir, &spec, &scratch)
        .await
        .map_err(|e| as_locate(&e))?;
    let (_, dom_path) = captured
        .doms
        .first()
        .ok_or("locate: the runner wrote no DOM report")?;
    let dom: DomReport = serde_json::from_slice(
        &std::fs::read(dom_path).map_err(|e| format!("locate: cannot read the DOM report: {e}"))?,
    )
    .map_err(|e| format!("locate: the DOM report is not valid: {e}"))?;

    // A rect is in the image's pixels; the page was captured at `width` CSS px. An image of another
    // width (a retina or a resized one) is scaled onto the page.
    let scale = match image_w {
        Some(w) if w > 0 && w != width => f64::from(width) / f64::from(w),
        _ => 1.0,
    };
    let rect = req
        .rect
        .map(|[x, y, w, h]| Rect::from_array([x * scale, y * scale, w * scale, h * scale]))
        .transpose()?;

    let files = route::list_files(&work, MAX_SCAN_FILES);
    let (found, ambiguous) = candidates(&work, &files, &dom, rect);
    if found.is_empty() {
        return Err(match rect {
            Some(_) => "locate: no element of the page is under that rect (is the URL the page of the screenshot, at the same state?)".into(),
            None => "locate: the page has no visible elements".into(),
        });
    }
    let mut out = json!({
        "url": url,
        "width": width,
        "image_width": image_w,
        "ambiguous": ambiguous,
        "candidates": found.iter().map(Candidate::to_json).collect::<Vec<_>>(),
    });
    if ambiguous {
        out["next"] = json!(
            "The top two candidates are within 10% of each other: ask_user which one the user \
             means, with each candidate's selector and text as the options, before editing."
        );
    }
    Ok(out)
}

#[cfg(all(test, unix))]
pub(crate) mod tests;
