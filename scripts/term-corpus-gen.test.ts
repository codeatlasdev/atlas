// The parser bench (`crates/atlas-term/benches/vt_parse.rs`) is only comparable across machines and commits if every
// run parses the same bytes, and only means what the baseline says if each corpus has the shape its name promises
// (a `seq` that is not one-number-per-line, a `truecolor` with few styles, would still produce a number).
import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { CORPORA, generate } from "./term-corpus-gen.ts";

const text = (name: (typeof CORPORA)[number], mib = 1) => generate(name, mib).toString("latin1");

test("the same arguments give the same bytes, and the corpora differ from each other", () => {
	const seen = new Set<string>();
	for (const name of CORPORA) {
		const a = generate(name, 1);
		expect(a.equals(generate(name, 1))).toBe(true);
		seen.add(a.subarray(0, 4096).toString("hex"));
	}
	expect(seen.size).toBe(CORPORA.length);
});

test("each corpus is at least the requested size and overshoots by less than a frame", () => {
	for (const name of CORPORA) {
		const n = generate(name, 1).length;
		expect(n).toBeGreaterThanOrEqual(1 << 20);
		expect(n).toBeLessThan((1 << 20) + 64 * 1024);
	}
});

test("seq is the numbers 1..N, one per line, in order", () => {
	const lines = text("seq").split("\r\n");
	lines.pop(); // the empty tail after the last CRLF
	expect(lines.length).toBeGreaterThan(100_000);
	lines.forEach((l, i) => {
		if (l !== String(i + 1)) throw new Error(`line ${i} is ${JSON.stringify(l)}, want ${i + 1}`);
	});
});

test("ascii is 96-column printable lines with no escape sequence", () => {
	const lines = text("ascii").split("\r\n");
	lines.pop();
	expect(text("ascii")).not.toContain("\x1b");
	for (const l of lines) expect(l.length).toBe(96);
});

test("truecolor has many unique styles, repaint256 has none truecolor, and both repaint inside a synchronized update", () => {
	const tc = text("truecolor");
	const colors = new Set(tc.match(/\x1b\[38;2;\d+;\d+;\d+m/g));
	expect(colors.size).toBeGreaterThan(10_000);
	expect(tc).not.toContain("48;5;");
	const r256 = text("repaint256");
	expect(r256).not.toContain("38;2;");
	expect(new Set(r256.match(/\x1b\[38;5;\d+;48;5;\d+m/g)).size).toBeGreaterThan(10_000);
	for (const s of [tc, r256]) {
		expect(s.startsWith("\x1b[?2026h")).toBe(true);
		const begins = s.split("\x1b[?2026h").length - 1;
		const ends = s.split("\x1b[?2026l").length - 1;
		expect(begins).toBe(ends);
	}
});

test("sgr colours eight runs per line", () => {
	const line = text("sgr").split("\r\n")[10]!;
	expect(line.match(/\x1b\[3\d;(1|22)m/g)?.length).toBe(8);
});

test("the agent patterns start with a screenful of newlines and then carry the generator's own frames", () => {
	for (const name of ["ink-2026", "ratatui-diff", "omp-boxes"] as const) {
		const s = text(name);
		expect(s.startsWith("\n".repeat(40))).toBe(true);
		expect(s.slice(40)).toContain("\x1b");
	}
});

test("the CLI writes one file per corpus, only the ones asked for, and refuses an unknown name", () => {
	const out = mkdtempSync(join(tmpdir(), "corpus-"));
	const run = (...args: string[]) => Bun.spawnSync(["bun", join(import.meta.dir, "term-corpus-gen.ts"), "--out", out, "--mib", "1", ...args], { stderr: "pipe" });
	expect(run("--only", "seq,sgr").exitCode).toBe(0);
	expect(readdirSync(out).sort()).toEqual(["seq.bin", "sgr.bin"]);
	expect(readFileSync(join(out, "seq.bin")).equals(generate("seq", 1))).toBe(true);
	const bad = run("--only", "seqq");
	expect(bad.exitCode).not.toBe(0);
	expect(bad.stderr.toString()).toContain("unknown corpus seqq");
});
