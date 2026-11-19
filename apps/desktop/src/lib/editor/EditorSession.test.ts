// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeAll, beforeEach, describe, expect, mock, spyOn, test } from "bun:test";
import { Text } from "@codemirror/state";
import { toast } from "@codeatlasdev/ui";
import { b64encode } from "#lib/b64.ts";
import { host } from "#lib/host.ts";
import { m } from "#lib/i18n/m.ts";

/**
 * A fake hostd file system behind `host`. `mock.module` is process-wide and `bun test` runs every file in
 * one process, so a module mock here would replace `#lib/host.ts` / the Tauri `invoke` for every suite that
 * loads after this one (the crew card tests broke exactly like that). `host` is a plain object: its fs
 * methods are swapped in `beforeAll` and put back in `afterAll`, and only the module that has no fake
 * (`listen`, the kit's `toast`) is stubbed the way the other suites stub it. Every write bumps mtime like a
 * real file system would.
 */
interface Disk {
	text: string;
	mtime: number;
}
let disk: Disk;
let clock: number;
let writes: string[];
let watchFails: boolean;
let unreadable: boolean;
/** `fs_watch` registrations and the handlers `listen` was given: the test plays the host's events. */
let watchIds: string[];
let listeners: ((e: { payload: unknown }) => void)[];

const touch = (text: string) => {
	disk = { text, mtime: ++clock };
};
const size = () => new TextEncoder().encode(disk.text).length;

mock.module("@tauri-apps/api/event", () => ({
	listen: async (_name: string, handler: (e: { payload: unknown }) => void) => {
		listeners.push(handler);
		return () => {};
	},
}));
await (await import("../../../test/uiKit.ts")).installUiKit();

const fake = {
	fsStat: async () => ({ kind: "file", size: size(), mtime: disk.mtime }),
	fsRead: async () => {
		if (unreadable) throw new Error("stream did not contain valid UTF-8");
		return { data: b64encode(disk.text), truncated: false };
	},
	fsWrite: async (_path: string, text: string) => {
		writes.push(text);
		touch(text);
	},
	fsWatch: async (_dir: string, id: string) => {
		watchIds.push(id);
		if (watchFails) throw new Error("inotify: too many open files");
	},
	fsUnwatch: async () => {},
};
const real: Record<string, unknown> = {};
beforeAll(() => {
	for (const k of Object.keys(fake)) real[k] = host[k];
	Object.assign(host, fake);
});
afterAll(() => {
	for (const k of Object.keys(fake)) {
		if (real[k] === undefined) delete host[k];
		else host[k] = real[k];
	}
});

// Dynamic on purpose: the mocks above must be in place before the session (and `host`) load.
const { EditorSession } = await import("./EditorSession.svelte.ts");

const ABS = "/w/src/a.ts";
let conflicts = 0;

async function open(initial = "one\n") {
	touch(initial);
	conflicts = 0;
	const s = new EditorSession(ABS, "src/a.ts", "/w", {
		extensions: () => [],
		onChange: () => {},
		onConflict: () => conflicts++,
	});
	const plan = await s.read(1_000_000, new AbortController().signal);
	s.mount(document.createElement("div"), plan);
	return s;
}

/** Let already-settled promises (the rejected `fs_watch`) run their handlers: microtasks only, no clock. */
const settle = async () => {
	for (let i = 0; i < 10; i++) await Promise.resolve();
};

const toastError = spyOn(toast, "error");

const type = (s, text: string) =>
	s.view.dispatch({ changes: { from: 0, to: s.view.state.doc.length, insert: text } });

beforeEach(() => {
	clock = 1000;
	writes = [];
	watchIds = [];
	listeners = [];
	watchFails = false;
	unreadable = false;
	toastError.mockClear();
});

