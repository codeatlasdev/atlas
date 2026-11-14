// Atlas Crew capture runner. Embedded in atlas (`crew/capture/mod.rs`) and written next to the pinned
// `playwright` package on the crew server by `atlas crew browsers install`.
//
// stdin : one JSON spec {url, widths[], themes[], engines[], slow_network, full_page, steps[], out_dir}
// stdout: one JSON report {shots: [{engine, width, theme, path} | {engine, width, theme, error}]}
// Exit 0 whenever a report was printed (a failed cell is an `error` entry, not a crash); the
// caller checks that every requested combination is present. Anything else goes to stderr.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { createRequire } from "node:module";

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
		return { ...base, path };
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
		browser = await playwright[engine].launch();
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
