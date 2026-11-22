// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeEach, expect, mock, test } from "bun:test";

const calls: [string, unknown][] = [];
let pending: unknown = null;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: unknown) => {
		calls.push([cmd, args]);
		if (cmd === "crash_card_pending") {
			if (pending instanceof Error) throw pending;
			return pending;
		}
		return null;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

const saved = Object.getOwnPropertyDescriptor(globalThis, "$state");
Object.defineProperty(globalThis, "$state", { value: (v) => v, configurable: true, writable: true });
afterAll(() => {
	if (saved) Object.defineProperty(globalThis, "$state", saved);
	else delete globalThis.$state;
});

// Dynamic on purpose: the store reads the `$state` stub and the mocked `invoke` at load time.
const { crashCards } = await import("./crashCard.svelte.ts");
const { copy, crashReport, parseCard } = await import("./crashCard.ts");
const { headline } = await import("./report.ts");

// The real producer value: `serde_json::to_value(crash_card::Card)` (pinned by `crash_card::tests::the_card_serializes_the_keys_the_page_reads`).
const REAL = { atMs: 1_800_000_000_000, kind: "crash", summary: "atlas 1.5.0: boom (src/main.rs:1:1)" };

beforeEach(() => {
	calls.length = 0;
	crashCards.card = null;
	pending = null;
});

test("the card the backend serializes is read, and anything else is no card", () => {
	expect(parseCard(REAL)).toEqual(REAL);
	expect(parseCard({ ...REAL, kind: "hostd-exited" })?.kind).toBe("hostd-exited");
	for (const bad of [
		null,
		undefined,
		"x",
		3,
		{},
		{ ...REAL, atMs: "1" },
		{ ...REAL, kind: "other" },
		{ ...REAL, summary: 1 },
		{ ...REAL, atMs: NaN },
	])
		expect(parseCard(bad)).toBeNull();
});

test("load shows the card the backend found, and a failing or empty answer shows nothing", async () => {
	pending = REAL;
	await crashCards.load();
	expect(crashCards.card).toEqual(REAL);
	pending = null;
	await crashCards.load();
	expect(crashCards.card).toBeNull();
	pending = new Error("command not found");
	crashCards.card = REAL;
	await crashCards.load();
	expect(crashCards.card).toBeNull();
});

// #322: "dismiss persists": the card goes at once and the stamp moves to ITS time (not "now": a crash that happened
// while the card was open stays news).
test("dismiss clears the card and acknowledges exactly its time", async () => {
	crashCards.card = REAL;
	await crashCards.dismiss();
	expect(crashCards.card).toBeNull();
	expect(calls).toEqual([["crash_card_dismiss", { atMs: REAL.atMs }]]);
	await crashCards.dismiss();
	expect(calls).toHaveLength(1);
});

// #322: "Report uses the redacted summary": a home path or token that reached the summary is still removed.
test("the report carries the summary with secrets and home paths removed, and the card's own headline", () => {
	const r = crashReport({
		...REAL,
		summary: "atlas 1.5.0: failed in /home/ana/work TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
	});
	expect(r.message).not.toContain("/home/ana");
	expect(r.message).not.toContain("ghp_abcdefghijklmnopqrstuvwxyz0123456789");
	expect(headline(r).title).toBe("Atlas closed unexpectedly last time");
	expect(headline(crashReport({ ...REAL, kind: "hostd-exited" })).title).toBe(
		"The terminal host closed unexpectedly",
	);
	expect(copy(REAL).body).toContain("Nothing was sent");
});
