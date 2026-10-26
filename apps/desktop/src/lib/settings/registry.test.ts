// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * The registry is the one place that says what Settings contains. These tests pin the contract that makes it
 * impossible for the sidebar, the page, the search and the deep links to disagree:
 * every category and tab is described, every item is reachable by search AND by deep link, aliases never collide,
 * and every page has a component (and every component a page).
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
	CLUSTERS,
	REGISTRY,
	SECTIONS,
	SECTION_TABS,
	SETTINGS_INDEX,
	resolveSection,
	searchSettings,
} from "./index.ts";

const lib = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = (rel: string) => readFileSync(join(lib, rel), "utf8");

describe("registry shape", () => {
	test("every category has a label, a one-line description and a known cluster", () => {
		for (const c of REGISTRY) {
			expect(c.label.length, c.id).toBeGreaterThan(1);
			expect(c.description, c.id).toMatch(/^[A-Z].*[.]$/);
			expect(c.description.length, c.id).toBeLessThan(150);
			expect(
				CLUSTERS.map((x) => x.id),
				c.id,
			).toContain(c.cluster);
		}
	});

	test("every cluster is used and every category sits in exactly one", () => {
		for (const k of CLUSTERS)
			expect(
				REGISTRY.some((c) => c.cluster === k.id),
				k.id,
			).toBe(true);
		expect(SECTIONS.map((s) => s.id)).toEqual(REGISTRY.map((c) => c.id));
	});

	test("a tabbed category names and describes every tab; a single page has no tab", () => {
		for (const c of REGISTRY) {
			if (c.pages.length === 1) {
				expect(c.pages[0].id, c.id).toBeUndefined();
				expect(SECTION_TABS[c.id], c.id).toBeUndefined();
			} else {
				for (const p of c.pages) {
					expect(p.id, c.id).toBeTruthy();
					expect(p.label, `${c.id}/${p.id}`).toBeTruthy();
					expect(p.description, `${c.id}/${p.id}`).toMatch(/^[A-Z].*[.]$/);
				}
				expect(SECTION_TABS[c.id].map((t) => t.id)).toEqual(c.pages.map((p) => p.id));
			}
		}
	});

	test("every page lists the source files that render it, and they exist", () => {
		for (const c of REGISTRY)
			for (const p of c.pages) {
				expect(p.files.length, `${c.id}/${p.id ?? ""}`).toBeGreaterThan(0);
				for (const f of p.files) expect(() => src(f), f).not.toThrow();
			}
	});

	test("every page holds at least one group and every group at least one item", () => {
		for (const c of REGISTRY)
			for (const p of c.pages) {
				expect(p.groups.length, `${c.id}/${p.id ?? ""}`).toBeGreaterThan(0);
				for (const g of p.groups) expect(g.items.length, `${c.id}/${g.title}`).toBeGreaterThan(0);
			}
	});

	test("the index is the registry flattened: section, tab and group come from the nesting", () => {
		const n = REGISTRY.flatMap((c) => c.pages.flatMap((p) => p.groups.flatMap((g) => g.items))).length;
		expect(SETTINGS_INDEX.length).toBe(n);
		for (const c of REGISTRY)
			for (const p of c.pages)
				for (const g of p.groups)
					for (const i of g.items)
						expect(SETTINGS_INDEX.find((e) => e.id === i.id)).toMatchObject({
							section: c.id,
							group: g.title,
							...(p.id && { tab: p.id }),
						});
	});
});

describe("every item is reachable", () => {
	test("by search: its own label finds it, and the hit says where it lives", () => {
		for (const e of SETTINGS_INDEX) {
			const hit = searchSettings(e.label).find((x) => x.id === e.id);
			expect(hit, e.label).toBeDefined();
			expect(hit.section, e.id).toBe(e.section);
		}
	});

	test("by deep link: its id opens the category and tab that hold it, and names the row to reveal", () => {
		for (const e of SETTINGS_INDEX) {
			const t = resolveSection(e.id);
			expect(t, e.id).toMatchObject({ section: e.section, item: e.id });
			if (e.tab) expect(t.tab, e.id).toBe(e.tab);
		}
	});

	test("every category and tab id opens itself", () => {
		for (const c of REGISTRY) {
			expect(resolveSection(c.id).section, c.id).toBe(c.id);
			for (const p of c.pages.filter((p) => p.id))
				expect(resolveSection(`${c.id}/${p.id}`), `${c.id}/${p.id}`).toEqual({ section: c.id, tab: p.id });
		}
	});
});

