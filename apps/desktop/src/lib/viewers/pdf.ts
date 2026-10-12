/** Pure math/state for the PDF viewer (zoom, layout, virtualization, search); no DOM, for tests. */

export const PDF_MIN_ZOOM = 0.25;
export const PDF_MAX_ZOOM = 8;
/** Vertical gap between pages and padding around the column (CSS px, unscaled). */
export const PAGE_GAP = 14;
export const PAGE_PAD = 20;

export const clampPdfZoom = (z: number) => Math.min(PDF_MAX_ZOOM, Math.max(PDF_MIN_ZOOM, z));

export type PageSize = { w: number; h: number };

/** Scale that makes the widest page fill the box (minus side padding). */
export function fitWidthScale(pages: PageSize[], boxW: number, pad = PAGE_PAD): number {
	const w = Math.max(0, ...pages.map((p) => p.w));
	if (!w || boxW <= 0) return 1;
	return clampPdfZoom((boxW - pad * 2) / w);
}

/** Scale that makes the current page fit entirely in the box. */
export function fitPageScale(page: PageSize | undefined, box: PageSize, pad = PAGE_PAD): number {
	if (!page?.w || !page.h || box.w <= 0 || box.h <= 0) return 1;
	return clampPdfZoom(Math.min((box.w - pad * 2) / page.w, (box.h - pad * 2) / page.h));
}

/** Multiplicative zoom step for ⌘+/⌘−/toolbar buttons. */
export const ZOOM_STEP = 1.2;
export const stepZoom = (z: number, dir: 1 | -1) => clampPdfZoom(dir > 0 ? z * ZOOM_STEP : z / ZOOM_STEP);

/** Zoom factor from a wheel event: pinch (ctrlKey) is finer than ⌘+wheel. */
export const wheelZoomFactor = (deltaY: number, pinch: boolean) => Math.exp(-deltaY * (pinch ? 0.01 : 0.004));

export type Layout = {
	/** Top offset of every page inside the scroller content (px, at `scale`). */
	tops: number[];
	/** Rendered size of every page (px, at `scale`). */
	sizes: PageSize[];
	/** Total content height. */
	height: number;
	/** Widest page width (content min-width). */
	width: number;
};

/** Vertical stack of pages at `scale`. */
export function layoutPages(pages: PageSize[], scale: number, gap = PAGE_GAP, pad = PAGE_PAD): Layout {
	const tops: number[] = [];
	const sizes: PageSize[] = [];
	let y = pad;
	let width = 0;
	for (const p of pages) {
		const s = { w: p.w * scale, h: p.h * scale };
		tops.push(y);
		sizes.push(s);
		y += s.h + gap;
		width = Math.max(width, s.w);
	}
	return { tops, sizes, height: pages.length ? y - gap + pad : pad * 2, width };
}

/** Binary search: index of the page containing (or nearest above) content offset `y`. */
export function pageAt(layout: Layout, y: number): number {
	const { tops, sizes } = layout;
	if (!tops.length) return 0;
	let lo = 0;
	let hi = tops.length - 1;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (tops[mid] <= y) lo = mid;
		else hi = mid - 1;
	}
	// In the gap below page `lo`, prefer the next page when it is closer.
	const next = lo + 1;
	if (next < tops.length && y > tops[lo] + sizes[lo].h && tops[next] - y < y - (tops[lo] + sizes[lo].h))
		return next;
	return lo;
}

/**
 * Page windowing for a viewport `[top, top+height)`: `visible` intersect it, `near` adds `margin`
 * viewports above/below (rendered ahead of time), both as inclusive index ranges.
 */
export function visibleRange(
	layout: Layout,
	top: number,
	height: number,
	margin = 1,
): { visible: [number, number]; near: [number, number] } | null {
	const n = layout.tops.length;
	if (!n || height <= 0) return null;
	const range = (a: number, b: number): [number, number] => {
		let first = pageAt(layout, a);
		let last = pageAt(layout, b);
		// pageAt rounds gaps to the nearest page; keep only pages that really intersect.
		if (layout.tops[first] + layout.sizes[first].h < a && first < n - 1) first++;
		if (layout.tops[last] > b && last > 0) last--;
		return [Math.min(first, last), Math.max(first, last)];
	};
	return {
		visible: range(top, top + height),
		near: range(Math.max(0, top - height * margin), top + height * (1 + margin)),
	};
}

