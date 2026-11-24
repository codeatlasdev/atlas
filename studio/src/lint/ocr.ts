// OCR for the text checks: `tesseract` when installed (TSV with boxes), otherwise a tiny macOS Vision helper compiled
// on demand from studio/native/vision.swift (same scheme as the window recorder: the source hash is in the binary's
// name). Both return word/line boxes in pixels of the image, top-left origin.

import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { run, sha256 } from "../lib/exec.ts";
import { CACHE, STUDIO } from "../lib/paths.ts";
import { contrastRatio, luminance, meanRgb, type Image } from "./pixels.ts";
import type { Diagnostic, Profile, Rect } from "./types.ts";

export interface TextBox {
	text: string;
	rect: Rect;
	conf: number;
}

export type OcrEngine = "tesseract" | "vision";

export async function hasTesseract(): Promise<boolean> {
	return (await run(["which", "tesseract"], { allowFail: true })).code === 0;
}

const SOURCE = join(STUDIO, "native/vision.swift");
const BIN_DIR = join(CACHE, "bin");

async function ensureVision(): Promise<string> {
	const hash = sha256(await Bun.file(SOURCE).text()).slice(0, 12);
	const bin = join(BIN_DIR, `vision-${hash}`);
	if (existsSync(bin)) return bin;
	mkdirSync(BIN_DIR, { recursive: true });
	for (const old of readdirSync(BIN_DIR)) if (/^vision-[0-9a-f]{12}$/.test(old)) rmSync(join(BIN_DIR, old), { force: true });
	await run(["swiftc", "-O", SOURCE, "-o", bin]);
	return bin;
}

/** tesseract `tsv` output -> boxes of lines. Columns: level page block par line word left top width height conf text. */
export function parseTesseractTsv(tsv: string): TextBox[] {
	const lines = new Map<string, { words: string[]; x0: number; y0: number; x1: number; y1: number; conf: number; n: number }>();
	for (const row of tsv.split("\n").slice(1)) {
		const c = row.split("\t");
		if (c.length < 12 || c[0] !== "5") continue;
		const text = (c[11] ?? "").trim();
		const conf = Number(c[10]);
		if (!text || conf < 0) continue;
		const [left, top, w, h] = [Number(c[6]), Number(c[7]), Number(c[8]), Number(c[9])] as [number, number, number, number];
		const key = `${c[2]}.${c[3]}.${c[4]}`;
		const l = lines.get(key) ?? { words: [], x0: left, y0: top, x1: left + w, y1: top + h, conf: 0, n: 0 };
		l.words.push(text);
		l.x0 = Math.min(l.x0, left);
		l.y0 = Math.min(l.y0, top);
		l.x1 = Math.max(l.x1, left + w);
		l.y1 = Math.max(l.y1, top + h);
		l.conf += conf;
		l.n++;
		lines.set(key, l);
	}
	return [...lines.values()].map((l) => ({ text: l.words.join(" "), rect: { x: l.x0, y: l.y0, w: l.x1 - l.x0, h: l.y1 - l.y0 }, conf: l.conf / l.n / 100 }));
}

/** vision.swift JSON lines -> boxes. */
export function parseVisionLines(out: string): TextBox[] {
	return out
		.split("\n")
		.filter((l) => l.startsWith("{"))
		.map((l) => JSON.parse(l) as { text: string; x: number; y: number; w: number; h: number; conf: number })
		.map((o) => ({ text: o.text, rect: { x: o.x, y: o.y, w: o.w, h: o.h }, conf: o.conf }));
}

export async function ocr(file: string): Promise<{ engine: OcrEngine; boxes: TextBox[] }> {
	if (await hasTesseract()) {
		const { out } = await run(["tesseract", file, "-", "--psm", "11", "tsv"]);
		return { engine: "tesseract", boxes: parseTesseractTsv(out) };
	}
	if (process.platform !== "darwin") return { engine: "vision", boxes: [] };
	const bin = await ensureVision();
	const { out } = await run([bin, file]);
	return { engine: "vision", boxes: parseVisionLines(out) };
}

/**
 * Legibility: a line's box height is the glyph band (ascender to descender). The size it will have on the surface is
 * `h * displayHeight / frameHeight`; below `minTextPx` it is unreadable on a phone / embedded player.
 */