// Regression #306: autosave wrote over an external edit whenever the watcher had not (yet) reported it.
describe("save precondition: the file on disk is what we last read or wrote", () => {
	test("dirty buffer, disk changed behind our back: save raises a conflict and writes nothing", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("theirs, longer\n"); // an agent / git checkout, before the watcher event arrives

		await s.save();

		expect(writes).toEqual([]);
		expect(disk.text).toBe("theirs, longer\n");
		expect(s.conflict).toEqual({ text: "theirs, longer\n" });
		expect(conflicts).toBe(1);
		expect(s.dirty).toBe(true);
		await s.dispose();
	});

	test("same size, only the mtime moved: still a conflict", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("two\n"); // same byte length as "one\n"

		await s.save();

		expect(writes).toEqual([]);
		expect(s.conflict).toEqual({ text: "two\n" });
		await s.dispose();
	});

	test("clean buffer, file deleted and recreated by someone else: no overwrite, their text is loaded", async () => {
		const s = await open("one\n");
		s.deleted = true; // the banner state: the file vanished, then another tool recreated it
		touch("recreated elsewhere\n");

		await s.save(); // the banner's Save button: the one way a clean buffer reaches a write

		expect(writes).toEqual([]);
		expect(disk.text).toBe("recreated elsewhere\n");
		expect(s.view.state.doc.toString()).toBe("recreated elsewhere\n");
		expect(s.dirty).toBe(false);
		await s.dispose();
	});

	test("untouched disk: the save goes through and the next save does not trip on our own write", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		await s.save();
		type(s, "mine again\n");
		await s.save();

		expect(writes).toEqual(["mine\n", "mine again\n"]);
		expect(s.conflict).toBeNull();
		expect(s.dirty).toBe(false);
		await s.dispose();
	});

	test("'Keep my version' overwrites the other change on purpose (force)", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("theirs\n");
		await s.save();
		expect(writes).toEqual([]);

		await s.resolve("keep");

		expect(writes).toEqual(["mine\n"]);
		expect(s.conflict).toBeNull();
		// ...and afterwards the new baseline is our own write, so saving again works.
		type(s, "mine 2\n");
		await s.save();
		expect(writes).toEqual(["mine\n", "mine 2\n"]);
		await s.dispose();
	});
});

describe("a save the disk does not actually contradict still goes through", () => {
	// Same bytes, newer mtime: `touch`, git restoring the content, a formatter writing identical output.
	test("same text with a newer mtime: the save writes and the buffer is clean afterwards", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("one\n");

		await s.save();

		expect(writes).toEqual(["mine\n"]);
		expect(s.conflict).toBeNull();
		expect(s.dirty).toBe(false);
		type(s, "mine again\n");
		await s.save(); // the baseline is now our own write
		expect(writes).toEqual(["mine\n", "mine again\n"]);
		await s.dispose();
	});

	test("closing the tab after a touch still writes the user's text", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("one\n");

		await s.dispose();

		expect(writes).toEqual(["mine\n"]);
		expect(toastError).not.toHaveBeenCalled();
	});

	test("a file that cannot be read is not overwritten blind: the user is told", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("changed\n");
		unreadable = true; // non-UTF-8 or permissions: `#readDisk` has nothing to compare

		await s.save();

		expect(writes).toEqual([]);
		expect(s.dirty).toBe(true);
		expect(toastError).toHaveBeenCalledTimes(1);
		expect(toastError.mock.calls[0][0]).toBe(m.workbench_editor_not_saved_unreadable({ path: "src/a.ts" }));
		await s.dispose();
	});
});

// Regression #306 review: closing the tab dropped the unsaved text without a word when the file had changed.
describe("closing with unsaved text the disk contradicts", () => {
	test("external change: nothing is written and a toast names the file", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("theirs, longer\n");

		await s.dispose();

		expect(writes).toEqual([]);
		expect(disk.text).toBe("theirs, longer\n");
		expect(conflicts).toBe(0); // the editor is going away: no conflict prompt
		expect(toastError).toHaveBeenCalledTimes(1);
		expect(toastError).toHaveBeenCalledWith(m.workbench_editor_not_saved_changed({ path: "src/a.ts" }));
	});

	test("a conflict still open when the tab closes is announced too", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("theirs, longer\n");
		await s.save();
		expect(s.conflict).not.toBeNull();
		toastError.mockClear();

		await s.dispose();

		expect(writes).toEqual([]);
		expect(toastError).toHaveBeenCalledWith(m.workbench_editor_not_saved_changed({ path: "src/a.ts" }));
	});

	test("a clean buffer closes quietly", async () => {
		const s = await open("one\n");
		touch("theirs, longer\n");
		await s.dispose();
		expect(toastError).not.toHaveBeenCalled();
	});
});

describe("watcher failure", () => {
	test("fs_watch rejecting sets a visible notice instead of being swallowed, and save still checks the disk", async () => {
		watchFails = true;
		const s = await open("one\n");
		await settle();

		expect(s.watchError).not.toBe("");

		type(s, "mine\n");
		touch("theirs, longer\n");
		await s.save();

		expect(writes).toEqual([]);
		expect(s.conflict).toEqual({ text: "theirs, longer\n" });
		await s.dispose();
	});

	test("a working watcher shows no notice", async () => {
		const s = await open("one\n");
		await settle();
		expect(s.watchError).toBe("");
		await s.dispose();
	});
});

/** The watcher reports `ABS` changed, then waits out the editor's debounce. */
async function watcherSaysChanged() {
	for (const l of listeners) l({ payload: { ev: "fs_event", id: watchIds.at(-1), paths: [ABS] } });
	await new Promise((r) => setTimeout(r, 250));
}

/**
 * Whole-document reads of the CodeMirror rope (`sliceString(0, length)`, which is what `doc.toString()` is),
 * counted at the rope so every caller is seen. A doc that grew by a few characters still counts: `docLength`
 * is the size when the probe started.
 */