/** The page that owns the viewport: the one covering its vertical center. */
export function currentPage(layout: Layout, top: number, height: number): number {
	return pageAt(layout, top + height / 2);
}

/**
 * New scroll offsets after changing zoom from `from` to `to` keeping the document point under the
 * cursor (`at`, relative to the scroller's top-left) fixed. Page gaps and padding are constant, so
 * the point is tracked as (page, fraction of that page's height) and re-projected on the new layout;
 * horizontally everything scales linearly around the padding.
 */
export function anchorScroll(
	pages: PageSize[],
	scroll: { x: number; y: number },
	at: { x: number; y: number },
	from: number,
	to: number,
	pad = PAGE_PAD,
	gap = PAGE_GAP,
): { x: number; y: number } {
	const k = to / from;
	const x = (() => {
		const c = scroll.x + at.x;
		return Math.max(0, c <= pad ? scroll.x : pad + (c - pad) * k - at.x);
	})();
	if (!pages.length) return { x, y: scroll.y };
	const old = layoutPages(pages, from, gap, pad);
	const next = layoutPages(pages, to, gap, pad);
	const cy = scroll.y + at.y;
	const i = pageAt(old, cy);
	const off = cy - old.tops[i];
	// Inside the page: keep the same fraction of it. In a gap/padding: keep the pixel distance.
	const ny =
		off >= 0 && off <= old.sizes[i].h
			? next.tops[i] + (off / old.sizes[i].h) * next.sizes[i].h
			: next.tops[i] + off + (off > 0 ? next.sizes[i].h - old.sizes[i].h : 0);
	return { x, y: Math.max(0, ny - at.y) };
}

/** Least-recently-used cap for rendered bitmaps: returns the indices to evict. */
export function evictFar(cached: number[], center: number, keep: number): number[] {
	if (cached.length <= keep) return [];
	return [...cached].sort((a, b) => Math.abs(a - center) - Math.abs(b - center)).slice(keep);
}

export type Match = { page: number; start: number; end: number };

/** Case-insensitive, diacritic-insensitive needle search in one page's text; non-overlapping. */
export function findMatches(text: string, query: string, page: number): Match[] {
	const q = fold(query.trim());
	if (!q) return [];
	const hay = fold(text);
	const out: Match[] = [];
	for (let i = hay.indexOf(q); i !== -1; i = hay.indexOf(q, i + q.length)) {
		out.push({ page, start: i, end: i + q.length });
	}
	return out;
}

/** Lowercase + strip combining marks; length-preserving for the Latin text we meet (NFD pieces are re-composed per char). */
export function fold(s: string): string {
	let out = "";
	for (const ch of s) {
		const base = ch.normalize("NFD")[0];
		out += base.toLowerCase().length === 1 ? base.toLowerCase() : ch.toLowerCase();
	}
	return out;
}

/** Next/previous match index with wraparound; -1 when there are none. */
export function stepMatch(cur: number, total: number, dir: 1 | -1): number {
	if (total <= 0) return -1;
	if (cur < 0) return dir > 0 ? 0 : total - 1;
	return (cur + dir + total) % total;
}

/** Parses the editable page box (`"7"`, `" 7 "`); null when it is not a page in range. */
export function parsePage(input: string, total: number): number | null {
	const n = Number(input.trim());
	if (!Number.isInteger(n) || n < 1 || n > total) return null;
	return n - 1;
}

/** Remembered per-file view, kept for the session. */
export type ViewState = { scrollTop: number; zoom: number | "width" | "page"; page: number };
const memory = new Map<string, ViewState>();
export const rememberView = (key: string, v: ViewState) => void memory.set(key, v);
export const recallView = (key: string): ViewState | undefined => memory.get(key);

/** Idle timeout before the floating toolbar fades out. */
export const TOOLBAR_IDLE_MS = 1800;