export function checkTextSize(boxes: TextBox[], frameH: number, profile: Profile): Diagnostic[] {
	const out: Diagnostic[] = [];
	const k = profile.displayHeight / frameH;
	for (const b of boxes) {
		if (b.conf < 0.5 || b.text.trim().length < 3) continue;
		const shown = b.rect.h * k;
		if (shown >= profile.minTextPx) continue;
		out.push({
			severity: shown < profile.minTextPx * 0.6 ? "error" : "warning",
			code: "text/too-small",
			message: `"${b.text}" is ${shown.toFixed(1)}px tall at ${profile.displayHeight}px display height (min ${profile.minTextPx}px for ${profile.id})`,
			rect: b.rect,
			measured: +shown.toFixed(1),
			limit: profile.minTextPx,
			fix: { action: "enlarge-text", description: `make the text at least ${Math.ceil(profile.minTextPx / k)}px tall in the ${frameH}px frame, or crop closer to it`, params: { minHeightPx: Math.ceil(profile.minTextPx / k) } },
		});
	}
	return out;
}

/**
 * The ink of an OCR box, WCAG 2.x: the text colour is the pixel of the box farthest from the box's background (mean of
 * the band above it), `best` the contrast between the two.
 */
function ink(img: Image, r: Rect): { best: number; fg: [number, number, number]; bg: [number, number, number] } {
	const pad = Math.max(2, Math.round(r.h * 0.15));
	const bg = meanRgb(img, { x: r.x - pad, y: r.y - pad, w: r.w + 2 * pad, h: pad });
	const bgL = luminance(...bg);
	let best = 0;
	let fg: [number, number, number] = bg;
	for (let y = r.y; y < r.y + r.h; y++)
		for (let x = r.x; x < r.x + r.w; x++) {
			const i = (y * img.w + x) * 3;
			if (x < 0 || y < 0 || x >= img.w || y >= img.h) continue;
			const rgb: [number, number, number] = [img.data[i]!, img.data[i + 1]!, img.data[i + 2]!];
			const c = contrastRatio(luminance(...rgb), bgL);
			if (c > best) {
				best = c;
				fg = rgb;
			}
		}
	return { best, fg, bg };
}

/** Below this, a box's "ink" is the picture's own noise (a gradient, JPEG blocks): no glyph a viewer can see. */
const NO_INK = 1.2;

/**
 * The OCR lines a viewer can see. OCR reads letters into gradients and noise ("ibs" at 1.05:1 in the poster's aurora);
 * such a line is neither too small nor low contrast, it is not text. Faint but visible text (1.5:1) stays.
 */
export function withInk(img: Image, boxes: TextBox[]): TextBox[] {
	return boxes.filter((b) => ink(img, b.rect).best >= NO_INK);
}

/** Contrast of detected text against its background: large text needs 3:1, normal text 4.5:1. */
export function checkContrast(img: Image, boxes: TextBox[], profile: Profile): Diagnostic[] {
	const out: Diagnostic[] = [];
	for (const b of boxes) {
		if (b.conf < 0.5 || b.text.trim().length < 3) continue;
		const r = b.rect;
		const { best, fg, bg } = ink(img, r);
		const shown = (r.h * profile.displayHeight) / img.h;
		const large = shown >= 24;
		const need = large ? 3 : 4.5;
		if (best >= need) continue;
		out.push({
			severity: best < 2 ? "error" : "warning",
			code: "text/low-contrast",
			message: `"${b.text}" has contrast ${best.toFixed(2)}:1 against its background (WCAG needs ${need}:1 for ${large ? "large" : "normal"} text)`,
			rect: r,
			measured: +best.toFixed(2),
			limit: need,
			fix: { action: "raise-contrast", description: `darken the background or lighten the text (fg rgb(${fg.map(Math.round)}) on bg rgb(${bg.map(Math.round)}))` },
		});
	}
	return out;
}

/**
 * Split OCR lines into text the film authored (titles, captions, overlays: on the field, or straddling the subject's
 * edge) and text that is part of the captured app UI (fully inside the subject box). Legibility and contrast are
 * judged on the first: a recording's own UI text is whatever the app drew, and a film cannot re-typeset it.
 */
export function scopeTextToAuthored(boxes: TextBox[], subject: Rect | undefined): { authored: TextBox[]; captured: TextBox[] } {
	if (!subject) return { authored: boxes, captured: [] };
	const inside = (r: Rect) => r.x >= subject.x && r.y >= subject.y && r.x + r.w <= subject.x + subject.w && r.y + r.h <= subject.y + subject.h;
	return { authored: boxes.filter((b) => !inside(b.rect)), captured: boxes.filter((b) => inside(b.rect)) };
}
