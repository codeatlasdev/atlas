// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { reduceExternal, type ExternalEvent, type ExternalState } from "./externalChange.ts";

const base: ExternalState = {
	diskText: null,
	bufferText: "saved",
	lastSaved: "saved",
	mtime: 1,
	dirty: false,
	autoReload: true,
};
const dirty: ExternalState = { ...base, bufferText: "edited", dirty: true };
const conflict: ExternalState = { ...dirty, diskText: "theirs", mtime: 2 };

interface Row {
	name: string;
	state: ExternalState;
	event: ExternalEvent;
	effect: string;
	patch: Partial<ExternalState>;
}

const rows: Row[] = [
	{
		name: "disk change identical to lastSaved is our own write",
		state: base,
		event: { kind: "disk-changed", text: "saved", mtime: 5 },
		effect: "ignore-own-write",
		patch: { mtime: 5 },
	},
	{
		name: "own write is ignored even when the buffer is dirty",
		state: dirty,
		event: { kind: "disk-changed", text: "saved", mtime: 5 },
		effect: "ignore-own-write",
		patch: { mtime: 5 },
	},
	{
		name: "clean + autoReload: reload",
		state: base,
		event: { kind: "disk-changed", text: "theirs", mtime: 5 },
		effect: "reload",
		patch: { diskText: "theirs", mtime: 5 },
	},
	{
		name: "clean without autoReload: prompt",
		state: { ...base, autoReload: false },
		event: { kind: "disk-changed", text: "theirs", mtime: 5 },
		effect: "prompt-conflict",
		patch: { diskText: "theirs", mtime: 5 },
	},
	{
		name: "dirty: prompt even with autoReload",
		state: dirty,
		event: { kind: "disk-changed", text: "theirs", mtime: 5 },
		effect: "prompt-conflict",
		patch: { diskText: "theirs", mtime: 5 },
	},
	{
		name: "disk now equals the buffer: in sync, it becomes the baseline",
		state: dirty,
		event: { kind: "disk-changed", text: "edited", mtime: 5 },
		effect: "in-sync",
		patch: { lastSaved: "edited", dirty: false, mtime: 5, diskText: null },
	},
	{
		name: "deleted (clean)",
		state: base,
		event: { kind: "disk-deleted" },
		effect: "notice-deleted",
		patch: { mtime: null, diskText: null },
	},
	{
		name: "deleted (dirty) keeps the buffer",
		state: dirty,
		event: { kind: "disk-deleted" },
		effect: "notice-deleted",
		patch: { mtime: null, diskText: null },
	},
	{
		name: "deleted clears a pending conflict",
		state: conflict,
		event: { kind: "disk-deleted" },
		effect: "notice-deleted",
		patch: { mtime: null, diskText: null },
	},
	{
		name: "saved records the new baseline and clears conflict",
		state: conflict,
		event: { kind: "saved", text: "edited", mtime: 9 },
		effect: "none",
		patch: { lastSaved: "edited", bufferText: "edited", dirty: false, mtime: 9, diskText: null },
	},
	{
		name: "edit away from lastSaved: dirty",
		state: base,
		event: { kind: "edited", text: "x" },
		effect: "none",
		patch: { bufferText: "x", dirty: true },
	},
	{
		name: "edit back to lastSaved: clean again",
		state: dirty,
		event: { kind: "edited", text: "saved" },
		effect: "none",
		patch: { bufferText: "saved", dirty: false },
	},
	{
		name: "resolve reload applies the disk text",
		state: conflict,
		event: { kind: "resolve", choice: "reload" },
		effect: "reload",
		patch: { diskText: null, bufferText: "theirs", lastSaved: "theirs", dirty: false },
	},
	{
		name: "resolve keep drops the disk text, keeps the edits",
		state: conflict,
		event: { kind: "resolve", choice: "keep" },
		effect: "none",
		patch: { diskText: null },
	},
	{
		name: "resolve compare leaves the conflict open",
		state: conflict,
		event: { kind: "resolve", choice: "compare" },
		effect: "none",
		patch: {},
	},
	{
		name: "resolve with nothing pending is a no-op",
		state: dirty,
		event: { kind: "resolve", choice: "reload" },
		effect: "none",
		patch: {},
	},
];

describe("reduceExternal transitions", () => {
	for (const r of rows) {
		test(r.name, () => {
			const before = structuredClone(r.state);
			const out = reduceExternal(r.state, r.event);
			expect(out.effect).toBe(r.effect);
			expect(out.state).toEqual({ ...r.state, ...r.patch });
			expect(r.state).toEqual(before); // never mutates its input
		});
	}
});

test("a full conflict cycle: edit, external change, keep, save", () => {
	let s = base;
	s = reduceExternal(s, { kind: "edited", text: "mine" }).state;
	const c = reduceExternal(s, { kind: "disk-changed", text: "theirs", mtime: 3 });
	expect(c.effect).toBe("prompt-conflict");
	s = reduceExternal(c.state, { kind: "resolve", choice: "keep" }).state;
	s = reduceExternal(s, { kind: "saved", text: "mine", mtime: 4 }).state;
	expect(s).toMatchObject({ bufferText: "mine", lastSaved: "mine", dirty: false, diskText: null, mtime: 4 });
	// the watcher echoing our save is ignored
	expect(reduceExternal(s, { kind: "disk-changed", text: "mine", mtime: 4 }).effect).toBe("ignore-own-write");
});

test("auto reload cycle: change, apply, echo", () => {
	const c = reduceExternal(base, { kind: "disk-changed", text: "theirs", mtime: 3 });
	expect(c.effect).toBe("reload");
	const r = reduceExternal(c.state, { kind: "resolve", choice: "reload" });
	expect(r.state.bufferText).toBe("theirs");
	expect(reduceExternal(r.state, { kind: "disk-changed", text: "theirs", mtime: 3 }).effect).toBe(
		"ignore-own-write",
	);
});
