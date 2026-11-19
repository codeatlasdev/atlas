// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { m } from "#lib/i18n/m.ts";
import { tabLabel } from "./tabLabel.ts";
import { inLocale } from "../../../test/locale.ts";

// Tabs are stored with the English name they opened with ("Changes", "Panel"); the screen shows the app's language.
test("a built-in tab name is drawn in the app's language, a name the person chose never is", async () => {
	const stored = { kind: "diff", label: "Changes" };
	const english = tabLabel(stored);
	expect(english).toBe(m.workbench_tab_changes());
	await inLocale("pt-BR", () => {
		expect(tabLabel(stored)).toBe(m.workbench_tab_changes());
		expect(tabLabel(stored)).not.toBe(english);
	});
	expect(tabLabel({ ...stored, customLabel: true, label: "Changes" })).toBe("Changes");
	expect(tabLabel({ kind: "editor", label: "main.rs" })).toBe("main.rs");
});

test("a terminal's own title wins over the built-in name; the Panel tab is always Panel", () => {
	expect(tabLabel({ kind: "terminal", label: "Terminal" }, "bun dev")).toBe("bun dev");
	expect(tabLabel({ kind: "terminal", label: "Terminal" })).toBe(m.crew_chat_header_terminal());
	expect(tabLabel({ kind: "services", label: "Services" })).toBe(m.workbench_tab_panel());
});

// Regression guard for the editor's own words: every key of `localizedEditor` is a phrase one of the CodeMirror packages
// really asks for, so a typo or a phrase a package dropped fails here instead of silently staying in English.
test("every phrase the editor translates is one CodeMirror asks for", () => {
	const dir = new URL("../../../node_modules/@codemirror/", import.meta.url);
	const dist = ["search", "language", "autocomplete", "lint", "view", "commands"]
		.map((p) => readFileSync(new URL(`${p}/dist/index.js`, dir), "utf8"))
		.join("\n");
	const src = readFileSync(new URL("../editor/phrases.ts", import.meta.url), "utf8");
	const keys = [...src.matchAll(/^\t\t\t(?:"([^"]+)"|(\w+)): m\./gm)].map((x) => x[1] ?? x[2]);
	expect(keys.length).toBeGreaterThan(10);
	for (const k of keys)
		expect(
			dist.includes(`phrase("${k}"`) || dist.includes(`, "${k}"`),
			`CodeMirror never asks for "${k}"`,
		).toBe(true);
});
