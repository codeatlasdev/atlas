// Atlas Crew capture runner. Embedded in atlas (`crew/capture/mod.rs`) and written next to the pinned
// `playwright` package on the crew server by `atlas crew browsers install`.
//
// stdin : one JSON spec {url, widths[], themes[], engines[], slow_network, full_page, steps[], dom?, checks?, proxy?, out_dir}
// With `proxy` (an isolated crewmate's capture) the browsers connect ONLY through that proxy, which refuses private
// networks and other crewmates' servers for every request of the page and every hop of a redirect (`crew/capture/guard.rs`).
// Each engine first proves it really sends loopback through it (`proxyIsUsed`); one that does not is not used.
// stdout: one JSON report {shots: [{engine, width, theme, path, dom?, findings?} | {engine, width, theme, error}]}
// With `dom: true` every cell also writes `<engine>-<width>px-<theme>.dom.json` next to the PNG: the
// element boxes of the page (see `collectDom`), which the `locate` tool ranks.
// With `checks: true` every cell also writes `<engine>-<width>px-<theme>.findings.json`: the UI
// detectors' findings (`detectors.mjs`, a temporary stand-in for ui#1290; an empty list is a pass).
// Exit 0 whenever a report was printed (a failed cell is an `error` entry, not a crash); the
// caller checks that every requested combination is present. Anything else goes to stderr.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createRequire } from "node:module";
import { createServer } from "node:http";
import { detect, observeShifts } from "./detectors.mjs";

// The package lives next to this file, installed by `npm install playwright@<pin>`.
const require = createRequire(import.meta.url);
const playwright = require("playwright");

const SLOW_DELAY_MS = 400; // per request: a poor mobile connection, the same on every engine
const NAV_TIMEOUT_MS = 30_000;
const STEP_TIMEOUT_MS = 10_000;

async function readStdin() {
	const chunks = [];
	for await (const c of process.stdin) chunks.push(c);
	return Buffer.concat(chunks).toString("utf8");
}

async function runStep(page, step) {
	switch (step.do) {
		case "click":
			return page.click(step.selector, { timeout: STEP_TIMEOUT_MS });
		case "fill":
			return page.fill(step.selector, step.text, { timeout: STEP_TIMEOUT_MS });
		case "press":
			return page.keyboard.press(step.key);
		case "wait_for":
			return page.waitForSelector(step.selector, { timeout: STEP_TIMEOUT_MS });
		case "wait_ms":
			return page.waitForTimeout(Math.min(step.ms, 10_000));
		default:
			throw new Error(`unknown step: ${step.do}`);
	}
}

/** Scroll to the end in viewport-sized jumps so lazy content loads, then back to the top. */
async function scrollThrough(page) {
	await page.evaluate(async () => {
		const step = Math.max(window.innerHeight, 300);
		for (let y = 0; y < document.documentElement.scrollHeight; y += step) {
			window.scrollTo(0, y);
			await new Promise((r) => setTimeout(r, 60));
		}
		window.scrollTo(0, 0);
	});
}

const DOM_MAX_ELEMENTS = 4000;
const DOM_TEXT_MAX = 120;

/**
 * The boxes of the page's visible elements, in CSS px from the top-left of the whole page. Runs in
 * the page; reads only (no clicks, no network). Elements with no box, `display: none` and
 * `visibility: hidden` ones are left out; `script`/`style` never appear.
 */
