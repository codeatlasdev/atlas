// @ts-nocheck bun test runtime types are not part of the app tsconfig
// The host messages load on demand (they are not in the startup bundle). While they load a coded text must never show
// as English in another language, and no event may be lost or reordered. `bun test` runs every file in one process, so
// each test starts from the startup state (`unloadHostMessages`) and leaves the messages loaded for the next file.
import { afterAll, beforeEach, expect, test } from "bun:test";
import { flushSync } from "svelte";
import { effect_root, render_effect } from "svelte/internal/client";
import { inLocale } from "../../test/locale.ts";

const closed = { code: "hostd.closed", message: "The terminal host closed the connection" };
const refused = { code: "hostd.refused", args: { why: "unauthorized" }, message: "hostd: unauthorized" };

const t = await import("./hostText.ts");
const { HostError } = await import("./hostMessages.ts");
const { unloadHostMessages } = await import("./hostMessagesLoad.svelte.ts");
beforeEach(unloadHostMessages);
afterAll(t.loadHostMessages);
const portuguese = async (w) => {
	const { render } = await import("./hostMessages.ts");
	return inLocale("pt-BR", () => render(w.code, w.args));
};

test("a toast that arrives before the messages load is shown in Portuguese once, never in English", async () => {
	const want = await portuguese(closed);
	expect(want).not.toBe(closed.message);
	await inLocale("pt-BR", async () => {
		// Nothing is written while loading: neither the English `message` nor a half-built sentence.
		expect(t.hostText(closed)).toBe("");
		const shown = await t.hostTextLoaded(closed);
		expect(shown).toBe(want);
		expect(t.hostText(closed)).toBe(want);
	});
});

test("events that arrive while the messages load are all written, in order", async () => {
	const order: string[] = [];
	await inLocale("pt-BR", () =>
		Promise.all([
			t.hostTextLoaded(closed).then((s) => order.push(s)),
			t.hostTextLoaded(refused).then((s) => order.push(s)),
		]),
	);
	expect(order).toEqual([await portuguese(closed), await portuguese(refused)]);
});

test("a reactive reader shows nothing, then the sentence when the messages land", async () => {
	const seen: string[] = [];
	let off: () => void = () => {};
	await inLocale("pt-BR", async () => {
		off = effect_root(() => {
			render_effect(() => void seen.push(t.hostText(closed)));
		});
		flushSync();
		expect(seen).toEqual([""]);
		await t.loadHostMessages();
		flushSync();
		expect(seen).toEqual(["", await portuguese(closed)]);
	});
	off();
});

test("an unknown code shows the English of the wire once loaded", async () => {
	const w = { code: "hostd.from_the_future", message: "a sentence this build lacks" };
	expect(await t.hostTextLoaded(w)).toBe(w.message);
});

test("a host failure is told by its code, with or without the messages", async () => {
	// Built by hand, before anything is loaded: no catalog involved.
	expect(t.isHostFailure(new HostError(closed))).toBe(true);
	expect(t.isHostFailure(new HostError(refused))).toBe(true);
	expect(t.isHostFailure(new HostError({ code: "hostd.remote_connecting", args: { host: "ssh:box" } }))).toBe(
		false,
	);
	// A rejection from `host_call` waits for the messages and is the same kind of error.
	const e = await t.hostRejection(closed);
	expect(e).toBeInstanceOf(HostError);
	expect(t.isHostFailure(e)).toBe(true);
	expect(e.message).not.toBe("");
});