function countWholeDocCopies(docLength: number) {
	let copies = 0;
	const protos = [Text.of(["a"]), Text.of(Array.from({ length: 200 }, () => "line"))].map((t) =>
		Object.getPrototypeOf(t),
	);
	const originals = protos.map((p) => p.sliceString);
	protos.forEach((p, i) => {
		p.sliceString = function (from: number, to = this.length, sep?: string) {
			if (from === 0 && to >= docLength) copies++;
			return originals[i].call(this, from, to, sep);
		};
	});
	return {
		get copies() {
			return copies;
		},
		stop: () => protos.forEach((p, i) => (p.sliceString = originals[i])),
	};
}

// Regression #309: every keystroke ran doc.toString() + planSave (two to four full copies of the file).
describe("typing does not copy the document", () => {
	const line = "const answer = compute(a, b, c); // twenty-odd words of ordinary source text\n";
	const big = line.repeat(Math.ceil(2_000_000 / line.length)); // a "large"-tier 2 MB file

	test("2 MB file, 200 keystrokes: no whole-document copy until the save", async () => {
		const s = await open(big);
		const probe = countWholeDocCopies(big.length);
		try {
			for (let i = 0; i < 200; i++) {
				const at = 1000 + i;
				s.view.dispatch({ changes: { from: at, insert: "x" }, userEvent: "input.type" });
			}
			expect(probe.copies).toBe(0);
			expect(s.dirty).toBe(true);

			await s.save();

			expect(probe.copies).toBeGreaterThan(0); // the save is the one place that materializes
			expect(writes).toHaveLength(1);
			expect(writes[0].length).toBe(big.length + 200);
			expect(s.dirty).toBe(false);
		} finally {
			probe.stop();
		}
		await s.dispose();
	});

	test("typing then deleting the character leaves the buffer clean again", async () => {
		const s = await open("one\ntwo\n");
		s.view.dispatch({ changes: { from: 3, insert: "!" } });
		expect(s.dirty).toBe(true);
		s.view.dispatch({ changes: { from: 3, to: 4 } });
		expect(s.dirty).toBe(false);
		await s.dispose();
		expect(writes).toEqual([]);
	});

	test("a CRLF file stays clean after a no-op edit and dirty after a real one; changing the EOL makes it dirty", async () => {
		const s = await open("a\r\nb\r\n");
		s.view.dispatch({ changes: { from: 1, insert: "x" } });
		s.view.dispatch({ changes: { from: 1, to: 2 } });
		expect(s.dirty).toBe(false);
		s.setEol("lf");
		expect(s.dirty).toBe(true);
		s.setEol("crlf");
		expect(s.dirty).toBe(false);
		await s.dispose();
		expect(writes).toEqual([]);
	});

	test("a file the save policy would rewrite is dirty after any edit, even one that is undone", async () => {
		const s = await open("trailing   \nspaces\n"); // .editorconfig-less default: trim is off; use the plan's own policy
		s.plan = { ...s.plan, save: { trim: true, finalNewline: true } };
		s.view.dispatch({ changes: { from: 0, insert: "x" } });
		s.view.dispatch({ changes: { from: 0, to: 1 } });
		expect(s.dirty).toBe(true);
		await s.save();
		expect(writes).toEqual(["trailing\nspaces\n"]);
		expect(s.dirty).toBe(false);
		await s.dispose();
	});

	test("`text` (the rich viewers' source) follows the document without being copied per keystroke", async () => {
		const s = await open("# title\n");
		s.view.dispatch({ changes: { from: 7, insert: " two" } });
		expect(s.text).toBe("# title two\n");
		s.view.dispatch({ changes: { from: 0, insert: "x" } });
		expect(s.text).toBe("x# title two\n");
		await s.dispose();
	});
});

describe("the buffer equals what someone else wrote to the disk", () => {
	test("watcher: no conflict, the buffer is clean, undoing a typo stays clean and nothing is rewritten", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		touch("mine\n");

		await watcherSaysChanged();

		expect(s.conflict).toBeNull();
		expect(conflicts).toBe(0);
		expect(s.dirty).toBe(false);
		s.view.dispatch({ changes: { from: 0, insert: "x" } });
		s.view.dispatch({ changes: { from: 0, to: 1 } });
		expect(s.dirty).toBe(false);
		await s.save();
		expect(writes).toEqual([]);
		await s.dispose();
	});

	test("watcher: different text under a dirty buffer is still a conflict", async () => {
		const s = await open("one\n");
		type(s, "mine\n");
		type(s, "mine, second thought\n");
		touch("mine\n"); // equals the buffer as of the first edit, not the current one

		await watcherSaysChanged();

		expect(s.conflict).toEqual({ text: "mine\n" });
		expect(conflicts).toBe(1);
		expect(s.dirty).toBe(true);
		await s.dispose();
	});
});
