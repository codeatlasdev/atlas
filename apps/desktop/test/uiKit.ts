// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * The one mock of `@codeatlasdev/ui` for the suites that stub `document` and the runes by hand (hostCall,
 * hostErrors, workspace): the real kit builds its stores at import time and would throw under that stub, and those
 * suites only need `toast`. Component tests (Tip, kitOverlays) get real components through `useRealKit` /
 * `realKitComponent`, which extend this same module instead of replacing it.
 */
import { mock } from "bun:test";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

// A module mock is process-wide and the last `mock.module` of the specifier wins, so the stubs and the real
// components are ONE module that every call extends: a suite that asks for a real `Tooltip` must not take `toast`
// away from the suite that loads after it ("Export named 'toast' not found", which also skipped that suite's `afterAll`
// and left its stub `document` in place for every file after it).
const KIT_COMPONENTS = [
	"Button",
	"Dialog",
	"Tooltip",
	"IconButton",
	"Spinner",
	"Kbd",
	"Select",
	"Switch",
	"Chip",
	"DropdownMenu",
	"DropdownMenuItem",
	"Popover",
];
const Stub = () => {};
const kit = {
	toast: { error() {}, info() {} },
	...Object.fromEntries(KIT_COMPONENTS.map((n) => [n, Stub])),
};
const publish = () => mock.module("@codeatlasdev/ui", () => ({ ...kit }));

export async function installUiKit() {
	// Every export a component in these suites' import graphs names (Tip's Tooltip, ErrorPanel's Button, …) must exist,
	// or an unrelated file that loads later in the run fails with "Export named 'X' not found": a stub component each.
	publish();
}

/**
 * A real kit component, loaded by its file under the package (`dist/components/<Name>/<Name>.svelte`) so a
 * `mock.module("@codeatlasdev/ui")` elsewhere in the run, which replaces the package entry, cannot replace it.
 * A sub-component that shares its parent's folder is `Dir/Name` (`DropdownMenu/DropdownMenuItem`); its export is `Name`.
 */
export async function realKitComponent(name) {
	const root = dirname(createRequire(import.meta.url).resolve("@codeatlasdev/ui/package.json"));
	const [dir, file = dir] = name.split("/");
	return (await import(join(root, "dist/components", dir, `${file}.svelte`))).default;
}

/**
 * Point `@codeatlasdev/ui` at real kit components for the current file (a later `mock.module` of the same specifier
 * wins), for a component test whose subject imports from the package entry (`Tip` imports `Tooltip`).
 */
export async function useRealKit(...names) {
	Object.assign(
		kit,
		Object.fromEntries(
			await Promise.all(names.map(async (n) => [n.split("/").pop(), await realKitComponent(n)])),
		),
	);
	publish();
}

/** A real kit module by its path under `dist/` (plain `.js`: no component, no store), same reason as `realKitComponent`. */
export async function realKitModule(path) {
	const root = dirname(createRequire(import.meta.url).resolve("@codeatlasdev/ui/package.json"));
	return import(join(root, "dist", path));
}
