// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { compile } from "svelte/compiler";

// Incident (#739): on a Linux tiling WM (`html.custom-chrome.tiling-wm`, --caption-controls: 0px) the title rows'
// right padding was `var(--caption-controls)` = 0, which REPLACED their own padding, so the Start/Stop button, the
// Agents header, the loose-folder header and the right sidebar's tab row touched the window edge. The band reserved for
// the caption buttons must only ever widen the row's own padding, never shrink it below it.
//
// The CSS under test is what the components really ship: each `.svelte` file goes through the Svelte compiler and its
// scoped output is read back. happy-dom cannot evaluate `max()`/`calc()` with `var()`, so the declared value is resolved
// here for the two window kinds.

const root = resolve(import.meta.dir, "..");
const read = (f: string) => readFileSync(resolve(root, f), "utf8");

/** `selector { declarations }` pairs of a flat stylesheet (the components use no @media around these rows). */
function rules(css: string) {
	return [...css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
		selector: m[1].trim(),
		decls: Object.fromEntries(
			m[2]
				.split(";")
				.map((d) => d.split(/:(.*)/s))
				.filter((d) => d[1] !== undefined)
				.map((d) => [d[0].trim(), d[1].trim()]),
		),
	}));
}

/** A CSS length expression (px, calc, max, min, var) → px. */
function px(expr: string, vars: Record<string, string>): number {
	let e = expr;
	for (let i = 0; i < 5 && /var\(/.test(e); i++)
		e = e.replace(/var\((--[\w-]+)\)/g, (_, n) => {
			if (!(n in vars)) throw new Error(`unknown ${n} in ${expr}`);
			return vars[n];
		});
	e = e
		.replace(/(\d*\.?\d+)px/g, "$1")
		.replace(/\bcalc\(/g, "(")
		.replace(/\bmax\(/g, "Math.max(")
		.replace(/\bmin\(/g, "Math.min(");
	if (!/^[\d\s.+\-*/(),Math.maxin]*$/.test(e)) throw new Error(`cannot evaluate ${expr}`);
	return new Function(`return ${e}`)();
}

const app = rules(read("app.css"));
const caption = (windowKind: "windows" | "tiling") => {
	const base = app.find((r) => r.selector === "html.custom-chrome")!.decls["--caption-controls"];
	const tiling = app.find((r) => r.selector === "html.custom-chrome.tiling-wm")!.decls["--caption-controls"];
	return { "--caption-controls": windowKind === "tiling" ? tiling : base };
};

// [file, the row's own element selector (before Svelte's scope class)]
const ROWS = [
	["routes/(app)/ProjectHeader.svelte", "header"],
	["lib/components/AgentsView.svelte", "header"],
	["lib/components/LooseView.svelte", "header"],
	["lib/components/RightSidebar.svelte", ".tabs"],
] as const;

/** The row's padding-right with no caption band at all (macOS, or any window without custom chrome), in px. */
function ownPadding(css: ReturnType<typeof rules>, el: string): number {
	let right = 0;
	for (const r of css) {
		if (r.selector.replace(/\.svelte-\w+/g, "") !== el) continue;
		const sh = r.decls.padding?.split(/\s+/).map((v) => px(v, {}));
		if (sh) right = sh.length === 1 ? sh[0] : sh.length === 2 ? sh[1] : sh[1];
		if (r.decls["padding-right"]) right = px(r.decls["padding-right"], {});
	}
	return right;
}

/** The padding-right a custom-chrome window gives the row: the chrome rule, else the row's own. */
function chromePadding(css: ReturnType<typeof rules>, el: string, vars: Record<string, string>): number {
	let right = ownPadding(css, el);
	for (const r of css) {
		const m = r.selector.replace(/\.svelte-\w+/g, "").match(/^html\.custom-chrome\s+(.+)$/);
		if (m?.[1] === el && r.decls["padding-right"]) right = px(r.decls["padding-right"], vars);
	}
	return right;
}

for (const [file, el] of ROWS) {
	const css = rules(compile(read(file), { filename: file, generate: "client" }).css?.code ?? "");

	test(`${file}: the ${el} row keeps its own right padding on a tiling WM (no caption buttons)`, () => {
		const own = ownPadding(css, el);
		expect(own).toBeGreaterThan(0);
		expect(chromePadding(css, el, caption("tiling"))).toBe(own);
	});

	test(`${file}: the ${el} row clears the caption buttons where they exist (Windows, floating Linux)`, () => {
		const band = px(caption("windows")["--caption-controls"], {});
		expect(chromePadding(css, el, caption("windows"))).toBeGreaterThanOrEqual(band);
	});
}

test("the evaluator resolves what the rows use (guards the test itself)", () => {
	const v = { "--caption-controls": "0px" };
	expect(px("max(10px, var(--caption-controls))", v)).toBe(10);
	expect(px("max(10px, var(--caption-controls))", { "--caption-controls": "138px" })).toBe(138);
	expect(px("calc(10px + var(--caption-controls))", { "--caption-controls": "138px" })).toBe(148);
	expect(px("var(--caption-controls)", v)).toBe(0);
});
