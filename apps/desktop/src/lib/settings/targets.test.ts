// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { resolveSection } from "./targets.ts";
import { IDS } from "./registryIds.ts";
import { ITEM_TEXT, CATEGORY_TEXT, PAGE_TEXT, GROUP_TITLE } from "./registryText.ts";

// Startup code (`stores/updates.svelte.ts`) resolves `updates.show("<id>")` through `targets.ts`. If that module reached the
// registry's text, its ~280 messages would load at startup instead of with the Settings view (the bundle budget caught
// it once). The text is `registryText.ts`; the structure is `registryIds.ts`.
const lib = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Every source file `file` imports, following relative and `#lib/` imports, as paths under `src/lib`. */
function graph(file: string, seen = new Set<string>()): Set<string> {
	if (seen.has(file)) return seen;
	seen.add(file);
	const source = readFileSync(file, "utf8");
	for (const m of source.matchAll(/(?:import|export)[^"'`;]*?from\s*["']([^"']+)["']/g)) {
		const spec = m[1];
		const base = spec.startsWith("#lib/")
			? join(lib, spec.slice(5))
			: spec.startsWith(".")
				? resolve(dirname(file), spec)
				: null;
		if (base && existsSync(base) && /\.(ts|svelte)$/.test(base)) graph(base, seen);
	}
	return seen;
}

test("the module that resolves deep links never reaches the registry's text", () => {
	const reached = [...graph(join(lib, "settings/targets.ts"))].map((f) => f.slice(lib.length + 1));
	for (const text of [
		"settings/registryText.ts",
		"settings/registry.ts",
		"settings/sections.ts",
		"settings/catalog.ts",
	])
		expect(reached, `targets.ts reaches ${text}`).not.toContain(text);
	expect(reached).toContain("settings/registryIds.ts");
	// Neither does the structure, nor the sound categories it lists.
	const ids = [...graph(join(lib, "settings/registryIds.ts"))].map((f) => f.slice(lib.length + 1));
	expect(ids).not.toContain("sound/policy.ts");
	expect(ids).not.toContain("settings/registryText.ts");
});

test("a deep link resolves with only the structure", () => {
	expect(resolveSection("terminal")).toMatchObject({ section: "terminal" });
	expect(resolveSection("editor/languages")).toMatchObject({ section: "editor", tab: "languages" });
	expect(resolveSection("git-commit-agent")).toMatchObject({ item: "git-commit-agent" });
});

test("the text has an entry for exactly the ids of the structure", () => {
	const items = new Set<string>();
	IDS.forEach((c, ci) => {
		expect(CATEGORY_TEXT[c.id], c.id).toBeDefined();
		c.pages.forEach((p, pi) => {
			if (p.id) expect(PAGE_TEXT[`${c.id}/${p.id}`], `${c.id}/${p.id}`).toBeDefined();
			p.groups.forEach((g, gi) => {
				expect(GROUP_TITLE[`${c.id}#${pi}#${gi}`], `${c.id}#${pi}#${gi}`).toBeDefined();
				for (const i of g.items) items.add(i.id);
			});
		});
	});
	for (const id of items) expect(ITEM_TEXT[id], id).toBeDefined();
	expect(Object.keys(ITEM_TEXT).sort()).toEqual([...items].sort());
	expect(Object.keys(CATEGORY_TEXT).sort()).toEqual(IDS.map((c) => c.id).sort());
});
