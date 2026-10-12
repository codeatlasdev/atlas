// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { MAX_BYTES, MIME, extOf, formatBytes, hexDump, viewerKind } from "./kind.ts";
import { clampZoom, fitScale, zoomAt } from "./zoom.ts";
import { resolveRelative } from "./markdown.ts";

test("viewer kind follows the extension, case-insensitively", () => {
	expect(viewerKind("a/b/photo.PNG")).toBe("image");
	expect(viewerKind("x.avif")).toBe("image");
	expect(viewerKind("logo.svg")).toBe("svg");
	expect(viewerKind("doc.pdf")).toBe("pdf");
	expect(viewerKind("clip.mov")).toBe("video");
	expect(viewerKind("song.m4a")).toBe("audio");
	expect(viewerKind("README.md")).toBe("markdown");
	expect(viewerKind("main.rs")).toBe("text");
});

test("dotfiles and extensionless names are text, not a bogus extension", () => {
	expect(extOf(".png")).toBe("");
	expect(viewerKind(".png")).toBe("text");
	expect(viewerKind("Makefile")).toBe("text");
	expect(extOf("dir.d/file")).toBe("");
});

test("every non-text kind has a mime and a size cap; text stays small", () => {
	for (const p of ["a.png", "a.pdf", "a.mp4", "a.mp3", "a.svg"]) expect(MIME[extOf(p)]).toBeTruthy();
	expect(MAX_BYTES.pdf).toBe(200 * 1024 * 1024);
	expect(MAX_BYTES.text).toBeLessThan(MAX_BYTES.image);
});

test("formatBytes picks a readable unit", () => {
	expect(formatBytes(512)).toBe("512 B");
	expect(formatBytes(1536)).toBe("1.5 KB");
	expect(formatBytes(200 * 1024 * 1024)).toBe("200 MB");
});

test("hexDump pads short rows and masks non-printables", () => {
	const out = hexDump(Uint8Array.from([0x48, 0x69, 0x00, 0xff]));
	expect(out.startsWith("00000000  48 69 00 ff")).toBe(true);
	expect(out.endsWith("|Hi..|")).toBe(true);
	expect(hexDump(new Uint8Array(32)).split("\n")).toHaveLength(2);
});

test("fit never upscales and respects both axes", () => {
	expect(fitScale({ w: 100, h: 100 }, { w: 1000, h: 1000 })).toBe(1);
	expect(fitScale({ w: 2000, h: 1000 }, { w: 1032, h: 1032 })).toBeCloseTo(0.5);
	expect(fitScale({ w: 0, h: 0 }, { w: 10, h: 10 })).toBe(1);
});

test("zoom keeps the point under the cursor fixed and clamps", () => {
	const pan = zoomAt({ x: 0, y: 0 }, { x: 100, y: 0 }, 1, 2);
	expect(pan).toEqual({ x: -100, y: 0 });
	// the image point under the cursor: (at - pan) / scale is unchanged
	expect((100 - pan.x) / 2).toBe((100 - 0) / 1);
	expect(clampZoom(1000)).toBe(32);
	expect(clampZoom(0)).toBe(0.05);
});

test("markdown relative images resolve against the file's directory", () => {
	expect(resolveRelative("docs", "img/a.png")).toBe("docs/img/a.png");
	expect(resolveRelative("docs/guide", "../a.png?x=1")).toBe("docs/a.png");
	expect(resolveRelative("", "./a.png")).toBe("a.png");
});

// ---- pdf.ts ----
import {
	anchorScroll,
	evictFar,
	findMatches,
	fitPageScale,
	fitWidthScale,
	layoutPages,
	pageAt,
	parsePage,
	recallView,
	rememberView,
	stepMatch,
	stepZoom,
	visibleRange,
	clampPdfZoom,
	currentPage,
} from "./pdf.ts";

const A4 = { w: 600, h: 800 };
const pagesOf = (n: number) => Array.from({ length: n }, () => A4);

test("pdf fit-width uses the widest page and clamps", () => {
	expect(fitWidthScale([A4, { w: 1000, h: 500 }], 1040)).toBeCloseTo(1);
	expect(fitWidthScale(pagesOf(1), 0)).toBe(1);
	expect(fitWidthScale([{ w: 10, h: 10 }], 100000)).toBe(8);
	expect(fitPageScale(A4, { w: 1000, h: 440 })).toBeCloseTo(0.5);
});

test("pdf zoom steps clamp at both ends", () => {
	expect(clampPdfZoom(100)).toBe(8);
	expect(clampPdfZoom(0)).toBe(0.25);
	expect(stepZoom(8, 1)).toBe(8);
	expect(stepZoom(1, 1)).toBeCloseTo(1.2);
	expect(stepZoom(1, -1)).toBeCloseTo(1 / 1.2);
});

