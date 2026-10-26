// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Glob } from "bun";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Source-level budget for compositor-only motion (ui-smooth study, C): a looping or transitioned `box-shadow`/`width`
// repaints or relayouts every frame for as long as the element lives (a status dot per running project, a glow on the
// sidebar footer, an install progress bar). Motion belongs to `transform` and `opacity`, and a layer is promoted only
// while it moves, never permanently.

const root = resolve(import.meta.dir, "..");
const files = [...new Glob("**/*.{svelte,css}").scanSync({ cwd: root })].map((f) => [
	f,
	readFileSync(resolve(root, f), "utf8"),
]);

/** `@keyframes name { ... }` bodies, balanced. */
function keyframes(css: string) {
	const out: { name: string; body: string }[] = [];
	for (const m of css.matchAll(/@keyframes\s+([\w-]+)\s*\{/g)) {
		let depth = 1;
		let i = m.index + m[0].length;
		const from = i;
		while (i < css.length && depth) ((depth += css[i] === "{" ? 1 : css[i] === "}" ? -1 : 0), i++);
		out.push({ name: m[1], body: css.slice(from, i - 1) });
	}
	return out;
}

test("no @keyframes animates box-shadow or a layout property", () => {
	const bad: string[] = [];
	for (const [f, css] of files)
		for (const k of keyframes(css))
			if (/\b(box-shadow|filter|width|height|top|left|right|bottom|margin|padding)\s*:/.test(k.body))
				bad.push(`${f}: @keyframes ${k.name}`);
	expect(bad).toEqual([]);
});

test("the status dot, the install bar and the state timeline do not transition box-shadow or width", () => {
	const covered = ["app.css", "lib/panel/StateTimeline.svelte", "lib/components/WhatsNew.svelte"];
	const bad: string[] = [];
	for (const [f, css] of files)
		if (covered.includes(f))
			for (const m of css.matchAll(/transition\s*:([^;]+);/g))
				if (
					/\b(box-shadow|width|height|padding)\b/.test(m[1]) &&
					!/sidebar-slot/.test(css.slice(m.index - 120, m.index))
				)
					bad.push(`${f}: transition:${m[1].trim().replace(/\s+/g, " ")}`);
	// `.sidebar-slot` slides the pinned sidebar's column and is the one deliberate width transition (content glides with it).
	expect(bad).toEqual([]);
});

test("viewers promote a layer only while it moves (will-change sits under an interaction class)", () => {
	const bad: string[] = [];
	for (const [f, css] of files)
		if (/viewers\//.test(f))
			// Every `will-change` declaration's selector must name the state that is moving (`.dragging`, `.pinching`).
			for (const m of css.matchAll(/([^{}]+)\{[^{}]*will-change\s*:/g))
				if (!/\.(dragging|pinching)\b/.test(m[1])) bad.push(`${f}: ${m[1].trim()}`);
	expect(bad).toEqual([]);
});
