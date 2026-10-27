// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { REGISTRY } from "../../settings/index.ts";

const lib = join(import.meta.dir, "../..");
const read = (rel) => readFileSync(join(lib, rel), "utf8");
const page = read("components/settings/CaptureSettings.svelte");
const history = read("shots/history.ts");
const settings = read("shots/settings.ts");

/** Commands the Settings page may call: the "Settings page" list of the shots contract, plus the dock's own `dock_prefs`. */
const CONTRACT: Record<string, true> = Object.fromEntries(
	[
		"shots_prefs",
		"shots_prefs_set",
		"shots_presets",
		"hotkey_check",
		"shots_permission",
		"shots_permission_request",
		"shots_open_settings",
		"shots_open_shortcuts_settings",
		"shots_record_capabilities",
		"shots_pick_folder",
		"shots_history",
		"shots_history_remove",
		"shots_history_reveal",
		"shots_history_open",
		"dock_prefs",
	].map((n) => [n, true]),
);

const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

describe("commands", () => {
	test("every command the capture settings call is in the contract", () => {
		const names = new Set([...invoked(page), ...invoked(history), ...invoked(settings)]);
		for (const n of names) expect(CONTRACT[n], `${n} is not in the shots contract`).toBe(true);
		for (const n of [
			"shots_prefs",
			"shots_prefs_set",
			"shots_presets",
			"hotkey_check",
			"shots_permission",
			"shots_permission_request",
			"shots_open_settings",
			"shots_open_shortcuts_settings",
			"shots_record_capabilities",
			"shots_pick_folder",
			"shots_history",
			"shots_history_remove",
			"shots_history_reveal",
			"shots_history_open",
		])
			expect(names.has(n), `${n} is never called`).toBe(true);
	});

	test("every save goes through shots_prefs_set, and prefs is only assigned from Rust's reply", () => {
		expect(page).toContain('invoke<CapturePrefs>("shots_prefs_set", { prefs: next })');
		// The only assignments to `prefs` are the initial load, Rust's reply, and the snap-back of an invalid number.
		const assigns = [...page.matchAll(/(?<![\w.$])prefs\s*=\s*(?!=)([^;\n]*)/g)].map((m) => m[1].trim());
		for (const rhs of assigns)
			expect(
				/^await invoke<CapturePrefs>\("shots_prefs_set"/.test(rhs) ||
					/^\{ \.\.\.prefs \}/.test(rhs) ||
					/^\[?prefs/.test(rhs) ||
					/^\$state</.test(rhs),
				`prefs = ${rhs}`,
			).toBe(true);
	});

	test("a rejection shows Rust's text in a toast", () => {
		expect(page).toMatch(/catch \(e\) \{\s*toast\.error\(errText\(e\)\);/);
	});

	test("history removal says it never deletes the file", () => {
		expect(page).toContain("Removing never deletes the file");
		expect(page).toContain("The file stays on your disk");
	});
});

describe("the page", () => {
	test("imports no workspace store and does not use the kit's `dark` class", () => {
		expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
		expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
	});

	test("the permission text and the preset note are on screen before anything is applied", () => {
		expect(page).toContain("Screen Recording lets Atlas see the pixels of other apps");
		expect(page).toContain("nothing is recorded in the background");
		expect(page).toContain("macOS asks you to restart Atlas");
		// The note renders from the picked preset, and applying is a separate, explicit button.
		expect(page).toContain("shownPreset?.note");
		expect(page).toContain("Apply these keys");
		expect(page.indexOf("shownPreset?.note")).toBeLessThan(page.indexOf("Apply these keys"));
	});

	test("buttons the user needs are named", () => {
		for (const t of [
			"Grant access",
			"Open System Settings",
			"Choose…",
			"Reset",
			"Clear",
			"Open",
			"Reveal",
			"Remove from list",
		])
			expect(page, t).toContain(t);
	});

	test("the Esc behaviour of recording is documented", () => {
		expect(page).toMatch(/Esc cancels/);
	});
});

describe("registry <-> component", () => {
	const section = REGISTRY.find((c) => c.id === "capture");
	const filed = section.pages.flatMap((p) => p.groups.flatMap((g) => g.items.map((i) => i.id)));
	/** Row ids rendered by the component: literals on a component tag's `id`. */
	const rendered = [
		...new Set(
			[...page.matchAll(/<Row\b[^<>]*?\bid=(?:"([^"{}]*)"|\{([^{}]*)\})/gs)].flatMap((m) =>
				m[1] ? [m[1]] : [...m[2].matchAll(/[?:]\s*"(capture-[a-z0-9-]+)"/g)].map((x) => x[1]),
			),
		),
	];

	test("the Capture section exists in the Atlas cluster and has one page", () => {
		expect(section.cluster).toBe("app");
		expect(section.label).toBe("Capture");
		expect(section.pages).toHaveLength(1);
		expect(section.pages[0].files).toEqual(["components/settings/CaptureSettings.svelte"]);
	});

	test("the registry item ids are exactly the Row ids of the component", () => {
		expect([...filed].sort()).toEqual([...rendered].sort());
	});

	test("every row's label and description in the registry is what the screen shows", () => {
		const items = Object.fromEntries(section.pages[0].groups.flatMap((g) => g.items).map((i) => [i.id, i]));
		for (const m of page.matchAll(/<Row\s+id="([a-z-]+)"\s+label="([^"]*)"(?:\s+description="([^"]*)")?/g)) {
			const it = items[m[1]];
			expect(it?.label, m[1]).toBe(m[2]);
			if (m[3] !== undefined) expect(it?.description, m[1]).toBe(m[3]);
		}
	});

	test("the capture-on-release row is in the Screenshots group and writes still.captureOnRelease", () => {
		const it = section.pages[0].groups.flatMap((g) => g.items).find((i) => i.id === "capture-on-release");
		expect(it?.label).toBe("Capture when the mouse is released");
		expect(it?.description).toBe(
			"Take the shot as soon as you let go of the selection; turn off to adjust it first and confirm with Return.",
		);
		const group = section.pages[0].groups.find((g) => g.items.some((i) => i.id === "capture-on-release"));
		expect(group?.title).toBe("Screenshots");
		expect(page).toMatch(/checked=\{prefs\.still\.captureOnRelease\}/);
		expect(page).toMatch(/still: \{ \.\.\.prefs\.still, captureOnRelease \}/);
	});

	test("the hotkey rows' texts come from KEY_FIELDS and equal the registry's", () => {
		const items = Object.fromEntries(section.pages[0].groups.flatMap((g) => g.items).map((i) => [i.id, i]));
		// Row ids are lowercase kebab-case (the catalog's convention): `allInOne` is `capture-key-all-in-one`.
		const rowId = (field) => `capture-key-${field.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
		// The Rust keys are the authority on which fields exist.
		const fields = Object.keys(JSON.parse(read("__fixtures__/capture-prefs.json")).prefs.keys);
		const seen = [];
		for (const m of settings.matchAll(/field: "(\w+)",\s*label: "([^"]+)",\s*description: "([^"]+)"/g)) {
			seen.push(m[1]);
			const it = items[rowId(m[1])];
			expect([it?.label, it?.description], m[1]).toEqual([m[2], m[3]]);
		}
		expect(seen.sort()).toEqual(fields.sort());
	});

	test("the search keywords the feature promises are all reachable", () => {
		const kw = section.pages[0].groups.flatMap((g) => g.items.map((i) => i.keywords ?? "")).join(" ");
		for (const w of "capture screenshot screen recording gif hotkey region window quick access history folder".split(
			" ",
		))
			expect(kw, w).toContain(w);
	});
});