test("pdf zoom anchoring keeps the document point under the cursor fixed, gaps included", () => {
	const pad = 20;
	const pages = pagesOf(400);
	const at = { x: 300, y: 200 };
	for (const [from, to, y] of [
		[1, 2, 20000],
		[1.7, 3.5, 41000],
		[2, 0.5, 90000],
		[1, 1.2, 0],
	]) {
		const scroll = { x: 0, y };
		const after = anchorScroll(pages, scroll, at, from, to, pad);
		const a = layoutPages(pages, from, 14, pad);
		const b = layoutPages(pages, to, 14, pad);
		const cy = scroll.y + at.y;
		const i = pageAt(a, cy);
		const frac = (cy - a.tops[i]) / a.sizes[i].h;
		const cy2 = after.y + at.y;
		expect(pageAt(b, cy2)).toBe(i);
		// same fraction of the same page under the cursor (a constant-gap layout is not linear in zoom)
		expect((cy2 - b.tops[i]) / b.sizes[i].h).toBeCloseTo(frac, 3);
	}
	// never scrolls to negative offsets
	expect(anchorScroll(pages, { x: 0, y: 0 }, { x: 0, y: 0 }, 2, 0.5, pad)).toEqual({ x: 0, y: 0 });
	// horizontally, content scales around the padding
	expect(anchorScroll(pages, { x: 0, y: 0 }, { x: 320, y: 0 }, 1, 2, pad).x).toBe(300);
});

test("pdf layout stacks pages with gaps and scales", () => {
	const l = layoutPages(pagesOf(3), 0.5, 10, 20);
	expect(l.tops).toEqual([20, 20 + 400 + 10, 20 + 2 * 410]);
	expect(l.height).toBe(20 + 3 * 400 + 2 * 10 + 20);
	expect(l.width).toBe(300);
	expect(layoutPages([], 1).tops).toEqual([]);
});

test("pdf pageAt finds the page, preferring the nearer one inside a gap", () => {
	const l = layoutPages(pagesOf(3), 1, 40, 0); // tops 0, 840, 1680; heights 800
	expect(pageAt(l, 0)).toBe(0);
	expect(pageAt(l, 799)).toBe(0);
	expect(pageAt(l, 805)).toBe(0);
	expect(pageAt(l, 835)).toBe(1);
	expect(pageAt(l, 99999)).toBe(2);
	expect(pageAt(layoutPages([], 1), 10)).toBe(0);
});

test("pdf windowing renders only pages near the viewport (100+ pages)", () => {
	const l = layoutPages(pagesOf(500), 1, 14, 20);
	const w = visibleRange(l, 0, 900, 1)!;
	expect(w.visible[0]).toBe(0);
	expect(w.visible[1]).toBeLessThanOrEqual(1);
	expect(w.near[1] - w.near[0]).toBeLessThan(6);
	const mid = visibleRange(l, l.tops[250] + 100, 900, 1)!;
	expect(mid.near[0]).toBeGreaterThan(245);
	expect(mid.near[1]).toBeLessThan(256);
	expect(visibleRange(l, 0, 0)).toBeNull();
	expect(currentPage(l, l.tops[10], 900)).toBe(10);
});

test("pdf bitmap cache evicts the farthest pages first", () => {
	expect(evictFar([1, 2, 3], 2, 6)).toEqual([]);
	expect(evictFar([1, 2, 3, 4, 10, 20], 2, 3).sort((a, b) => a - b)).toEqual([4, 10, 20]);
});

test("pdf search is case/diacritic-insensitive, non-overlapping, indexed per page", () => {
	expect(findMatches("Café cafe CAFÉ", "cafe", 3)).toEqual([
		{ page: 3, start: 0, end: 4 },
		{ page: 3, start: 5, end: 9 },
		{ page: 3, start: 10, end: 14 },
	]);
	expect(findMatches("aaaa", "aa", 0)).toHaveLength(2);
	expect(findMatches("abc", "  ", 0)).toEqual([]);
});

test("pdf match stepping wraps both ways and handles empty", () => {
	expect(stepMatch(-1, 3, 1)).toBe(0);
	expect(stepMatch(-1, 3, -1)).toBe(2);
	expect(stepMatch(2, 3, 1)).toBe(0);
	expect(stepMatch(0, 3, -1)).toBe(2);
	expect(stepMatch(0, 0, 1)).toBe(-1);
});

test("pdf page box accepts only real pages", () => {
	expect(parsePage(" 3 ", 12)).toBe(2);
	expect(parsePage("0", 12)).toBeNull();
	expect(parsePage("13", 12)).toBeNull();
	expect(parsePage("2.5", 12)).toBeNull();
	expect(parsePage("abc", 12)).toBeNull();
});

test("pdf view state is remembered per file for the session", () => {
	rememberView("a.pdf", { scrollTop: 900, zoom: 1.5, page: 4 });
	expect(recallView("a.pdf")).toEqual({ scrollTop: 900, zoom: 1.5, page: 4 });
	expect(recallView("b.pdf")).toBeUndefined();
});
