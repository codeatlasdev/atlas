// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixtures from "./__fixtures__/fingerprints.json";
import { fingerprint, frameKey, normalizeMessage } from "./fingerprint.ts";

interface Case {
	name: string;
	group: string;
	message: string;
	stack: string;
	fingerprint: string;
}
const cases = fixtures.cases as Case[];

describe("the contract with crates/atlas-core/src/fingerprint.rs", () => {
	// The values in the fixture are what Rust computes (and its test asserts them); the page must give the same ones
	// byte for byte, or a crash file and an issue body would name one bug by two fingerprints (#320).
	for (const c of cases) {
		test(`${c.group}: ${c.name}`, () => {
			expect(fingerprint(c.message, c.stack)).toBe(c.fingerprint);
		});
	}

	test("cases of a group agree and groups differ", () => {
		const groups = new Map<string, Set<string>>();
		for (const c of cases)
			groups.set(c.group, (groups.get(c.group) ?? new Set()).add(fingerprint(c.message, c.stack)));
		for (const [g, fps] of groups) expect({ g, n: fps.size }).toEqual({ g, n: 1 });
		expect(new Set([...groups.values()].map((s) => [...s][0])).size).toBe(groups.size);
	});
});

describe("normalizeMessage", () => {
	test("a message loses what changes between machines and versions", () => {
		expect(normalizeMessage("Cannot open /home/ana/.atlas/state-12.json: No such file (os error 2)")).toBe(
			"cannot open state-#.json no such file (os error #)",
		);
		expect(normalizeMessage("atlas 1.5.0-alpha.27 failed on port 24101 id 3fa9c2d17b04 at 0x7ffd12ab")).toBe(
			"atlas # failed on port # id # at #",
		);
		expect(normalizeMessage("request 123e4567-e89b-12d3-a456-426614174000 timed out")).toBe(
			"request # timed out",
		);
		expect(normalizeMessage("C:\\Users\\Ana\\proj\\src\\lib.rs:12:5 panicked")).toBe("lib.rs panicked");
		expect(normalizeMessage("\n\n  first line  \nsecond")).toBe("first line");
	});

	test("only ASCII is folded and trimmed, as in Rust", () => {
		// U+0130 lower-cases to two characters in JavaScript's toLowerCase(); Rust's to_ascii_lowercase leaves it.
		expect(normalizeMessage("İSTANBUL")).toBe("İstanbul");
		// U+FEFF and U+0085 are not whitespace for the contract.
		expect(normalizeMessage("\uFEFFa")).toBe("\uFEFFa");
		expect(normalizeMessage("\u0085a")).toBe("\u0085a");
	});

	test("at most 200 characters, counted as code points", () => {
		expect(Array.from(normalizeMessage("😀".repeat(300)))).toHaveLength(200);
	});
});

describe("frameKey", () => {
	test("the first app frame is the key whatever the line or the directory", () => {
		const v8 =
			"at Object.run (node_modules/svelte/src/runtime.js:10:2)\nat load (http://tauri.localhost/_app/immutable/chunks/index-AbCd1234.js:1:200)";
		const moved =
			"at Object.run (node_modules/svelte/src/runtime.js:99:9)\nat load (http://localhost:1420/_app/immutable/chunks/index-ZzYy9876.js:7:3)";
		expect(frameKey(v8)).toBe("load|index.js");
		expect(frameKey(v8)).toBe(frameKey(moved));
		expect(frameKey("run@node_modules/x/y.js:1:1\nopenFile@/Users/ana/atlas/Editor.svelte?t=1:45:6")).toBe(
			"openFile|Editor.svelte",
		);
		expect(frameKey("/home/runner/work/atlas/atlas/crates/atlas-core/src/fs.rs:88:9")).toBe(
			"|crates/atlas-core/src/fs.rs",
		);
		// No app frame: the first line stands in; no stack: nothing.
		expect(frameKey("a\nz")).toBe("|a");
		expect(frameKey("")).toBe("");
	});
});

test("a fingerprint is sixteen lower-case hex characters", () => {
	expect(fingerprint("boom", "")).toMatch(/^[0-9a-f]{16}$/);
});
