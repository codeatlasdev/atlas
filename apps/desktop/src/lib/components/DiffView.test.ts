// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";

// Incident (space switch froze the page): a space switch changes `focused`, and TabGroupView feeds DiffView through
// `<Lazy props={{ worktreeId, focused }}>`, an object literal rebuilt on every change. The view reloaded each time with
// byte-identical arguments (measured on a live switch: 4 `git_diff` for 2 involved spaces, one space 3 times), and every
// reload is a `git diff`, a reset of all highlighted rows and a rewrite of the table (133k node mutations for a 400-line
// file per space). A reload is for a different worktree, mode or base, never for focus.

// The stores a lazily loaded view pulls in call Tauri at import time; there is no Tauri in a test.
// Set before the dynamic imports below (a static import would run first).
window.__TAURI_INTERNALS__ = { invoke: async () => null, transformCallback: () => 0 };

let diffCalls: string[][];
// Keep every other export of the real module (the stores import `api`); replace only the two calls the view makes.
const realApi = await import("#lib/api.ts");
mock.module("#lib/api.ts", () => ({
	...realApi,
	git: {
		...realApi.git,
		diff: async (id: string, mode: string, base?: string) => {
			diffCalls.push([id, mode, base ?? ""]);
			return { base: "main", files: [] };
		},
		run: async () => ({ code: 0, stdout: "", stderr: "" }),
	},
}));
mock.module("#lib/components/diff/highlight.ts", () => ({ highlightFile: async () => {} }));

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Harness } = await import("../../../test/DiffViewHarness.svelte");
// `Lazy` renders nothing until its `import()` resolves, and that needs the event loop, not just microtasks. Loading the
// module here first means the harness's own import resolves from the cache within microtasks.
await import("#lib/components/DiffView.svelte");

let done: (() => void) | undefined;
beforeEach(() => (diffCalls = []));
afterEach(() => {
	done?.();
	done = undefined;
});

/** Let effects run and promise chains resolve: turns of the microtask queue, no wall clock. */
async function settle() {
	for (let i = 0; i < 10; i++) {
		flushSync();
		await tick();
		await Promise.resolve();
	}
}

/** `Lazy` imports the view with a real `import()`: wait for what it renders, not for a guessed number of turns. */
async function mounted(target: Element) {
	for (let i = 0; i < 2000 && !target.querySelector(".diff"); i++) await settle();
	expect(target.querySelector(".diff")).not.toBeNull();
}

async function setup(worktreeId: string) {
	const target = document.createElement("div");
	document.body.append(target);
	let set;
	const app = flushSync(() => mount(Harness, { target, props: { worktreeId, expose: (s) => (set = s) } }));
	done = () => (unmount(app), target.remove());
	await mounted(target);
	await settle();
	return set;
}

test("loads once on mount", async () => {
	await setup("w1");
	expect(diffCalls).toEqual([["w1", "base", ""]]);
});

test("flipping `focused` through a rebuilt props object does not reload the diff", async () => {
	const set = await setup("w1");
	for (const f of [true, false, true, false]) {
		set.focused(f);
		await settle();
	}
	expect(diffCalls).toHaveLength(1);
});
