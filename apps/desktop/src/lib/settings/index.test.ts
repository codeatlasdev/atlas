// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { CATEGORIES } from "#lib/sound/policy.ts";
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
	SECTIONS,
	SECTION_TABS,
	SETTINGS_INDEX,
	crumb,
	relativeFrom,
	resolveSection,
	searchSettings,
} from "./index.ts";

describe("resolveSection", () => {
	test("legacy ids map to the new structure", () => {
		expect(resolveSection("updates")).toEqual({ section: "general" });
		expect(resolveSection("shell")).toEqual({ section: "general" });
		expect(resolveSection("appearance")).toEqual({ section: "appearance" });
		expect(resolveSection("editor")).toEqual({ section: "editor", tab: "editor" });
	});

	// Before the redo Editor, Languages, Interface and Terminal were tabs of Appearance. Deep links from the
	// command palette, the LSP code actions and the sidebar still use those ids and must land in the new homes.
	test("old appearance/<tab> deep links land in the section that now owns the tab", () => {
		expect(resolveSection("appearance/terminal")).toEqual({ section: "terminal" });
		expect(resolveSection("appearance/languages")).toEqual({ section: "editor", tab: "languages" });
		expect(resolveSection("appearance/editor")).toEqual({ section: "editor", tab: "editor" });
		expect(resolveSection("appearance/interface")).toEqual({ section: "appearance" });
		expect(resolveSection("appearance/appearance")).toEqual({ section: "appearance" });
		expect(resolveSection("appearance/nope")).toEqual({ section: "appearance" });
	});

	test("current ids pass through, tabbed sections open on their first tab", () => {
		for (const s of SECTIONS) {
			const first = SECTION_TABS[s.id]?.[0]?.id;
			expect(resolveSection(s.id)).toEqual(first ? { section: s.id, tab: first } : { section: s.id });
		}
	});

	test("<section>/<tab> picks the tab, an unknown tab falls back to the first", () => {
		expect(resolveSection("editor/languages")).toEqual({ section: "editor", tab: "languages" });
		expect(resolveSection("editor/nope")).toEqual({ section: "editor", tab: "editor" });
		expect(resolveSection("terminal/languages")).toEqual({ section: "terminal" });
	});

	test("unknown or prototype-ish ids fall back to General", () => {
		expect(resolveSection("nope")).toEqual({ section: "general" });
		expect(resolveSection("constructor")).toEqual({ section: "general" });
		expect(resolveSection("__proto__")).toEqual({ section: "general" });
		expect(resolveSection("")).toEqual({ section: "general" });
	});
});

