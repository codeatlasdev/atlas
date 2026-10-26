// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { compile } from "svelte/compiler";

// Incident (#121): flipping a layer's role restyled every descendant (57 ms per 23k-node layer, 286 ms for a switch with
// a 400-line diff in each of five spaces). `inert` and `pointer-events` are INHERITED: a change on the layer root makes
// Chromium recompute the style of the whole subtree, whatever the selectors are (there is no descendant selector on the
// role; bisected: with every declaration of the `near` rule pinned inline the flip costs 0 ms, unpinning only
// `pointer-events` brings back 56 ms, and `inert` costs the same on the root or on a wrapper of it).
// These tests read the real component sources: the role may drive only properties that do not inherit.
const routes = join(import.meta.dir, "../../routes/(app)");
const read = (f: string) => readFileSync(join(routes, f), "utf8");
const layerSrc = read("SpaceLayer.svelte");
const pagerSrc = read("SpacePager.svelte");
const css = compile(layerSrc, { filename: "SpaceLayer.svelte", generate: "client" }).css?.code ?? "";

/** Declarations of every rule whose selector depends on the role (`data-role`, `data-side`, `[inert]`, `:has`). */
function roleDeclarations(code: string): { selector: string; prop: string }[] {
	const out: { selector: string; prop: string }[] = [];
	for (const m of code.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
		const selector = m[1].trim();
		if (!/data-role|data-side|inert|aria-hidden/.test(selector)) continue;
		for (const d of m[2].matchAll(/([\w-]+)\s*:/g)) out.push({ selector, prop: d[1] });
	}
	return out;
}

/** CSS properties that inherit: a change on an ancestor recomputes the style of every descendant. */
const INHERITED: Record<string, true> = {
	"pointer-events": true,
	visibility: true,
	cursor: true,
	color: true,
	"user-select": true,
	"-webkit-user-select": true,
	font: true,
	"font-family": true,
	"font-size": true,
	"font-weight": true,
	"line-height": true,
	"letter-spacing": true,
	"white-space": true,
	"text-align": true,
	direction: true,
	"caret-color": true,
	"accent-color": true,
	"color-scheme": true,
	quotes: true,
	"-webkit-user-modify": true,
	"-webkit-text-fill-color": true,
};

test("the role drives only properties that do not inherit", () => {
	const decls = roleDeclarations(css);
	expect(decls.length).toBeGreaterThan(0); // the rules exist: this is not vacuous
	const bad = decls.filter((d) => INHERITED[d.prop] || d.prop.startsWith("--"));
	expect(bad).toEqual([]);
});

test("no role rule mentions `pointer-events` (`inert` is gone, so is the shield it made redundant)", () => {
	expect(css).not.toMatch(/pointer-events/);
});

test("the role never reaches an inline style or an attribute that inherits", () => {
	expect(layerSrc).not.toMatch(/\binert\b\s*=/);
	expect(layerSrc).not.toMatch(/style:(pointer-events|visibility|cursor|color|user-select)\b/);
	expect(pagerSrc).not.toMatch(/\binert\b\s*=/);
});

test("`contain` on a layer never includes `style` (style containment toggles cost a subtree restyle: 7 ms per layer)", () => {
	for (const m of css.matchAll(/contain\s*:\s*([^;]+);/g))
		expect(m[1]).not.toMatch(/\bstyle\b|\bstrict\b|\bcontent\b/);
});

test("the four `--space-*` vars are constant for a layer's lifetime (they inherit): none is written from the role", () => {
	// A layer carries its own, `shell.layerVars(id)` of ITS space. The role (`current`) must not pick between sets.
	const root = layerSrc.slice(layerSrc.indexOf('<div\n\tclass="space-layer"'), layerSrc.indexOf("</div>"));
	for (const v of ["--space-accent", "--space-accent-text", "--space-on-accent", "--space-tint"])
		expect(root).toMatch(new RegExp(`style:${v}=\\{vars\\.`));
	expect(root).not.toMatch(/current\s*\?[^}]*vars/);
});

test("the pager renders every layer through SpaceLayer, so the rule above covers all of them", () => {
	expect(pagerSrc).toContain("<SpaceLayer");
	expect(pagerSrc).not.toMatch(/class="space-layer"/);
	expect(dirname(join(routes, "SpaceLayer.svelte"))).toBe(routes);
});
