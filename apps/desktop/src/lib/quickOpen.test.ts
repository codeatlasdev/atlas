// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { fuzzy } from "./fuzzy.ts";
import { FileIndex, indexFor } from "./quickOpen.ts";

/** What Palette did on every keystroke before the index: score everything, drop the misses, stable sort, keep `limit`. */
function reference(files: string[], query: string, limit: number): number[] {
	const q = query.trim();
	const rows = files.map((f, i) => {
		const slash = f.lastIndexOf("/");
		return { i, label: f.slice(slash + 1), description: slash > 0 ? f.slice(0, slash) : undefined };
	});
	if (!q) return rows.slice(0, limit).map((r) => r.i);
	return rows
		.map((r) => ({ r, s: fuzzy(q, r.label, r.description) }))
		.filter((x) => x.s > 0)
		.sort((a, b) => b.s - a.s)
		.slice(0, limit)
		.map((x) => x.r.i);
}

/** A repeatable pile of paths shaped like a monorepo: deep folders, repeated names, a few awkward ones. */
export function corpus(n: number): string[] {
	let seed = 42;
	const rnd = (m: number) => ((seed = (seed * 1664525 + 1013904223) >>> 0), seed % m);
	const dirs = [
		"src",
		"lib",
		"packages",
		"apps",
		"core",
		"ui",
		"components",
		"stores",
		"test",
		"docs",
		"crates",
		"node_modules",
	];
	const stems = [
		"index",
		"main",
		"controller",
		"service",
		"utils",
		"button",
		"Palette",
		"SourceControl",
		"README",
		"mod",
		"types",
	];
	const exts = ["ts", "svelte", "rs", "md", "json", "toml", "css"];
	const out: string[] = [];
	for (let i = 0; i < n; i++) {
		const depth = 1 + rnd(5);
		const parts = Array.from({ length: depth }, () => `${dirs[rnd(dirs.length)]}${rnd(40)}`);
		out.push(`${parts.join("/")}/${stems[rnd(stems.length)]}${rnd(300)}.${exts[rnd(exts.length)]}`);
	}
	out.push(
		"README.md",
		"Cargo.toml",
		"ünïcode/Ünï Çode.ts",
		"a b/c d e.md",
		"x".repeat(300) + "/ab.ts",
		"dot.file/.env",
	);
	return out;
}

const QUERIES = [
	"",
	"a",
	"ct",
	"ctrl",
	"controller",
	"src ts",
	"SC",
	"sourcecontrol",
	"rs",
	"readme",
	"ab",
	"ç",
	"/",
	"z9z9",
	"node cont",
	"ünï",
];

describe("FileIndex ranks exactly like fuzzy + a stable sort", () => {
	const files = corpus(4000);
	test("every query, fresh index", () => {
		for (const q of QUERIES) expect(new FileIndex(files).top(q, 80)).toEqual(reference(files, q, 80));
	});
	test("the same index typed into letter by letter and backspaced", () => {
		const idx = new FileIndex(files);
		for (const word of ["controller", "sourcecontrol", "node cont"]) {
			for (let i = 1; i <= word.length; i++)
				expect(idx.top(word.slice(0, i), 80)).toEqual(reference(files, word.slice(0, i), 80));
			for (let i = word.length; i >= 1; i--)
				expect(idx.top(word.slice(0, i), 80)).toEqual(reference(files, word.slice(0, i), 80));
		}
	});
	// A path-only match of a long query can score above 1000, i.e. above name matches: the "enough names matched, skip the
	// paths" shortcut must not apply there.
	test("a 45-character query that only a path matches still ranks like fuzzy", () => {
		const idx = new FileIndex(files);
		const q = files[7].toLowerCase().slice(0, 45);
		expect(idx.top("c", 80)).toEqual(reference(files, "c", 80));
		expect(idx.top(q, 80)).toEqual(reference(files, q, 80));
		expect(reference(files, q, 80)[0]).toBe(7);
	});
	test("a different query after a long one is not narrowed by it", () => {
		const idx = new FileIndex(files);
		idx.top("controller", 80);
		expect(idx.top("service", 80)).toEqual(reference(files, "service", 80));
		expect(idx.top("c", 80)).toEqual(reference(files, "c", 80));
	});
	test("limits: smaller than the matches, larger than the matches, zero matches, an empty list", () => {
		expect(new FileIndex(files).top("ts", 5)).toEqual(reference(files, "ts", 5));
		expect(new FileIndex(files).top("readme", 80)).toEqual(reference(files, "readme", 80));
		expect(new FileIndex(files).top("qqqqqqqq", 80)).toEqual([]);
		expect(new FileIndex([]).top("a", 80)).toEqual([]);
		expect(new FileIndex([]).top("", 80)).toEqual([]);
	});
	// fuzzy() subtracts the path length from a path-only match, so a very long path with a short hit scores <= 0 and is
	// not listed, yet "ab" can score above 0 where "a" did not: narrowing by the previous query's *scores* would lose it.
	test("a long path that missed a short query can match the longer one", () => {
		const long = ["x".repeat(400) + "/a-b.ts", "src/a-b.ts"];
		const idx = new FileIndex(long);
		expect(idx.top("a", 80)).toEqual(reference(long, "a", 80));
		expect(idx.top("ab", 80)).toEqual(reference(long, "ab", 80));
		expect(idx.top("ab", 80)).toEqual(reference(long, "ab", 80));
	});
});

