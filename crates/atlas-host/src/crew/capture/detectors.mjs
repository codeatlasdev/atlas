// TEMPORARY. UI detectors for Atlas Crew's `capture {checks: true}` and the UI gate (#215).
//
// This file is a local stand-in for `@codeatlasdev/ui/testing/detectors` (codeatlasdev/ui#1290). It
// implements the API that issue asks for, with the same names, so the swap is one import in
// `runner.mjs`:
//
//     import { detect, observeShifts } from "./detectors.mjs";                     // today
//     import { detect, observeShifts } from "@codeatlasdev/ui/testing/detectors";  // when it ships
//
// and then this file, its fixtures (`crates/atlas-host/src/crew/capture/fixtures/`) and the
// "UI detectors (temporary)" section of docs/crew.md are deleted. See that section for the plan.
//
//     type Finding = { detector: string; severity: "error" | "warn"; message: string;
//                      rect?: DOMRectInit; selector?: string };
//     detect(page: Playwright Page, opts?: { only?: string[], ...thresholds }): Promise<Finding[]>
//
// No dependency on any component kit: every detector works on any page (Atlas runs on customers'
// apps). Each one is a heuristic, written down next to it, and favours a missed finding over a false
// one: a gate that cries wolf is switched off.
//
// How it runs: `detect` evaluates `inPage` once inside the page for the static detectors, activates
// controls with Playwright's keyboard for `focus-after-tap`, and reads the layout shifts
// `observeShifts` recorded. `observeShifts` must be registered BEFORE navigation
// (`await page.addInitScript(observeShifts)`; the runner does) so shifts during load are seen.

export const DETECTORS = [
	"empty-band",
	"gutter",
	"sibling-overlap",
	"raw-text",
	"action-hierarchy",
	"cls",
	"focus-after-tap",
];

/** Thresholds. Every one can be overridden through `detect(page, {…})`. */
export const DEFAULTS = {
	// empty-band: the longest stretch of page height with nothing in it. 490 px at 390 wide is the
	// band of codeatlasdev/ui#1084; on a wider viewport the band is at least 60 % of a screen.
	emptyBandPx: 490,
	emptyBandScreens: 0.6,
	// gutter: the smallest distance between content and the viewport edge, on viewports up to
	// `mobileMaxWidth` px.
	gutterPx: 12,
	mobileMaxWidth: 500,
	// sibling-overlap: two in-flow siblings overlap when the shared area is at least this many px²
	// AND at least this share of the smaller box.
	overlapMinPx2: 16,
	overlapMinShare: 0.15,
	// cls: the sum of the layout shifts (without recent input) that fails; below it is a warning
	// once it passes `clsWarn`. Google's "good" is 0.1.
	clsError: 0.1,
	clsWarn: 0.02,
	// focus-after-tap: how many controls are activated at most.
	tapMax: 6,
};

/**
 * Runs in the page BEFORE it loads (`page.addInitScript`). Records every layout shift with the
 * node that moved, for the `cls` detector.
 *
 * Chromium reports them itself (`layout-shift` entries, with the moved nodes). WebKit has no such
 * entry type, so there the same quantity is measured by hand, following the definition of the
 * spec: once per frame, the outermost in-flow box that moved (a block that moves takes its
 * children along, they are not counted again) scores the viewport share of its old and new place
 * times the longest distance it moved over the viewport's longer side. Boxes are compared in
 * DOCUMENT coordinates, because scrolling moves everything in the viewport and shifts nothing. The
 * moved box with the largest area is the source. Moves within 500 ms of a pointer or key input are
 * expected and are not counted, like the native ones.
 */