function collectDom([maxElements, textMax]) {
	const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "META", "LINK", "HEAD", "TITLE", "BR", "WBR", "PATH", "DEFS"]);
	const IMPLICIT = {
		A: (e) => (e.hasAttribute("href") ? "link" : null),
		BUTTON: () => "button",
		H1: () => "heading", H2: () => "heading", H3: () => "heading",
		H4: () => "heading", H5: () => "heading", H6: () => "heading",
		INPUT: (e) => ({ checkbox: "checkbox", radio: "radio", button: "button", submit: "button", range: "slider" })[e.type] || "textbox",
		TEXTAREA: () => "textbox",
		SELECT: () => "combobox",
		IMG: () => "img",
		NAV: () => "navigation",
		MAIN: () => "main",
		HEADER: () => "banner",
		FOOTER: () => "contentinfo",
		ASIDE: () => "complementary",
		FORM: () => "form",
		UL: () => "list", OL: () => "list", LI: () => "listitem",
		TABLE: () => "table",
		DIALOG: () => "dialog",
	};
	const clip = (s) => {
		const t = (s || "").replace(/\s+/g, " ").trim();
		return t.length > textMax ? t.slice(0, textMax) + "…" : t;
	};
	const cssEscape = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : s.replace(/[^a-zA-Z0-9_-]/g, "\\$&"));
	const unique = (sel) => {
		try {
			return document.querySelectorAll(sel).length === 1;
		} catch {
			return false;
		}
	};
	// The shortest selector that finds exactly this element and does not depend on generated class
	// names: data-testid, then id, then a chain of tag:nth-of-type up to the nearest anchor.
	const selectorOf = (el) => {
		const tid = el.getAttribute("data-testid");
		if (tid) {
			const s = `[data-testid="${tid.replace(/"/g, '\\"')}"]`;
			if (unique(s)) return s;
		}
		if (el.id) {
			const s = `#${cssEscape(el.id)}`;
			if (unique(s)) return s;
		}
		const parts = [];
		for (let cur = el; cur && cur.nodeType === 1 && cur !== document.documentElement; cur = cur.parentElement) {
			if (cur !== el && cur.id && unique(`#${cssEscape(cur.id)}`)) {
				parts.unshift(`#${cssEscape(cur.id)}`);
				break;
			}
			const tag = cur.tagName.toLowerCase();
			const same = cur.parentElement ? [...cur.parentElement.children].filter((c) => c.tagName === cur.tagName) : [cur];
			parts.unshift(same.length > 1 ? `${tag}:nth-of-type(${same.indexOf(cur) + 1})` : tag);
			if (cur === document.body) break;
		}
		return parts.join(" > ");
	};
	const nameOf = (el) => {
		const aria = el.getAttribute("aria-label");
		if (aria) return clip(aria);
		const by = el.getAttribute("aria-labelledby");
		if (by) {
			const t = by.split(/\s+/).map((id) => document.getElementById(id)?.textContent || "").join(" ");
			if (t.trim()) return clip(t);
		}
		if (el.tagName === "IMG") return clip(el.getAttribute("alt"));
		if (el.labels && el.labels.length) return clip(el.labels[0].textContent);
		if (["BUTTON", "A", "SUMMARY"].includes(el.tagName) || /^H[1-6]$/.test(el.tagName)) return clip(el.textContent);
		return clip(el.getAttribute("title") || el.getAttribute("placeholder"));
	};
	const ownText = (el) => {
		let t = "";
		for (const n of el.childNodes) if (n.nodeType === 3) t += n.textContent + " ";
		return clip(t);
	};
	const sx = window.scrollX, sy = window.scrollY;
	const out = [];
	const walk = (el, depth) => {
		if (out.length >= maxElements || SKIP.has(el.tagName) || el.namespaceURI !== "http://www.w3.org/1999/xhtml") return;
		const cs = getComputedStyle(el);
		if (cs.display === "none") return;
		const r = el.getBoundingClientRect();
		if (cs.visibility !== "hidden" && r.width > 0 && r.height > 0) {
			const explicit = el.getAttribute("role");
			const role = explicit || (IMPLICIT[el.tagName] ? IMPLICIT[el.tagName](el) : null);
			const data = {};
			for (const a of el.attributes) if (a.name.startsWith("data-") && a.name !== "data-testid") data[a.name.slice(5)] = clip(a.value);
			out.push({
				tag: el.tagName.toLowerCase(),
				role: role || null,
				name: nameOf(el) || null,
				text: ownText(el) || null,
				testid: el.getAttribute("data-testid") || null,
				data,
				selector: selectorOf(el),
				x: Math.round((r.left + sx) * 10) / 10,
				y: Math.round((r.top + sy) * 10) / 10,
				w: Math.round(r.width * 10) / 10,
				h: Math.round(r.height * 10) / 10,
				depth,
				main: el.tagName === "MAIN" || explicit === "main",
			});
		}
		for (const c of el.children) walk(c, depth + 1);
	};
	if (document.body) walk(document.body, 0);
	const de = document.documentElement;
	return { page_w: Math.max(de.scrollWidth, window.innerWidth), page_h: Math.max(de.scrollHeight, window.innerHeight), elements: out };
}

