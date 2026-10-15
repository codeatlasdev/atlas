// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Glob } from "bun";

// Incident (desktop flicker): the window is transparent (Tauri `transparent: true`), and a `backdrop-filter` over
// content that moves re-renders the blur on every frame of a space swipe and recomposites the whole window
// (tauri-apps/tauri#15471). The blurred surfaces here sit over the pager or inside a page it translates, and their
// backgrounds are near-opaque, so the blur showed nothing. Do not bring it back on these; a new surface that overlaps
// the pager must be opaque instead.
const OVER_PAGER = [
	"src/app.css",
	"src/lib/panel/ActionResultToast.svelte",
	"src/lib/errors/ErrorToasts.svelte",
	"src/lib/onboarding/FirstRun.svelte",
	"src/lib/onboarding/Checklist.svelte",
	"src/lib/viewers/pdf/PdfToolbar.svelte",
];

/** Declarations that turn a blur ON (`none` is how a rule switches the kit's blur off). */
const blur = /(?:-webkit-)?backdrop-filter\s*:\s*(?!none\b)\S[^;]*;/g;

for (const file of OVER_PAGER) {
	test(`${file} paints no backdrop-filter`, async () => {
		const css = await Bun.file(new URL(`../../../${file}`, import.meta.url)).text();
		expect(css.match(blur) ?? []).toEqual([]);
	});
}

test("the guard list names real files", async () => {
	const found = new Set<string>();
	for await (const f of new Glob("src/**/*.{svelte,css}").scan({
		cwd: new URL("../../../", import.meta.url).pathname,
	}))
		found.add(f);
	expect(OVER_PAGER.filter((f) => !found.has(f))).toEqual([]);
});
