// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * The catalog (search, deep links, flash-on-arrival) and the screens are two lists that must stay one: a setting
 * id rendered in a component that the catalog files under another section sends "Go to setting" to a page where
 * the row is not, and nothing flashes. This reads the real Svelte sources, so moving a row without its catalog
 * entry (or the reverse) goes red.
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { REGISTRY } from "./index.ts";

/** The pages of the registry, one per tab (or one per single-page section). */
const PAGES = REGISTRY.flatMap((c) =>
	c.pages.map((p) => ({ section: c.id, tab: p.id, files: p.files, groups: p.groups })),
);

const lib = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = (rel: string) => readFileSync(join(lib, rel), "utf8");

/**
 * Setting ids a Svelte source renders: `id=` on a component tag (`<Row id=…>`; HTML ids such as an `<h3 id>` label
 * are not settings), `data-setting=` and `setting=`, each either a literal or an expression whose string literals
 * are ids (`data-setting={first ? "keymap" : undefined}`), plus `idPrefix=` (font rows: `<prefix>-font`/`-size`).
 */
function idsOf(source: string): string[] {
	const ids = new Set<string>();
	const attr = /(?:<[A-Z][A-Za-z]*[^<>]*?\bid|\bdata-setting|\bsetting)=(?:"([^"{}]*)"|\{([^{}]*)\})/gs;
	for (const m of source.matchAll(attr)) {
		for (const lit of (m[2] ?? `"${m[1]}"`).matchAll(/"([a-z][a-z0-9-]*)"/g)) ids.add(lit[1]!);
	}
	for (const m of source.matchAll(/idPrefix="([a-z]+)"/g)) ids.add(`${m[1]}-font`).add(`${m[1]}-size`);
	return [...ids];
}

describe("every screen renders only the settings the registry files under it", () => {
	for (const page of PAGES) {
		test(`${page.section}${page.tab ? `/${page.tab}` : ""}: ${page.files.join(", ")}`, () => {
			const rendered = new Set(page.files.flatMap((f) => idsOf(src(f))));
			const filed = page.groups.flatMap((g) => g.items.map((i) => i.id));
			const where = `${page.section}${page.tab ? "/" + page.tab : ""}`;
			for (const id of rendered) {
				if (id.includes("{")) continue;
				expect(
					filed.includes(id),
					`${id} is rendered by ${page.files.join("/")} but the registry does not file it under ${where}`,
				).toBe(true);
			}
			for (const id of filed) {
				if (id.startsWith("sound-cat-")) continue;
				expect(
					rendered.has(id),
					`${id} is filed under ${where} but ${page.files.join("/")} never renders it`,
				).toBe(true);
			}
		});
	}
});