export function observeShifts() {
	const w = window;
	if (w.__atlasShifts) return;
	const rec = (w.__atlasShifts = { total: 0, entries: [], native: false });
	const sel = (el) => {
		if (!el || el.nodeType !== 1) return null;
		if (el.id) return `#${CSS.escape(el.id)}`;
		const parts = [];
		for (let n = el; n && n.nodeType === 1 && n !== document.documentElement && parts.length < 4; n = n.parentElement) {
			const t = n.getAttribute("data-testid");
			if (t) {
				parts.unshift(`[data-testid="${t}"]`);
				break;
			}
			let i = 1;
			for (let s = n.previousElementSibling; s; s = s.previousElementSibling) if (s.tagName === n.tagName) i++;
			parts.unshift(`${n.tagName.toLowerCase()}:nth-of-type(${i})`);
		}
		return parts.join(" > ");
	};
	if (PerformanceObserver.supportedEntryTypes && PerformanceObserver.supportedEntryTypes.includes("layout-shift")) {
		rec.native = true;
		new PerformanceObserver((list) => {
			for (const e of list.getEntries()) {
				if (e.hadRecentInput) continue;
				rec.total += e.value;
				const s = (e.sources || []).find((x) => x.node && x.node.nodeType === 1) || (e.sources || [])[0];
				const r = s && s.currentRect;
				rec.entries.push({
					value: e.value,
					selector: s ? sel(s.node) : null,
					rect: r ? { x: r.x, y: r.y, width: r.width, height: r.height } : undefined,
				});
			}
		}).observe({ type: "layout-shift", buffered: true });
		return;
	}
	// Fallback (WebKit): sample the boxes of what is in the viewport once per frame. The unit is the
	// outermost box that moved (a block that moves takes its children with it: only the outermost is
	// recorded, like the native entry names the block, not every word in it).
	let lastInput = -1e9;
	for (const t of ["pointerdown", "keydown", "touchstart", "mousedown"]) addEventListener(t, () => (lastInput = performance.now()), { capture: true, passive: true });
	let prev = new Map();
	const tick = () => {
		const vw = innerWidth;
		const vh = innerHeight;
		const now = new Map();
		const moved = [];
		const sx = scrollX;
		const sy = scrollY;
		const doc = (b) => ({ top: b.top + sy, bottom: b.bottom + sy, left: b.left + sx, right: b.right + sx, width: b.width, height: b.height });
		// In the viewport NOW, in document coordinates (a box that scrolled out is not on screen).
		const inView = (b) => b.bottom - sy > 0 && b.top - sy < vh && b.right - sx > 0 && b.left - sx < vw;
		const walk = (el) => {
			for (const c of el.children) {
				if (/^(SCRIPT|STYLE|LINK|META|NOSCRIPT|TEMPLATE|HEAD)$/.test(c.tagName)) continue;
				const cs = getComputedStyle(c);
				if (cs.display === "none" || cs.visibility === "hidden" || cs.position === "fixed" || cs.position === "absolute") continue;
				const b = c.getBoundingClientRect();
				if (b.width <= 0 || b.height <= 0) continue;
				// Document coordinates: scrolling moves every box in the viewport's but shifts nothing.
				const r = doc(b);
				now.set(c, r);
				const o = prev.get(c);
				if (o && (o.top !== r.top || o.left !== r.left) && (inView(o) || inView(r))) {
					moved.push({ el: c, o, r });
					// Its children moved with it: do not count them again.
					for (const d of c.querySelectorAll("*")) {
						const dr = d.getBoundingClientRect();
						if (dr.width > 0) now.set(d, doc(dr));
					}
					continue;
				}
				walk(c);
			}
		};
		if (document.body) walk(document.body);
		if (moved.length && performance.now() - lastInput >= 500) {
			// One frame, one score: the viewport share of every moved box (old and new place) times
			// the longest distance any of them travelled over the viewport's longer side.
			let area = 0;
			let dist = 0;
			let worst = moved[0];
			for (const m of moved) {
				const top = Math.max(0, Math.min(m.o.top, m.r.top) - sy);
				const bottom = Math.min(vh, Math.max(m.o.bottom, m.r.bottom) - sy);
				const left = Math.max(0, Math.min(m.o.left, m.r.left) - sx);
				const right = Math.min(vw, Math.max(m.o.right, m.r.right) - sx);
				const a = Math.max(0, bottom - top) * Math.max(0, right - left);
				area += a;
				const d = Math.max(Math.abs(m.r.top - m.o.top), Math.abs(m.r.left - m.o.left));
				dist = Math.max(dist, d);
				if (a > 0 && a >= worst.area) worst = { ...m, area: a };
				else if (worst.area === undefined) worst = { ...m, area: a };
			}
			const value = Math.min(1, area / (vw * vh)) * (dist / Math.max(vw, vh));
			if (value > 0) {
				rec.total += value;
				rec.entries.push({ value, selector: sel(worst.el), rect: { x: worst.r.left, y: worst.r.top, width: worst.r.width, height: worst.r.height } });
			}
		}
		prev = now;
		requestAnimationFrame(tick);
	};
	requestAnimationFrame(tick);
}

