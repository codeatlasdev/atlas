// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeEach, describe, expect, mock, test } from "bun:test";
import { b64encode } from "#lib/b64.ts";

/**
 * A fake hostd file system behind the real `host` wrapper: `invoke("host_call")` is the only seam
 * mocked, so the session talks to the same `host.fsStat/fsRead/fsWrite/fsWatch` it uses in the app.
 * Every write bumps mtime like a real file system would.
 */
interface Disk {
	text: string;
	mtime: number;
}
let disk: Disk;
let clock: number;
let writes: string[];
let watchFails: boolean;

const touch = (text: string) => {
	disk = { text, mtime: ++clock };
};
const size = () => new TextEncoder().encode(disk.text).length;

mock.module("@tauri-apps/api/core", () => ({
	invoke: async (_name: string, { cmd, args }: { cmd: string; args: Record<string, unknown> }) => {
		switch (cmd) {
			case "fs_stat":
				return { kind: "file", size: size(), mtime: disk.mtime };
			case "fs_read":
				return { data: b64encode(disk.text), truncated: false };
			case "fs_write":
				writes.push(atob(args.data as string));
				touch(atob(args.data as string));
				return {};
			case "fs_watch":
				if (watchFails) throw new Error("inotify: too many open files");
				return {};
			default:
				return {};
		}
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../test/uiKit.ts")).installUiKit();

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

const type = (s, text: string) =>
	s.view.dispatch({ changes: { from: 0, to: s.view.state.doc.length, insert: text } });

beforeEach(() => {
	clock = 1000;
	writes = [];
	watchFails = false;
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

describe("watcher failure", () => {
	test("fs_watch rejecting sets a visible notice instead of being swallowed, and save still checks the disk", async () => {
		watchFails = true;
		const s = await open("one\n");
		await settle();

		expect(s.watchError).toMatch(/not watched/i);

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