/**
 * Whether `browser` sends a request for a loopback address to its proxy and not straight out. A canary server on
 * 127.0.0.1 belongs to this process, not to the crewmate the guard serves, so the guard refuses it: the canary seeing
 * a connection means the engine bypassed the proxy for loopback (as a browser may by default), and nothing it loads
 * could be decided. Both spellings are tried: `localhost` is special-cased by some engines.
 */
async function proxyIsUsed(browser) {
	let hits = 0;
	const canary = createServer((_req, res) => {
		hits++;
		res.end("canary");
	});
	await new Promise((resolve) => canary.listen(0, "127.0.0.1", resolve));
	const { port } = canary.address();
	const context = await browser.newContext();
	try {
		const page = await context.newPage();
		for (const host of ["127.0.0.1", "localhost"]) {
			await page.goto(`http://${host}:${port}/`, { timeout: 8_000 }).catch(() => {});
		}
	} finally {
		await context.close().catch(() => {});
		await new Promise((resolve) => canary.close(resolve));
	}
	return hits === 0;
}

async function shot(browser, spec, engine, width, theme) {
	const base = { engine, width, theme };
	const context = await browser.newContext({
		viewport: { width, height: 900 },
		colorScheme: theme,
		reducedMotion: "reduce",
	});
	try {
		const page = await context.newPage();
		page.setDefaultNavigationTimeout(NAV_TIMEOUT_MS);
		// Before the first navigation, so the layout shifts of the load itself are recorded.
		if (spec.checks) await page.addInitScript(observeShifts);
		if (spec.slow_network) {
			await page.route("**/*", async (route) => {
				await new Promise((r) => setTimeout(r, SLOW_DELAY_MS));
				await route.continue();
			});
		}
		await page.goto(spec.url, { waitUntil: "load" });
		for (const step of spec.steps ?? []) await runStep(page, step);
		if (spec.full_page) await scrollThrough(page);
		const path = join(spec.out_dir, `${engine}-${width}px-${theme}.png`);
		await page.screenshot({ path, fullPage: !!spec.full_page });
		const out = { ...base, path };
		// After the screenshot: the page is where the picture shows it (scrolled back to the top), and
		// the detectors' key presses cannot show in it.
		if (spec.dom) {
			await page.evaluate(() => window.scrollTo(0, 0));
			const found = await page.evaluate(collectDom, [DOM_MAX_ELEMENTS, DOM_TEXT_MAX]);
			out.dom = join(spec.out_dir, `${engine}-${width}px-${theme}.dom.json`);
			writeFileSync(out.dom, JSON.stringify({ url: page.url(), width, ...found }));
		}
		if (spec.checks) {
			out.findings = join(spec.out_dir, `${engine}-${width}px-${theme}.findings.json`);
			writeFileSync(out.findings, JSON.stringify(await detect(page)));
		}
		return out;
	} catch (e) {
		return { ...base, error: String(e && e.message ? e.message : e).split("\n")[0] };
	} finally {
		await context.close().catch(() => {});
	}
}

const spec = JSON.parse(await readStdin());
mkdirSync(spec.out_dir, { recursive: true });
const shots = [];
for (const engine of spec.engines) {
	let browser;
	try {
		browser = await playwright[engine].launch(
			// `<-loopback>`: Chromium never sends loopback through a proxy otherwise.
			spec.proxy ? { proxy: { server: spec.proxy, bypass: "<-loopback>" } } : {},
		);
		if (spec.proxy && !(await proxyIsUsed(browser))) {
			await browser.close().catch(() => {});
			throw new Error(`${engine} does not send loopback addresses through the network guard, so nothing it loads can be checked`);
		}
	} catch (e) {
		const error = String(e && e.message ? e.message : e).split("\n")[0];
		for (const width of spec.widths)
			for (const theme of spec.themes) shots.push({ engine, width, theme, error });
		continue;
	}
	for (const width of spec.widths)
		for (const theme of spec.themes)
			shots.push(await shot(browser, spec, engine, width, theme));
	await browser.close().catch(() => {});
}
process.stdout.write(JSON.stringify({ shots }));