// Every id the app passes to `updates.show(...)` (command palette, LSP code action, sidebar chip, theme offers) must
// open the screen it names. They live in other modules, so read the real sources instead of repeating the list here.
describe("deep links used by the app resolve to the section that holds what they promise", () => {
	const lib = join(dirname(fileURLToPath(import.meta.url)), "..");
	const sources = [
		"shell/projectCommands.svelte.ts",
		"shell/themeCommands.svelte.ts",
		"lsp/run.ts",
		"settings/CodeTab.svelte",
		"components/DevLine.svelte",
		"components/sidebar/SidebarFooter.svelte",
	];
	const targets = sources.flatMap((f) =>
		[...readFileSync(join(lib, f), "utf8").matchAll(/updates\.show\("([^"]*)"\)/g)].map((m) => m[1]!),
	);

	test("the app has deep links to check", () => {
		expect(targets.length).toBeGreaterThanOrEqual(6);
	});

	test("every deep link lands on a real section (and tab) rather than the General fallback by accident", () => {
		for (const t of targets) {
			const r = resolveSection(t);
			expect(
				SECTIONS.some((s) => s.id === r.section),
				t,
			).toBe(true);
			if (r.section === "general") expect(["updates", "shell", "general"], t).toContain(t);
		}
	});

	test("the language-server links open Editor ▸ Languages and the theme links open Appearance", () => {
		expect(resolveSection("editor/languages")).toEqual({ section: "editor", tab: "languages" });
		for (const t of targets.filter((t) => t === "appearance"))
			expect(resolveSection(t)).toEqual({ section: "appearance" });
	});
});

describe("settings index", () => {
	test("ids are unique per section/tab and every target exists", () => {
		const keys = SETTINGS_INDEX.map((e) => `${e.section}/${e.tab ?? ""}/${e.id}`);
		expect(new Set(keys).size).toBe(keys.length);
		// A setting id is one row on one screen: the same id in two places would flash the wrong one.
		expect(new Set(SETTINGS_INDEX.map((e) => e.id)).size).toBe(SETTINGS_INDEX.length);
		for (const e of SETTINGS_INDEX) {
			expect(
				SECTIONS.some((s) => s.id === e.section),
				e.id,
			).toBe(true);
			const tabs = SECTION_TABS[e.section];
			// A tabbed section's entries always name a tab that exists; an untabbed section's never do.
			if (tabs)
				expect(
					tabs.some((t) => t.id === e.tab),
					`${e.id} needs a tab of ${e.section}`,
				).toBe(true);
			else expect(e.tab, e.id).toBeUndefined();
		}
	});

	test("labels and descriptions carry no raw setting keys or file paths", () => {
		for (const e of SETTINGS_INDEX) {
			expect(e.label, e.id).not.toMatch(/[a-z]+_[a-z]+|\.json|osc ?52|xterm/i);
			expect(e.description, e.id).not.toMatch(/[a-z]+_[a-z]+|settings\.json/);
		}
	});

	// The incident this guards: Languages lived under Appearance, Terminal under Appearance, links and
	// notifications under Agents. Each setting must be where a user would look first.
	test("every setting lives in the section a user would look for it in", () => {
		const home: Record<string, string> = {
			updates: "general",
			"update-channel": "general",
			shell: "general",
			sounds: "general",
			browser: "general",
			notify: "general",
			"theme-mode": "appearance",
			accent: "appearance",
			"ui-zoom": "appearance",
			"ui-font": "appearance",
			"ui-size": "appearance",
			"panel-dock": "appearance",
			"pane-border": "appearance",
			"buffer-font": "editor",
			"buffer-size": "editor",
			"editor-tab-size": "editor",
			"editor-minimap": "editor",
			"lsp-enable": "editor",
			"lsp-servers": "editor",
			"lsp-custom": "editor",
			"lsp-project": "editor",
			gpu: "terminal",
			"term-font": "terminal",
			scrollback: "terminal",
			clipboard: "terminal",
			"ws-dir": "workspaces",
			"agent-default": "agents",
			"agent-permissions": "agents",
			quick: "agents",
			keymap: "keyboard",
			"ssh-hosts": "ssh",
			tour: "help",
		};
		for (const [id, section] of Object.entries(home)) {
			const e = SETTINGS_INDEX.find((x) => x.id === id);
			expect(e, id).toBeDefined();
			expect(e!.section, id).toBe(section);
		}
	});

	test("Editor splits into Editor and Languages tabs, with every language-server manager row in Languages", () => {
		expect(SECTION_TABS.editor!.map((t) => t.id)).toEqual(["editor", "languages"]);
		for (const id of ["lsp-servers", "lsp-custom", "lsp-project"])
			expect(SETTINGS_INDEX.find((e) => e.id === id)).toMatchObject({ section: "editor", tab: "languages" });
		for (const id of ["lsp-enable", "lsp-format-on-save", "lsp-inlay-hints", "lsp-manage"])
			expect(SETTINGS_INDEX.find((e) => e.id === id)).toMatchObject({ section: "editor", tab: "editor" });
	});

	test("the sections read in the order a user works: General, Appearance, Editor, Terminal, … Help", () => {
		expect(SECTIONS.map((s) => s.id)).toEqual([
			"general",
			"appearance",
			"editor",
			"terminal",
			"workspaces",
			"agents",
			"keyboard",
			"ssh",
			"help",
		]);
	});
});

describe("searchSettings", () => {
	test("empty query matches nothing", () => {
		expect(searchSettings("")).toEqual([]);
		expect(searchSettings("   ")).toEqual([]);
	});

	test("matches by label and jumps to the right tab", () => {
		const r = searchSettings("code font");
		expect(r[0]).toMatchObject({ id: "buffer-font", section: "editor", tab: "editor" });
	});

	test("matches descriptions", () => {
		const r = searchSettings("graphics chip");
		expect(r.map((e) => e.id)).toContain("gpu");
	});

	test("matches hidden keywords such as the old technical names", () => {
		expect(searchSettings("osc52")[0]).toMatchObject({ id: "clipboard", section: "terminal" });
		expect(searchSettings("scrollback")[0]).toMatchObject({ id: "scrollback" });
		expect(searchSettings("zsh")[0]).toMatchObject({ id: "shell", section: "general" });
	});

	test("every word must match; order of words does not matter", () => {
		expect(searchSettings("font terminal").map((e) => e.id)).toContain("term-font");
		expect(searchSettings("terminal font").map((e) => e.id)).toContain("term-font");
		expect(searchSettings("font zzzz")).toEqual([]);
	});

	test("label hits rank above description-only hits", () => {
		const r = searchSettings("cursor");
		const first = r.findIndex((e) => !/cursor/i.test(e.label));
		const lastLabel = r.map((e) => /cursor/i.test(e.label)).lastIndexOf(true);
		expect(lastLabel).toBeLessThan(first === -1 ? Infinity : first);
	});

	test("case and accents are ignored", () => {
		expect(searchSettings("THEME")[0]?.id).toBe("theme-mode");
		expect(searchSettings("thème")[0]?.id).toBe("theme-mode");
	});

	test("every setting is findable by its own label", () => {
		for (const e of SETTINGS_INDEX) {
			expect(
				searchSettings(e.label).map((x) => x.id),
				e.label,
			).toContain(e.id);
		}
	});

	test("language server words find the Languages tab of Editor", () => {
		for (const q of ["language server typescript", "gopls", "rust-analyzer", "custom command"]) {
			const hit = searchSettings(q).find((e) => e.section === "editor" && e.tab === "languages");
			expect(hit, q).toBeDefined();
		}
		expect(searchSettings("custom command")[0]).toMatchObject({
			id: "lsp-custom",
			section: "editor",
			tab: "languages",
		});
		expect(
			searchSettings("language server typescript gopls rust-analyzer custom command").length,
		).toBeGreaterThan(0);
	});

	test("editor settings are searchable and live in the Editor section", () => {
		expect(searchSettings("tab size")[0]).toMatchObject({
			id: "editor-tab-size",
			section: "editor",
			tab: "editor",
		});
		expect(searchSettings("word_wrap")[0]).toMatchObject({ id: "editor-word-wrap", section: "editor" });
		expect(searchSettings("minimap")[0]).toMatchObject({ id: "editor-minimap", section: "editor" });
	});

	// Moved settings must be found by what the user types, and the result must say where they went.
	test("moved settings are found in their new section", () => {
		expect(searchSettings("fast terminal")[0]).toMatchObject({ id: "gpu", section: "terminal" });
		expect(searchSettings("terminal font")[0]).toMatchObject({ id: "term-font", section: "terminal" });
		expect(searchSettings("interface font")[0]).toMatchObject({ id: "ui-font", section: "appearance" });
		expect(searchSettings("file icons")[0]).toMatchObject({ id: "panel-file-icons", section: "appearance" });
		expect(searchSettings("open links")[0]).toMatchObject({ id: "browser", section: "general" });
		expect(searchSettings("notifications")[0]).toMatchObject({ id: "notify", section: "general" });
		expect(searchSettings("custom language server")[0]).toMatchObject({
			id: "lsp-custom",
			section: "editor",
		});
	});
});

describe("crumb", () => {
	test("shows section and tab", () => {
		expect(crumb(SETTINGS_INDEX.find((e) => e.id === "gpu")!)).toBe("Terminal");
		expect(crumb(SETTINGS_INDEX.find((e) => e.id === "ws-dir")!)).toBe("Workspaces");
		expect(crumb(SETTINGS_INDEX.find((e) => e.id === "lsp-custom")!)).toBe("Editor › Languages");
		expect(crumb(SETTINGS_INDEX.find((e) => e.id === "buffer-font")!)).toBe("Editor");
	});
});

describe("relativeFrom", () => {
	test("climbs out of the worktree to reach a file elsewhere", () => {
		expect(relativeFrom("/Users/me/.atlas/worktrees/repo/a", "/Users/me/.atlas/desktop/settings.json")).toBe(
			"../../../desktop/settings.json",
		);
		expect(relativeFrom("/Users/me/p", "/Users/me/p/src/a.ts")).toBe("src/a.ts");
		expect(relativeFrom("/a/b", "/c/d.json")).toBe("../../c/d.json");
	});
});

describe("sound settings are searchable", () => {
	// Every row of Settings ▸ General ▸ Sounds must be reachable from search, and nothing may point at a category
	// the sound system does not have (the catalog and the policy are two lists that must stay one).
	test("master, volume and reduce are findable", () => {
		for (const q of ["sounds", "volume", "reduce sounds", "earcon", "chime"])
			expect(
				searchSettings(q).some((e) => e.section === "general" && e.group === "Sounds"),
				q,
			).toBe(true);
	});
	test("one entry per sound category, ids matching the policy", () => {
		const ids = SETTINGS_INDEX.filter((e) => e.group === "Sounds" && e.id.startsWith("sound-cat-")).map((e) =>
			e.id.slice("sound-cat-".length),
		);
		expect(ids.sort()).toEqual([...CATEGORIES].sort());
	});
});