describe("indexFor: one index per file list", () => {
	test("the same list gives the same index, a new list (the files changed) a new one that sees the change", () => {
		const a = ["src/old.ts", "src/keep.ts"];
		const first = indexFor(a);
		expect(indexFor(a)).toBe(first);
		const b = [...a, "src/brand-new.ts"];
		const second = indexFor(b);
		expect(second).not.toBe(first);
		expect(second.top("brand", 80).map((i) => b[i])).toEqual(["src/brand-new.ts"]);
		expect(first.top("brand", 80)).toEqual([]);
	});
});

// Regression #308: Quick Open rebuilt an item per file (icon markup included) and scored, sorted and sliced all of them on
// every keystroke. The guard counts the work instead of timing it (a wall-clock budget, then a ratio to the old scoring,
// failed on a loaded machine with the index still 14x faster): a keystroke may look at the files the previous one
// matched and nothing more, and typing never reads the file list again once the index is built.
describe("100k files", () => {
	const files = corpus(100_000);
	const typed = [
		"c",
		"co",
		"con",
		"cont",
		"contr",
		"contro",
		"control",
		"controll",
		"controlle",
		"controller",
	];

	/** `files` that reports every element read, so a per-keystroke pass over the list shows. */
	function watched() {
		const log = { reads: 0 };
		const proxy = new Proxy(files, {
			get(target, key, receiver) {
				if (typeof key === "string" && /^\d+$/.test(key)) log.reads++;
				return Reflect.get(target, key, receiver);
			},
		});
		return { proxy, log };
	}

	test("typing a word narrows what each keystroke looks at, and the index is built once", () => {
		const { proxy, log } = watched();
		const idx = new FileIndex(proxy);
		expect(log.reads).toBe(files.length); // the build reads each file once...
		const scanned: number[] = [];
		for (const q of typed) {
			idx.top(q, 80);
			scanned.push(idx.scanned);
		}
		expect(log.reads).toBe(files.length); // ...and no keystroke reads the list again
		console.log(`quick-open 100k: files looked at per keystroke of "controller": ${scanned.join(" ")}`);
		// The first keystroke scans the list; every later one only what its prefix matched: never more than the keystroke
		// before, and far from the list (the pre-index palette, and an index that forgot the narrowing, scan all 100k).
		expect(scanned[0]).toBeGreaterThanOrEqual(files.length);
		for (let i = 1; i < scanned.length; i++) {
			expect(scanned[i]).toBeLessThanOrEqual(scanned[i - 1]);
			expect(scanned[i]).toBeLessThan(files.length / 2);
		}
		// Narrowing changed what is looked at, not the answer.
		expect(idx.top("controller", 80)).toEqual(reference(files, "controller", 80));
	});
});
