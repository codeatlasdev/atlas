// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { docState, gitBlobSha, SOURCE_FILE, SOURCE_SHA, sourceHistoryUrl } from "./doc-translation";

// The contract: `source_sha` in docs/pt-BR/*.md is what `git hash-object` prints for the English file, so the tracker the
// site uses and the one a human runs in a terminal agree. Ask the real git, never a hand-copied hash.
const gitHash = (bytes: Uint8Array): string => {
	const p = Bun.spawnSync(["git", "hash-object", "--no-filters", "--stdin"], { stdin: bytes });
	if (p.exitCode !== 0) throw new Error(p.stderr.toString());
	return p.stdout.toString().trim();
};

describe("gitBlobSha", () => {
	const cases: [string, Uint8Array][] = [
		["empty file", new Uint8Array()],
		["one byte", new Uint8Array([10])],
		["ascii with a trailing newline", new TextEncoder().encode("# Title\n\nBody\n")],
		// The header carries the BYTE length: a string length would make these two differ from git.
		["multibyte utf-8 (ã, ·, an emoji)", new TextEncoder().encode("Você · 日本語 🚀\n")],
		["CRLF bytes are hashed as they are", new TextEncoder().encode("a\r\nb\r\n")],
		["a NUL byte inside", new Uint8Array([97, 0, 98])],
	];
	for (const [name, bytes] of cases) test(`equals git hash-object: ${name}`, () => expect(gitBlobSha(bytes)).toBe(gitHash(bytes)));

	test("equals git hash-object on a real doc of this repo", () => {
		const bytes = readFileSync(new URL("../../../docs/getting-started.md", import.meta.url));
		expect(gitBlobSha(bytes)).toBe(gitHash(bytes));
	});

	test("one changed byte changes the sha (it is a content hash)", () => {
		const a = new TextEncoder().encode("same\n");
		const b = new TextEncoder().encode("sama\n");
		expect(gitBlobSha(a)).not.toBe(gitBlobSha(b));
	});
});

describe("docState: which banner a docs page gets", () => {
	const sha = gitBlobSha(new TextEncoder().encode("english v1\n"));
	const changed = gitBlobSha(new TextEncoder().encode("english v2\n"));

	test("no translation is missing (English body plus the untranslated banner)", () => {
		expect(docState(undefined, sha)).toBe("missing");
	});
	test("a translation made from the current English file is ok (no banner)", () => {
		expect(docState({ source_sha: sha }, sha)).toBe("ok");
	});
	test("a translation made from an older English file is outdated (banner with the history link)", () => {
		expect(docState({ source_sha: sha }, changed)).toBe("outdated");
	});
	test("a current translation flagged machine is machine (banner until a person reviews it)", () => {
		expect(docState({ source_sha: sha, translation: "machine" }, sha)).toBe("machine");
	});
	test("an outdated machine translation is outdated: that banner wins", () => {
		expect(docState({ source_sha: sha, translation: "machine" }, changed)).toBe("outdated");
	});
	test("an uppercase copy of the sha is a different value, not a match: the checker requires lowercase hex", () => {
		expect(docState({ source_sha: sha.toUpperCase() }, sha)).toBe("outdated");
		expect(SOURCE_SHA.test(sha.toUpperCase())).toBe(false);
		expect(SOURCE_SHA.test(sha)).toBe(true);
	});
});

describe("front matter shapes", () => {
	test("source is a bare file name in docs/, never a path", () => {
		expect(SOURCE_FILE.test("getting-started.md")).toBe(true);
		expect(SOURCE_FILE.test("../README.md")).toBe(false);
		expect(SOURCE_FILE.test("perf/x.md")).toBe(false);
		expect(SOURCE_FILE.test("getting-started")).toBe(false);
	});
	test("the history link points at the English source file on main", () => {
		expect(sourceHistoryUrl("getting-started")).toBe("https://github.com/codeatlasdev/atlas/commits/main/docs/getting-started.md");
	});
});
