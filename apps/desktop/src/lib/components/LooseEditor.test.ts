// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, afterEach, beforeAll, beforeEach, expect, mock, test } from "bun:test";
import { b64encode } from "#lib/b64.ts";
import { host } from "#lib/host.ts";

// Issue #499: opening one `.md` to fix a line went through the whole project UI. The light editor is the real
// `EditorSession` and the real CodeMirror view over a fake hostd file system (the same seam as EditorSession.test.ts:
// `host` is a plain object whose fs methods are swapped for the file's duration; module mocks are process-wide, so only
// `listen` and the kit are stubbed, the way the other suites do). Every other `host` call is recorded: the light editor
// must read, write and watch one file and ask hostd for nothing else (no exec, no git, no project).
let disk: { text: string; mtime: number };
let clock = 0;
let writes: string[];
let strays: string[];

const touch = (text: string) => (disk = { text, mtime: ++clock });

mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../test/uiKit.ts")).installUiKit();

const fake = {
	fsStat: async () => ({ kind: "file", size: new TextEncoder().encode(disk.text).length, mtime: disk.mtime }),
	fsRead: async () => ({ data: b64encode(disk.text), truncated: false }),
	fsWrite: async (_path: string, text: string) => {
		writes.push(text);
		touch(text);
	},
	fsWatch: async () => {},
	fsUnwatch: async () => {},
};
// The light editor reads and writes through `looseIo` (the app process, no hostd), so the fake file system is behind
// `looseIo`, and EVERY `host` method (hostd) is a recorder: opening and editing a file must not touch hostd at all.
// Another suite may have replaced `#lib/host.ts` with a partial object (module mocks are process-wide), so keys are
// assigned whether or not `host` has them, and put back (or removed) afterwards.
const { looseIo } = await import("#lib/looseIo.ts");
const realHost: Record<string, unknown> = {};
const realIo: Record<string, unknown> = {};
const stray =
	(k: string) =>
	(...args) => {
		strays.push(k);
		return Promise.reject(new Error(`unexpected host.${k}(${JSON.stringify(args)})`));
	};
beforeAll(() => {
	for (const k of new Set([...Object.keys(host), ...Object.keys(fake)])) {
		realHost[k] = host[k];
		host[k] = stray(k);
	}
	for (const k of Object.keys(fake)) {
		realIo[k] = looseIo[k];
		looseIo[k] = fake[k];
	}
});
afterAll(() => {
	for (const [k, v] of Object.entries(realHost)) v === undefined ? delete host[k] : (host[k] = v);
	Object.assign(looseIo, realIo);
});

const { flushSync, mount, unmount, tick } = await import("svelte");
const { EditorView } = await import("@codemirror/view");
const { default: LooseEditor } = await import("./LooseEditor.svelte");
const { loose } = await import("#lib/stores/loose.svelte.ts");

let done: (() => void) | undefined;
beforeEach(() => {
	writes = [];
	strays = [];
	clock = 1000;
});
afterEach(() => {
	done?.();
	done = undefined;
});

/** Let every already-settled promise (the fake hostd answers in microtasks) and Svelte's flush run; no clock. */
const settle = async () => {
	for (let i = 0; i < 40; i++) {
		flushSync();
		await tick();
	}
};

/** ⌘S: the save that does not wait for the idle autosave (that timer is covered by EditorSession.test.ts). */
const saveNow = async () => {
	window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", metaKey: true }));
	await settle();
};

// happy-dom answers `focus()` with a selectionchange that CodeMirror handles in the middle of its own update (a
// browser does not); the editor under test only focuses so the caret is ready to type.
const realFocus = EditorView.prototype.focus;
beforeAll(() => (EditorView.prototype.focus = () => {}));
afterAll(() => (EditorView.prototype.focus = realFocus));

async function open(path: string, text: string) {
	touch(text);
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(LooseEditor, { target, props: { path } }));
	done = () => (unmount(app), target.remove());
	await settle();
	const dom = target.querySelector(".cm-editor");
	return { target, view: dom ? EditorView.findFromDOM(dom) : null };
}
const button = (t: Element, label: string) => t.querySelector(`button[aria-label="${label}"]`);
const select = (view, needle: string) => {
	const from = view.state.doc.toString().indexOf(needle);
	view.dispatch({ selection: { anchor: from, head: from + needle.length } });
};

test("a Markdown file opens in the editor with its formatting bar and the file's text", async () => {
	const { target, view } = await open("/Users/m/Downloads/notes.md", "# Notes\n\nfix this line\n");
	expect(view).not.toBeNull();
	expect(view.state.doc.toString()).toBe("# Notes\n\nfix this line\n");
	const bar = target.querySelector('[role="toolbar"]');
	expect(bar?.getAttribute("aria-label")).toBe("Markdown formatting");
	for (const l of ["Bold", "Italic", "Heading", "Bulleted list", "Numbered list", "Link", "Preview"]) {
		expect(button(target, l), l).not.toBeNull();
	}
});

test("a bar button edits the document, and Save writes the edit to the file", async () => {
	const { target, view } = await open("/Users/m/Downloads/notes.md", "fix this line\n");
	select(view, "this");
	button(target, "Bold").click();
	expect(view.state.doc.toString()).toBe("fix **this** line\n");
	expect(writes).toEqual([]);
	await saveNow();
	expect(writes).toEqual(["fix **this** line\n"]);
	expect(disk.text).toBe("fix **this** line\n");
});

test("Preview shows the rendered Markdown, hides the formatting buttons' use, and Edit source comes back", async () => {
	const { target, view } = await open("/Users/m/Downloads/notes.md", "some **bold** text\n");
	button(target, "Preview").click();
	await settle();
	expect(target.querySelector("article.md strong")?.textContent).toBe("bold");
	expect(button(target, "Bold").disabled).toBe(true);
	button(target, "Edit source").click();
	await settle();
	expect(target.querySelector("article.md")).toBeNull();
	expect(button(target, "Bold").disabled).toBe(false);
	expect(view.state.doc.toString()).toBe("some **bold** text\n");
});

test("a plain text file gets no Markdown bar", async () => {
	const { target, view } = await open("/Users/m/Downloads/todo.txt", "milk\n");
	expect(view.state.doc.toString()).toBe("milk\n");
	expect(target.querySelector('[role="toolbar"]')).toBeNull();
});

test("opening and editing a file never calls hostd", async () => {
	const { target, view } = await open("/Users/m/Downloads/notes.md", "a\n");
	select(view, "a");
	button(target, "Italic").click();
	await saveNow();
	expect(strays).toEqual([]);
});

test("`atlas notes.md:3` lands on that line, once", async () => {
	loose.open("/Users/m/Downloads/long.txt", 3);
	const { view } = await open("/Users/m/Downloads/long.txt", "one\ntwo\nthree\nfour\n");
	expect(view.state.doc.lineAt(view.state.selection.main.head).number).toBe(3);
	expect(loose.takeLine("/Users/m/Downloads/long.txt")).toBeUndefined();
	loose.close("/Users/m/Downloads/long.txt");
});
