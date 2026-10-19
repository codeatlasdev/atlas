// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

// Kit 4.2.0 (f889334): every backdrop blur comes from ONE place. Components no longer declare `backdrop-filter`; they set
// `data-ca-blur="<role>"` and `styles/surface.css` paints `var(--ca-blur-role)` from the tokens `html[data-style]` rewrites.
// The PR #34 rule still holds: the window is transparent and the pager translates content under floating surfaces, so a
// backdrop blur over swiped content re-renders every frame (tauri-apps/tauri#15471). Atlas runs `data-style="solid"`, where
// every tier resolves to `none`; the roles that read a LAYER token (`overlay`, `tooltip`, scrim) are the ones app.css zeroes.
// This reads the real kit CSS and Atlas's app.css, so a kit upgrade that adds a role, or a token Atlas stops zeroing, goes red.
const root = dirname(createRequire(import.meta.url).resolve("@codeatlasdev/ui/package.json"));
const surface = readFileSync(join(root, "dist/styles/surface.css"), "utf8");
const materials = readFileSync(join(root, "dist/styles/materials.css"), "utf8");
const app = readFileSync(new URL("../../app.css", import.meta.url), "utf8");

/** role -> the token its `[data-ca-blur="role"]` rule reads (`regular` shares a rule with the glass buttons, so it is named). */
const roles = Object.fromEntries([
	...[...surface.matchAll(/\[data-ca-blur="([\w-]+)"\]\s*\{\s*--ca-blur-role:\s*var\((--[\w-]+)\)/g)].map(
		(m) => [m[1], m[2]],
	),
	["regular", surface.match(/\.ca-icon-btn--glass\s*\{\s*--ca-blur-role:\s*var\((--[\w-]+)\)/)?.[1]],
]);

/** Declarations of `--token: value` inside the `[data-style="<style>"]` block of materials.css. */
function styleBlock(style: string) {
	const m = materials.match(new RegExp(`\\[data-style="${style}"\\]\\s*\\{([^}]*)\\}`));
	return Object.fromEntries(
		[...(m?.[1] ?? "").matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((d) => [d[1], d[2].trim()]),
	);
}
/** Atlas's unlayered `:root` declarations of blur tokens (they win over the kit's layers). */
const atlasRoot = Object.fromEntries(
	[...app.matchAll(/:root\s*\{([^}]*)\}/g)].flatMap((m) =>
		[...m[1].matchAll(/(--ca-[\w-]*blur):\s*([^;]+);/g)].map((d) => [d[1], d[2].trim()]),
	),
);
const solid = styleBlock("solid");
/** The `:root` indirections (`--ca-mat-thin-filter: var(--ca-blur-thin)`), which `data-style` does not rewrite. */
const base = Object.fromEntries(
	[...materials.matchAll(/(--ca-mat-[\w-]*filter):\s*(var\(--[\w-]+\));/g)].map((d) => [d[1], d[2]]),
);

/** The value a blur token has in Atlas: its own override, else what `data-style="solid"` sets, following `var()` links. */
function resolved(token: string): string | undefined {
	const v = atlasRoot[token] ?? solid[token] ?? base[token];
	const link = v?.match(/^var\((--[\w-]+)\)$/);
	return link ? resolved(link[1]) : v;
}

test("the kit's blur roles were found (guards the parse, not the blur)", () => {
	expect(Object.keys(roles)).toEqual(
		expect.arrayContaining([
			"ultra-thin",
			"thin",
			"thick",
			"chrome",
			"overlay",
			"tooltip",
			"chrome-layer",
			"regular",
		]),
	);
});

test("under Atlas's tokens every kit blur role resolves to none (no blur over swiped content)", () => {
	// `regular` and `mat-*` read tier tokens that solid sets to none; every layer token is zeroed or solid.
	const blurred = Object.entries(roles)
		.map(([role, token]) => [role, token, resolved(token)])
		.filter(([, , v]) => v !== "none");
	expect(blurred).toEqual([]);
});

test("the layer tokens Atlas owns are zeroed in app.css, not left to the style's value", () => {
	for (const t of ["--ca-scrim-blur", "--ca-overlay-blur", "--ca-tooltip-blur"])
		expect(atlasRoot[t]).toBe("none");
});

test("the scrim is painted from --ca-scrim-blur (the one token a modal's ::backdrop reads)", () => {
	expect(surface).toMatch(/dialog\.ca-overlay::backdrop\s*\{[^}]*backdrop-filter:\s*var\(--ca-scrim-blur\)/);
});

// The one blur the roles do not cover: an explicit material ignores `data-style` (`.ca-surface[data-material="glass"]` is
// glass under every style), so a `Card material="glass"` over the pager blurs even though the app is solid.
test("no Atlas component asks the kit for an explicit glass/frosted/liquid material (it ignores data-style)", () => {
	const { readdirSync, statSync } = require("node:fs");
	const hits: string[] = [];
	const walk = (dir: string) => {
		for (const name of readdirSync(dir)) {
			const path = join(dir, name);
			if (statSync(path).isDirectory()) walk(path);
			else if (
				/\.svelte$/.test(name) &&
				/material=["'{](?:glass|frosted|liquid)/.test(readFileSync(path, "utf8"))
			)
				hits.push(path.replace(/.*\/src\//, "src/"));
		}
	};
	walk(new URL("../..", import.meta.url).pathname);
	expect(hits).toEqual([]);
});