/**
 * The five static detectors and the list of controls `focus-after-tap` taps. Runs in the page and
 * reads only (no clicks, no network). Self-contained: Playwright serialises it, so it may not use
 * anything from outside.
 */
export function inPage([only, o]) {
	const want = (name) => !only || only.includes(name);
	const out = [];
	const vw = window.innerWidth;
	const vh = window.innerHeight;
	const de = document.documentElement;
	const pageH = Math.max(de.scrollHeight, vh);
	const sx = window.scrollX;
	const sy = window.scrollY;

	const sel = (el) => {
		if (el.id && document.querySelectorAll(`#${CSS.escape(el.id)}`).length === 1) return `#${CSS.escape(el.id)}`;
		const parts = [];
		for (let n = el; n && n.nodeType === 1 && n !== document.documentElement && parts.length < 4; n = n.parentElement) {
			const t = n.getAttribute("data-testid");
			if (t) {
				parts.unshift(`[data-testid="${t}"]`);
				break;
			}
			let i = 1;
			for (let s = n.previousElementSibling; s; s = s.previousElementSibling) if (s.tagName === n.tagName) i++;
			parts.unshift(`${n.tagName.toLowerCase()}:nth-of-type(${i})`);
		}
		return parts.join(" > ");
	};
	const box = (el) => {
		const r = el.getBoundingClientRect();
		return { x: r.left + sx, y: r.top + sy, width: r.width, height: r.height };
	};
	const visible = (el) => {
		const cs = getComputedStyle(el);
		if (cs.display === "none" || cs.visibility === "hidden" || cs.visibility === "collapse") return false;
		if (parseFloat(cs.opacity) === 0) return false;
		if (el.closest("[hidden],[aria-hidden='true'],[inert]")) return false;
		const r = el.getBoundingClientRect();
		return r.width > 0 && r.height > 0;
	};
	const ownText = (el) => {
		let t = "";
		for (const n of el.childNodes) if (n.nodeType === 3) t += n.nodeValue;
		return t.replace(/\s+/g, " ").trim();
	};
	const text = (el) => (el.innerText || el.textContent || "").replace(/\s+/g, " ").trim();
	const add = (detector, severity, message, el, rect) =>
		out.push({ detector, severity, message, ...(rect || el ? { rect: rect || box(el) } : {}), ...(el ? { selector: sel(el) } : {}) });

	const all = [...document.body.querySelectorAll("*")].filter((el) => !/^(SCRIPT|STYLE|LINK|META|NOSCRIPT|TEMPLATE|HEAD)$/.test(el.tagName));
	const vis = all.filter(visible);

	// What counts as content: an element that draws something itself, a run of its own text, media,
	// a form control, or a painted background/border. A bare wrapper (`<div>` around other things)
	// is not content: it must not hide an empty stretch it merely spans.
	const MEDIA = /^(IMG|VIDEO|CANVAS|SVG|PICTURE|IFRAME|OBJECT|EMBED|INPUT|SELECT|TEXTAREA|BUTTON|PROGRESS|METER|HR)$/;
	const paints = (el) => {
		const cs = getComputedStyle(el);
		const bg = cs.backgroundColor;
		const painted = bg && bg !== "transparent" && !/rgba\(\s*\d+,\s*\d+,\s*\d+,\s*0\s*\)/.test(bg);
		return (
			painted ||
			(cs.backgroundImage && cs.backgroundImage !== "none") ||
			["Top", "Right", "Bottom", "Left"].some((s) => parseFloat(cs[`border${s}Width`]) > 0 && cs[`border${s}Style`] !== "none")
		);
	};
	const isContent = (el) => MEDIA.test(el.tagName.toUpperCase()) || ownText(el) !== "" || (el.children.length === 0 && paints(el));

	// ── empty-band ────────────────────────────────────────────────────────────────────────────────
	// Heuristic: collect the vertical spans of every content element, merge them, and the longest
	// uncovered stretch between 0 and the page height is the band. A tall painted section with
	// nothing in it still counts as empty (the paint is the hole's wallpaper): only a section whose
	// *children* draw something covers it. Fixed-position bars are ignored (they sit over the page).
	if (want("empty-band")) {
		const limit = vw <= o.mobileMaxWidth ? o.emptyBandPx : Math.max(o.emptyBandPx, Math.round(o.emptyBandScreens * vh));
		const spans = [];
		for (const el of vis) {
			if (!isContent(el)) continue;
			if (getComputedStyle(el).position === "fixed") continue;
			const r = box(el);
			if (r.height <= 0) continue;
			// A box the section paints but nothing else fills is not content on its own when it is huge:
			// a leaf that only paints and spans over a third of the page is the hole's own background.
			if (!MEDIA.test(el.tagName.toUpperCase()) && ownText(el) === "" && r.height > pageH / 3) continue;
			spans.push([r.y, r.y + r.height]);
		}
		spans.sort((a, b) => a[0] - b[0]);
		let cursor = 0;
		let best = { from: 0, to: 0 };
		for (const [a, b] of spans) {
			if (a - cursor > best.to - best.from) best = { from: cursor, to: a };
			cursor = Math.max(cursor, b);
		}
		if (pageH - cursor > best.to - best.from) best = { from: cursor, to: pageH };
		const len = best.to - best.from;
		// The trailing stretch is only a band when content follows nothing at all: a short page that
		// simply ends is not "an empty band", the page's height is what the viewport gives it.
		const trailing = best.to === pageH && best.from === cursor && cursor <= vh;
		if (len >= limit && !trailing) {
			add(
				"empty-band",
				"error",
				`${Math.round(len)} px of the page (from y=${Math.round(best.from)} to y=${Math.round(best.to)}) hold no content; the limit is ${limit} px at ${vw} px wide`,
				null,
				{ x: 0, y: best.from, width: vw, height: len },
			);
		}
	}

	// ── gutter ────────────────────────────────────────────────────────────────────────────────────
	// Heuristic (mobile widths only): text, controls and media must stay `gutterPx` away from the
	// viewport edge. A box that spans (nearly) the whole width is a full-bleed section, which is
	// fine as long as its own text is not glued to the edge: for those, the text's own box is what
	// is measured (through the `ownText` leaves), the bleed container is skipped.
	if (want("gutter") && vw <= o.mobileMaxWidth) {
		const seen = new Set();
		for (const el of vis) {
			const t = MEDIA.test(el.tagName.toUpperCase()) ? el.tagName.toUpperCase() : "";
			if (!(ownText(el) !== "" || (t && t !== "HR" && t !== "PROGRESS"))) continue;
			const r = el.getBoundingClientRect();
			if (r.width >= vw * 0.96 && (!ownText(el) || r.left <= 0.5)) {
				// A full-bleed image or video is allowed to touch the edges; so is a full-width text block
				// only if it has padding: text then starts inside it. Measure the text range instead.
				if (t) continue;
			}
			let left = r.left;
			let right = vw - r.right;
			if (ownText(el)) {
				// The text's real box, not the (padded or not) element's.
				const range = document.createRange();
				range.selectNodeContents(el);
				const rr = [...range.getClientRects()].filter((x) => x.width > 0);
				if (rr.length) {
					left = Math.min(...rr.map((x) => x.left));
					right = vw - Math.max(...rr.map((x) => x.right));
				}
			}
			if (r.right < 0 || r.left > vw) continue; // off-screen on purpose (a carousel, a drawer)
			const edge = Math.min(left, right);
			if (edge < o.gutterPx && !seen.has(el)) {
				seen.add(el);
				add(
					"gutter",
					"error",
					`${edge.toFixed(1)} px from the ${left <= right ? "left" : "right"} edge of a ${vw} px viewport (at least ${o.gutterPx} px)`,
					el,
				);
			}
		}
	}

	// ── sibling-overlap ───────────────────────────────────────────────────────────────────────────
	// Heuristic: two element children of the same parent, both in normal flow (not absolute/fixed/
	// sticky, not floated, not dialogs, tooltips, menus), that share an area of at least
	// `overlapMinPx2` AND `overlapMinShare` of the smaller box. Negative margins and transforms are
	// how a layout bug produces this; deliberate stacking uses positioning, which is skipped.
	if (want("sibling-overlap")) {
		const OVERLAY = "[role=dialog],[role=alertdialog],[role=tooltip],[role=menu],[role=listbox],[popover],dialog";
		const flows = (el) => {
			const cs = getComputedStyle(el);
			return !["absolute", "fixed", "sticky"].includes(cs.position) && cs.float === "none" && !el.matches(OVERLAY) && visible(el);
		};
		const parents = new Set(vis.map((e) => e.parentElement).filter(Boolean));
		for (const p of parents) {
			const kids = [...p.children].filter(flows);
			for (let i = 0; i < kids.length; i++) {
				for (let j = i + 1; j < kids.length; j++) {
					const a = kids[i].getBoundingClientRect();
					const b = kids[j].getBoundingClientRect();
					const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
					const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
					if (w <= 0 || h <= 0) continue;
					const area = w * h;
					const small = Math.min(a.width * a.height, b.width * b.height);
					if (area >= o.overlapMinPx2 && small > 0 && area / small >= o.overlapMinShare) {
						add(
							"sibling-overlap",
							"error",
							`${sel(kids[i])} and ${sel(kids[j])} overlap by ${Math.round(w)}x${Math.round(h)} px (${Math.round((100 * area) / small)}% of the smaller)`,
							kids[j],
							{ x: Math.max(a.left, b.left) + sx, y: Math.max(a.top, b.top) + sy, width: w, height: h },
						);
					}
				}
			}
		}
	}

	// ── raw-text ──────────────────────────────────────────────────────────────────────────────────
	// Heuristic: in the places a person reads (headings, buttons, links styled as buttons, labels,
	// tabs, menu items, table headers, legends, summaries), a text that is a bare identifier:
	// snake_case (`user_profile`), kebab-case with 2+ lowercase words (`user-profile-card`), a UUID,
	// or a long hex/numeric id. Real prose, single words, and anything with a space are left alone.
	if (want("raw-text")) {
		const PLACES = "h1,h2,h3,h4,h5,h6,button,[role=button],[role=tab],[role=menuitem],label,th,legend,summary,[role=heading]";
		const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
		const raw = (s) =>
			UUID.test(s) ||
			/^[a-z][a-z0-9]*(_[a-z0-9]+)+$/.test(s) ||
			/^[a-z][a-z0-9]*(-[a-z0-9]+){1,}$/.test(s) ||
			/^[0-9a-f]{16,}$/i.test(s);
		for (const el of document.body.querySelectorAll(PLACES)) {
			if (!visible(el)) continue;
			const t = text(el);
			if (t && t.length <= 80 && raw(t)) {
				add("raw-text", "error", `"${t}" looks like an internal identifier, not text for a person`, el);
			}
		}
	}

	// ── action-hierarchy ──────────────────────────────────────────────────────────────────────────
	// Heuristic: a visible, enabled control is a PRIMARY action when it says so: `data-variant|
	// data-kind|data-intent|data-emphasis|data-priority="primary"`, or a class token `primary`,
	// `cta`, `btn-primary`, `button-primary`, `is-primary`..., or it is a `type=submit` button
	// with none of those attributes. More than one primary in the same GROUP is the error: the
	// nearest `form`, `[role=group|toolbar|dialog]`, `dialog`, `.actions`, `.button-group`,
	// `.btn-group`, `footer`, `nav`, `section`, `article` or `header` ancestor, else the parent.
	if (want("action-hierarchy")) {
		const CTRL = "button,[role=button],a[class],input[type=submit],input[type=button]";
		const GROUP = "form,[role=group],[role=toolbar],dialog,[role=dialog],.actions,.button-group,.btn-group,footer,nav,section,article,header";
		const marked = (el) =>
			["data-variant", "data-kind", "data-intent", "data-emphasis", "data-priority"].some((a) => (el.getAttribute(a) || "").toLowerCase() === "primary") ||
			[...el.classList].some((c) => /(^|[-_])(primary|cta)$/i.test(c) || /^(primary|cta)([-_]|$)/i.test(c));
		const isPrimary = (el) => marked(el) || (el.matches("button[type=submit],input[type=submit]") && !el.closest(".secondary,[data-variant=secondary]") && !el.hasAttribute("data-variant") && !el.hasAttribute("data-kind"));
		const groups = new Map();
		for (const el of document.body.querySelectorAll(CTRL)) {
			if (!visible(el) || el.disabled || el.getAttribute("aria-disabled") === "true") continue;
			if (!isPrimary(el)) continue;
			const g = el.parentElement ? el.parentElement.closest(GROUP) || el.parentElement : document.body;
			(groups.get(g) || groups.set(g, []).get(g)).push(el);
		}
		for (const [g, els] of groups) {
			if (els.length > 1) {
				const names = els.map((e) => `"${text(e) || e.getAttribute("aria-label") || e.value || sel(e)}"`).join(", ");
				add("action-hierarchy", "error", `${els.length} primary actions in one group (${sel(g)}): ${names}. One group has one primary`, els[1]);
			}
		}
	}

	// ── focus-after-tap: the list of what to activate (the key press is Playwright's) ─────────────
	const taps = [];
	if (want("focus-after-tap")) {
		// Controls that DO something when activated; a text field has nothing to activate.
		const FOCUSABLE = "button,a[href],summary,[tabindex]:not([tabindex='-1']):not(input):not(select):not(textarea),[role=button],[role=tab],[role=menuitem]";
		for (const el of document.body.querySelectorAll(FOCUSABLE)) {
			if (taps.length >= o.tapMax) break;
			if (!visible(el) || el.disabled || el.getAttribute("aria-disabled") === "true") continue;
			if (el.matches("a[href]") && !/^(#|javascript:)/.test(el.getAttribute("href") || "")) continue; // would navigate away
			if (el.matches("input[type=submit],button[type=submit]")) continue; // would submit
			const r = el.getBoundingClientRect();
			if (r.bottom < 0 || r.top > vh || r.right < 0 || r.left > vw) continue;
			el.setAttribute("data-atlas-tap", String(taps.length));
			taps.push({ i: taps.length, x: r.left + r.width / 2, y: r.top + r.height / 2, selector: sel(el), label: text(el) || el.getAttribute("aria-label") || el.tagName.toLowerCase() });
		}
	}
	return { findings: out, taps };
}

/** What `inPage` is told to look at; `opts` overrides the defaults. */
function options(opts) {
	const { only, ...rest } = opts || {};
	return [only && only.length ? only : null, { ...DEFAULTS, ...rest }];
}

/**
 * Run the detectors on a Playwright page and return their findings, errors first.
 * `opts.only` limits it to some of [`DETECTORS`](#); any threshold of [`DEFAULTS`](#) may be given.
 */
export async function detect(page, opts) {
	const [only, o] = options(opts);
	const want = (name) => !only || only.includes(name);
	for (const name of only || []) {
		if (!DETECTORS.includes(name)) throw new Error(`unknown detector: ${name}`);
	}
	await page.evaluate(() => window.scrollTo(0, 0));
	const { findings, taps } = await page.evaluate(inPage, [only, o]);
	const out = [...findings];

	if (want("cls")) {
		const s = await page.evaluate(() => window.__atlasShifts || null);
		if (s && s.total >= o.clsWarn) {
			const worst = [...s.entries].sort((a, b) => b.value - a.value)[0];
			out.push({
				detector: "cls",
				severity: s.total >= o.clsError ? "error" : "warn",
				message: `layout shifted ${s.total.toFixed(3)} (limit ${o.clsError}); the biggest shift came from ${worst && worst.selector ? worst.selector : "an element the browser did not name"}`,
				...(worst && worst.rect ? { rect: worst.rect } : {}),
				...(worst && worst.selector ? { selector: worst.selector } : {}),
			});
		}
	}

	if (want("focus-after-tap")) {
		// The rule (ui#1229): the control that was activated must not lose focus to the page body.
		// Engines differ on what a pointer press does to focus (Safari and WebKit blur the active
		// element and never focus a button or a link), which would be the engine's doing, not the
		// page's. So the control is focused the way keyboard and screen reader users reach it and
		// activated with the key that activates it (Enter; Space for a button-like role): what the
		// page does on activation is then judged identically on every engine. Focus moving INTO what
		// the activation opened (a dialog, a field) is fine, and so is a control it removed.
		for (const t of taps) {
			await page.evaluate((i) => document.querySelector(`[data-atlas-tap="${i}"]`)?.focus(), t.i);
			await page.keyboard.press("Enter");
			const after = await page.evaluate((i) => {
				const a = document.activeElement;
				return { body: !a || a === document.body || a === document.documentElement, gone: !document.querySelector(`[data-atlas-tap="${i}"]`) };
			}, t.i);
			if (after.body && !after.gone) {
				out.push({
					detector: "focus-after-tap",
					severity: "error",
					message: `after activating "${t.label}" the focus is on the page body: keyboard and screen reader users lose their place`,
					selector: t.selector,
					rect: { x: t.x - 1, y: t.y - 1, width: 2, height: 2 },
				});
			}
		}
		await page.evaluate(() => document.querySelectorAll("[data-atlas-tap]").forEach((e) => e.removeAttribute("data-atlas-tap")));
	}

	const rank = { error: 0, warn: 1 };
	return out.sort((a, b) => rank[a.severity] - rank[b.severity]);
}
