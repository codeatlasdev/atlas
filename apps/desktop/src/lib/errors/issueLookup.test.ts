// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, mock, test } from "bun:test";

const invoked: [string, unknown][] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: unknown) => {
		invoked.push([cmd, args]);
		return {
			kind: "found",
			number: 9,
			url: "https://github.com/codeatlasdev/atlas/issues/9",
			title: "Crash 9",
		};
	},
}));
// Dynamic on purpose: the module binds `invoke` when it loads.
const { findExisting, lookupIssue, meTooUrl, parseLookup } = await import("./issueLookup.ts");

// The real producer values: `serde_json::to_value(issue_search::Lookup)` (pinned by
// `issue_search::tests::the_lookup_serializes_the_keys_the_page_reads`).
const FOUND = {
	kind: "found",
	number: 9,
	url: "https://github.com/codeatlasdev/atlas/issues/9",
	title: "Crash 9",
};
const NONE = { kind: "none" };
const LIMITED = { kind: "unavailable", reason: "rate-limited" };

test("what the backend serializes is read, and anything else is no answer", () => {
	expect(parseLookup(FOUND)).toEqual(FOUND);
	expect(parseLookup(NONE)).toEqual(NONE);
	expect(parseLookup(LIMITED)).toEqual({ kind: "unavailable" });
	for (const bad of [
		null,
		undefined,
		"x",
		{},
		{ kind: "found" },
		{ ...FOUND, number: "9" },
		{ ...FOUND, url: 5 },
		{ ...FOUND, url: "javascript:alert(1)" },
		{ ...FOUND, url: "https://evil.example/issues/9" },
	])
		expect(parseLookup(bad)).toEqual({ kind: "unavailable" });
});

test("only an issue of this repo is offered, so the link opened is always the project's", () => {
	expect(
		parseLookup({ ...FOUND, url: "https://github.com/codeatlasdev/atlas/issues/9#issuecomment-1" }).kind,
	).toBe("unavailable");
});

test("looking up asks for the fingerprint, and a failing call is an unavailable search, never a throw", async () => {
	const calls: unknown[] = [];
	const ask = async (fp: string) => (calls.push(fp), FOUND);
	expect(await lookupIssue("0123456789abcdef", ask)).toEqual(FOUND);
	expect(calls).toEqual(["0123456789abcdef"]);
	expect(
		await lookupIssue("0123456789abcdef", async () => {
			throw new Error("command issue_search not found");
		}),
	).toEqual({ kind: "unavailable" });
});

test("me too opens the issue at its new-comment box", () => {
	expect(meTooUrl(FOUND)).toBe("https://github.com/codeatlasdev/atlas/issues/9#issuecomment-new");
});

test("a report is looked up by its fingerprint, computed when it has none, through the issue_search command", async () => {
	const report = { message: "boom", stack: "", where: "x" };
	const asked: string[] = [];
	await findExisting(report, async (fp) => (asked.push(fp), { kind: "none" }));
	expect(asked).toHaveLength(1);
	expect(asked[0]).toMatch(/^[0-9a-f]{16}$/);
	// A report that carries one (a crash file's) is searched by it, and the default search is the real command.
	invoked.length = 0;
	expect(await findExisting({ ...report, fingerprint: "0123456789abcdef" })).toEqual(FOUND);
	expect(invoked).toEqual([["issue_search", { fingerprint: "0123456789abcdef" }]]);
});
