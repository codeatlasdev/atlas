// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { suite } from "./traceSuite.ts";
import { summary, synth } from "./trace.ts";

const dir = join(import.meta.dir, "__fixtures__/traces");

test("the fixtures are exactly what the synthesizer produces (regenerate with bun scripts/gesture-fixtures.ts)", () => {
	const onDisk = readdirSync(dir).filter((f) => f.startsWith("recorded-") === false && f.endsWith(".json"));
	expect(onDisk.sort()).toEqual(
		suite()
			.map((t) => `${t.name}.json`)
			.sort(),
	);
	for (const t of suite()) {
		expect(JSON.parse(readFileSync(join(dir, `${t.name}.json`), "utf8"))).toEqual(
			JSON.parse(JSON.stringify(t)),
		);
	}
});

test("every trace is a valid stream: time never goes back, phases follow the trackpad's grammar", () => {
	for (const t of suite()) {
		let last = -1;
		let down = false;
		for (const e of t.events) {
			expect(e.t).toBeGreaterThanOrEqual(last);
			last = e.t;
			if (e.phase === "began") down = true;
			if (e.phase === "changed") expect(down).toBe(true);
			if (e.phase === "ended" || e.phase === "cancelled") down = false;
			// momentum events only exist while the fingers are up
			if (e.momentum !== "none") expect(down).toBe(false);
		}
	}
});

test("the synthesizer delivers the travel it was asked for, deterministically", () => {
	const a = synth({ x: 1, y: 1, travel: -200, ms: 300, seed: 3 });
	const b = synth({ x: 1, y: 1, travel: -200, ms: 300, seed: 3 });
	expect(a).toEqual(b);
	expect(Math.abs(summary(a).dx + 200)).toBeLessThan(2);
	expect(synth({ x: 1, y: 1, travel: -200, ms: 300, seed: 4 })).not.toEqual(a);
});

test("the suite covers every shape the engine has to get right", () => {
	const names = suite().map((t) => t.name);
	for (const need of [
		"flick-next",
		"flick-prev",
		"slow-drag-past-half",
		"slow-drag-short-cancel",
		"reversal-commit-back",
		"reversal-cancel",
		"two-quick-swipes",
		"momentum-tail-never-chains",
		"vertical-scroll",
		"diagonal-vertical-dominant",
		"tiny-jitter",
		"first-space-rubber",
		"last-space-rubber",
		"swipe-back-during-settle",
		"over-native-view",
		"tab-flick-next",
		"tab-many-tabs",
		"wheel-mouse",
		"editor-scroll-owned",
		"editor-scroll-back-owned",
		"editor-at-edge-hands-over",
	])
		expect(names).toContain(need);
});
