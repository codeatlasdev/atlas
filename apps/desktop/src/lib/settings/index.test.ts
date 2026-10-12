// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	APPEARANCE_TABS,
	SECTIONS,
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
		expect(resolveSection("appearance")).toEqual({ section: "appearance", tab: "appearance" });
		expect(resolveSection("editor")).toEqual({ section: "appearance", tab: "editor" });
	});

	test("current ids pass through", () => {
		for (const s of SECTIONS.filter((s) => s.id !== "appearance")) {
			expect(resolveSection(s.id)).toEqual({ section: s.id });
		}
	});

	test("appearance/<tab> picks the tab, unknown tab falls back to the first", () => {
		for (const t of APPEARANCE_TABS) {
			expect(resolveSection(`appearance/${t.id}`)).toEqual({ section: "appearance", tab: t.id });
		}
		expect(resolveSection("appearance/nope")).toEqual({ section: "appearance", tab: "appearance" });
	});

	test("unknown or prototype-ish ids fall back to General", () => {
		expect(resolveSection("nope")).toEqual({ section: "general" });
		expect(resolveSection("constructor")).toEqual({ section: "general" });
		expect(resolveSection("")).toEqual({ section: "general" });
	});
});

describe("settings index", () => {
	test("ids are unique per section/tab and every target exists", () => {
		const keys = SETTINGS_INDEX.map((e) => `${e.section}/${e.tab ?? ""}/${e.id}`);
		expect(new Set(keys).size).toBe(keys.length);
		for (const e of SETTINGS_INDEX) {
			expect(SECTIONS.some((s) => s.id === e.section)).toBe(true);
			if (e.tab) expect(e.section).toBe("appearance");
			else expect(e.section).not.toBe("appearance");
		}
	});

	test("labels and descriptions carry no raw setting keys or file paths", () => {
		for (const e of SETTINGS_INDEX) {
			expect(e.label, e.id).not.toMatch(/[a-z]+_[a-z]+|\.json|osc ?52|xterm/i);
			expect(e.description, e.id).not.toMatch(/[a-z]+_[a-z]+|settings\.json/);
		}
	});
});

describe("searchSettings", () => {
	test("empty query matches nothing", () => {
		expect(searchSettings("")).toEqual([]);
		expect(searchSettings("   ")).toEqual([]);
	});

	test("matches by label and jumps to the right tab", () => {
		const r = searchSettings("code font");
		expect(r[0]).toMatchObject({ id: "buffer-font", section: "appearance", tab: "editor" });
	});

	test("matches descriptions", () => {
		const r = searchSettings("graphics chip");
		expect(r.map((e) => e.id)).toContain("gpu");
	});

	test("matches hidden keywords such as the old technical names", () => {
		expect(searchSettings("osc52")[0]).toMatchObject({ id: "clipboard", tab: "terminal" });
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
});

describe("crumb", () => {
	test("shows section and tab", () => {
		const gpu = SETTINGS_INDEX.find((e) => e.id === "gpu")!;
		expect(crumb(gpu)).toBe("Appearance › Terminal");
		expect(crumb(SETTINGS_INDEX.find((e) => e.id === "ws-dir")!)).toBe("Workspaces");
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
