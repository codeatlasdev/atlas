// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Glob } from "bun";
import { readFileSync } from "node:fs";

// Regression: CrewPalette shipped `:global(.ca-dialog .ca-dialog__body) { padding: 0 }` unscoped. Svelte scopes nothing
// inside `:global()`, so once the palette's stylesheet loaded, EVERY dialog in the app (Error details, Settings, Add
// project…) lost its body padding. The kit already pads the body and `chromeless` removes it per dialog.
// A component may style a kit dialog only through a selector that starts with one of its own (scoped) classes, or a
// class it passes to the dialog itself; never through a bare `:global(.ca-dialog…)` that reaches all of them.
const BARE_KIT_DIALOG = /(^|[,{}])\s*:global\(\s*\.ca-dialog(?:__[\w-]+)?\b[^)]*\)/m;

function styleOf(src: string) {
	return [...src.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join("\n");
}

test("no component restyles every kit dialog through a bare :global(.ca-dialog…) selector", () => {
	const offenders: string[] = [];
	for (const f of new Glob("src/**/*.svelte").scanSync(".")) {
		const css = styleOf(readFileSync(f, "utf8"));
		for (const sel of css.split(/\{[^{}]*\}/)) {
			const m = sel.match(BARE_KIT_DIALOG);
			// `:global(.ca-dialog.settings-dialog)` names one dialog by its own class: allowed.
			if (m && !/\.ca-dialog\.[\w-]+/.test(m[0])) offenders.push(`${f}: ${sel.trim()}`);
		}
	}
	expect(offenders).toEqual([]);
});

test("the detector catches the exact selector that shipped", () => {
	const bad = "\t:global(.ca-dialog .ca-dialog__body) {\n\t\tpadding: 0;\n\t}";
	expect(bad.split(/\{[^{}]*\}/)[0].match(BARE_KIT_DIALOG)).not.toBeNull();
	expect(":global(.ca-dialog.settings-dialog)".match(BARE_KIT_DIALOG)?.[0]).toMatch(/\.ca-dialog\.[\w-]+/);
});