describe("ids and aliases never collide", () => {
	const items = SETTINGS_INDEX.map((e) => e.id);
	const aliases = [
		...REGISTRY.flatMap((c) => [
			...(c.aliases ?? []),
			...c.pages.flatMap((p) => [
				...(p.aliases ?? []),
				...p.groups.flatMap((g) => g.items.flatMap((i) => i.aliases ?? [])),
			]),
		]),
	];

	test("item ids are unique", () => {
		expect(new Set(items).size).toBe(items.length);
	});

	test("an alias is never a category id, an item id or another alias", () => {
		const taken = new Set([...REGISTRY.map((c) => c.id), ...items]);
		for (const a of aliases) expect(taken.has(a), `alias ${a} shadows a real id`).toBe(false);
		expect(new Set(aliases).size).toBe(aliases.length);
	});

	test("an item id is never also a category id (a deep link must mean one thing)", () => {
		for (const c of REGISTRY) expect(items, c.id).not.toContain(c.id);
	});
});

// Ids that shipped before and are still passed to `updates.show(...)`, written in ctl/palette/onboarding/toasts/docs.
// They must keep landing on the content they promised, wherever it lives now.
describe("deep links that shipped before still land on their content", () => {
	const old: [string, { section: string; tab?: string }][] = [
		["updates", { section: "general" }],
		["shell", { section: "integrations", tab: "shell" }],
		["browser", { section: "integrations", tab: "shell" }],
		["notify", { section: "notifications" }],
		["sounds", { section: "notifications" }],
		["ssh", { section: "integrations", tab: "ssh" }],
		["mobile", { section: "integrations", tab: "mobile" }],
		["agent-integration", { section: "integrations", tab: "agents" }],
		["appearance/terminal", { section: "terminal" }],
		["appearance/languages", { section: "editor", tab: "languages" }],
		["appearance/editor", { section: "editor", tab: "editor" }],
		["appearance/interface", { section: "appearance" }],
		["languages", { section: "editor", tab: "languages" }],
		["editor", { section: "editor", tab: "editor" }],
		["editor/languages", { section: "editor", tab: "languages" }],
		["keyboard", { section: "keyboard" }],
		["git", { section: "git" }],
		["help", { section: "help" }],
		["workspaces", { section: "workspaces" }],
		["agents", { section: "agents" }],
	];
	for (const [id, want] of old)
		test(id, () => {
			expect(resolveSection(id)).toMatchObject(want);
		});

	test("the section/tab of a tabbed category: a tab that is not there falls back to the first", () => {
		expect(resolveSection("integrations")).toEqual({ section: "integrations", tab: "shell" });
		expect(resolveSection("integrations/nope")).toEqual({ section: "integrations", tab: "shell" });
	});
});

describe("every id the app passes to updates.show(...) resolves", () => {
	// Read the real call sites (palette, LSP action, sidebar chip, theme offers, commit toast, …) so a rename that
	// forgets a caller goes red instead of silently opening General.
	const files = [
		"shell/projectCommands.svelte.ts",
		"shell/themeCommands.svelte.ts",
		"lsp/run.ts",
		"settings/CodeTab.svelte",
		"components/DevLine.svelte",
		"components/sidebar/SidebarFooter.svelte",
		"stores/commitMessage.svelte.ts",
	];
	const used = files.flatMap((f) =>
		[...src(f).matchAll(/updates\.show\("([^"]+)"\)/g)].map((m) => [f, m[1]]),
	);

	test("the app has deep links to check", () => {
		expect(used.length).toBeGreaterThanOrEqual(8);
	});

	test("none of them falls through to the General fallback by accident", () => {
		const general = new Set(["general", "updates", "update-channel"]);
		for (const [f, id] of used) {
			const t = resolveSection(id);
			if (t.section === "general") expect(general.has(id), `${f}: ${id}`).toBe(true);
		}
	});
});

describe("unknown ids", () => {
	test("fall back to the first category and never to a prototype property", () => {
		for (const id of ["nope", "constructor", "__proto__", "toString", "", "a/b/c"])
			expect(resolveSection(id), id).toEqual({ section: REGISTRY[0].id });
	});
});

describe("pages have components, components have pages", () => {
	// The Svelte map is plain TS, so read it as text: importing every page here would load the whole app.
	const map = src("components/settings/pageComponents.ts");
	const keys = [...map.matchAll(/^\s*"?([a-z][a-z/-]*)"?:\s*[A-Z]\w+,?\s*$/gm)].map((m) => m[1]);
	const want = REGISTRY.flatMap((c) => c.pages.map((p) => (p.id ? `${c.id}/${p.id}` : c.id)));

	test("one component per registry page, none extra", () => {
		expect(keys.sort()).toEqual(want.sort());
	});
});
