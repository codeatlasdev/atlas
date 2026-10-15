// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * The one mock of `@codeatlasdev/ui` for the suites that stub `document` and the runes by hand (hostCall,
 * hostErrors, workspace): the real kit builds its stores at import time and would throw under that stub, and those
 * suites only need `toast`. Component tests (Tip, kitOverlays) run the REAL kit through `realKit()`, because a module
 * mock is process-wide and `mock.module` on the bare specifier would hand them this object instead.
 */
import { mock } from "bun:test";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

export async function installUiKit() {
	// Every export a component in these suites' import graphs names (Tip's Tooltip, ErrorPanel's Button, …) must exist,
	// or an unrelated file that loads later in the run fails with "Export named 'X' not found": a stub component each.
	const Stub = () => {};
	mock.module("@codeatlasdev/ui", () => ({
		toast: { error() {}, info() {} },
		...Object.fromEntries(KIT_COMPONENTS.map((n) => [n, Stub])),
	}));
}

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
];

/**
 * A real kit component, loaded by its file under the package (`dist/components/<Name>/<Name>.svelte`) so a
 * `mock.module("@codeatlasdev/ui")` elsewhere in the run, which replaces the package entry, cannot replace it.
 */
export async function realKitComponent(name) {
	const root = dirname(createRequire(import.meta.url).resolve("@codeatlasdev/ui/package.json"));
	return (await import(join(root, "dist/components", name, `${name}.svelte`))).default;
}

/**
 * Point `@codeatlasdev/ui` at real kit components for the current file (a later `mock.module` of the same specifier
 * wins), for a component test whose subject imports from the package entry (`Tip` imports `Tooltip`).
 */
export async function useRealKit(...names) {
	const real = Object.fromEntries(await Promise.all(names.map(async (n) => [n, await realKitComponent(n)])));
	mock.module("@codeatlasdev/ui", () => real);
}

/** A real kit module by its path under `dist/` (plain `.js`: no component, no store), same reason as `realKitComponent`. */
export async function realKitModule(path) {
	const root = dirname(createRequire(import.meta.url).resolve("@codeatlasdev/ui/package.json"));
	return import(join(root, "dist", path));
}
