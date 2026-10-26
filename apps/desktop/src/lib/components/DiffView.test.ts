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
// `agents.svelte.ts` (imported by the view) loads the real `@tauri-apps/api/window`, whose `dpi.js` links
// `SERIALIZE_TO_IPC_FN` from `api/core`: any other test's `core` mock (`{ invoke }` only, module mocks are process-wide)
// makes that link fail, in either file order. This view never touches the window, so the window module is replaced.
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));

let diffCalls: string[][];
let logCalls: string[];
// A complete stand-in instead of the real module: workspace.test.ts replaces `#lib/api.ts` with `{ errText }` for the whole
// process, so whether `import("#lib/api.ts")` yields the real one depends on file order. These are the three names the
// view and the stores it pulls in import; `api` is only called from `init()`s this test never runs.
mock.module("#lib/api.ts", () => ({
	api: {},
	errText: String,
	git: {
		diff: async (id: string, mode: string, base?: string) => {
			diffCalls.push([id, mode, base ?? ""]);
			return { base: "main", files: [] };
		},
		run: async (id: string, args: string[]) => {
			if (args[0] === "log") logCalls.push(id);
			return { code: 0, stdout: "", stderr: "" };
		},
	},
}));
// Terminal.test.ts and editor/settings.test.ts replace the prefs store with only the parts they read (process-wide), so
// the view's `prefs.value.diff.view` and its `savePref` -> `setPref` link depend on file order. This test owns both.
mock.module("#lib/stores/prefs.svelte.ts", () => ({
	prefs: { value: { diff: { view: "unified" } } },
	setPref: async () => {},
	loadPrefs: async () => {},
}));
mock.module("#lib/settings/savePref.ts", () => ({ savePref: () => {} }));
mock.module("#lib/components/diff/highlight.ts", () => ({ highlightFile: async () => {} }));

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Harness } = await import("../../../test/DiffViewHarness.svelte");
// `Lazy` renders nothing until its `import()` resolves, and that needs the event loop, not just microtasks. Loading the
// module here first means the harness's own import resolves from the cache within microtasks.
await import("#lib/components/DiffView.svelte");

let done: (() => void) | undefined;
beforeEach(() => ((diffCalls = []), (logCalls = [])));
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

test("flipping `focused` does not refetch the commit list either (the base picker's `git log`)", async () => {
	const set = await setup("w1");
	expect(logCalls).toEqual(["w1"]);
	for (const f of [true, false, true, false]) {
		set.focused(f);
		await settle();
	}
	expect(logCalls).toEqual(["w1"]);
});
