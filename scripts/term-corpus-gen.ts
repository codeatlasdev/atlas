// bun scripts/term-corpus-gen.ts --out <dir> [--mib 16] [--only seq,ascii] [--cols 120] [--rows 40]
//
// The byte corpora of the terminal parser bench (`cargo bench -p atlas-term --bench vt_parse`, docs/perf/terminal-linux.md).
// Each is a file `<dir>/<name>.bin` of about `--mib` MiB, deterministic (a seeded PRNG: the same arguments give the same
// bytes, so two machines parse the same input). Shapes, and what each stresses:
//   seq          `1..N` one line each: the grid scrolls on every line (`seq 1 100000000`).
//   ascii        96-column prose lines: plain printing, scroll every line.
//   sgr          8 colour runs per line (`ls --color`, a compiler's output).
//   repaint256   a full-screen repaint: 256-colour fg + bg on every cell, CUP per row, inside 2026 (the zed#61819 shape).
//   truecolor    the same with a random 24-bit foreground per cell: many unique styles, the known slow path.
//   ink-2026, ratatui-diff, omp-boxes   the agent TUI patterns of `term-agent-gen.ts` (`frameBytes`, the generator's own
//                bytes, so the bench and the torn-frame oracle read the same producer).
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { frameBytes } from "./term-agent-gen.ts";

export const CORPORA = ["seq", "ascii", "sgr", "repaint256", "truecolor", "ink-2026", "ratatui-diff", "omp-boxes"] as const;
export type Corpus = (typeof CORPORA)[number];

/** mulberry32: a seeded 32-bit PRNG; the corpora must not depend on `Math.random`. */
export function rng(seed: number): () => number {
	let a = seed >>> 0;
	return () => {
		a = (a + 0x6d2b79f5) >>> 0;
		let t = a;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

const WORDS = "the quick brown fox jumps over a lazy dog while terminal output scrolls past grid cell parser state escape sequence line buffer render frame agent reads writes files and runs commands in parallel across worktrees".split(" ");

function prose(r: () => number, width: number): string {
	let s = "";
	for (;;) {
		const w = WORDS[Math.floor(r() * WORDS.length)]!;
		if (s.length + w.length + 1 > width) return s.padEnd(width, " ");
		s += (s ? " " : "") + w;
	}
}

/** Appends chunks produced by `next(i)` until at least `bytes` bytes. */
function fill(bytes: number, next: (i: number) => string): Buffer {
	const parts: string[] = [];
	let n = 0;
	for (let i = 0; n < bytes; i++) {
		const s = next(i);
		parts.push(s);
		n += Buffer.byteLength(s);
	}
	return Buffer.from(parts.join(""));
}

export function generate(name: Corpus, mib: number, cols = 120, rows = 40): Buffer {
	const bytes = mib << 20;
	const r = rng(0xa71a5 + name.length * 977 + name.charCodeAt(0));
	switch (name) {
		case "seq":
			return fill(bytes, (i) => `${i + 1}\r\n`);
		case "ascii":
			return fill(bytes, () => `${prose(r, 96)}\r\n`);
		case "sgr":
			return fill(bytes, () => {
				let line = "";
				for (let k = 0; k < 8; k++) line += `\x1b[${30 + Math.floor(r() * 8)};${r() < 0.3 ? 1 : 22}m${prose(r, 10 + Math.floor(r() * 4)).trimEnd()} `;
				return `${line}\x1b[0m\r\n`;
			});
		case "repaint256":
		case "truecolor": {
			const frame = () => {
				let s = "\x1b[?2026h";
				for (let row = 0; row < rows; row++) {
					s += `\x1b[${row + 1};1H`;
					for (let c = 0; c < cols; c++) {
						const ch = String.fromCharCode(33 + Math.floor(r() * 90));
						s += name === "repaint256" ? `\x1b[38;5;${Math.floor(r() * 256)};48;5;${Math.floor(r() * 256)}m${ch}` : `\x1b[38;2;${Math.floor(r() * 256)};${Math.floor(r() * 256)};${Math.floor(r() * 256)}m${ch}`;
					}
				}
				return `${s}\x1b[0m\x1b[?2026l`;
			};
			return fill(bytes, frame);
		}
		case "ink-2026":
		case "ratatui-diff":
		case "omp-boxes":
			// Ink erases upward from the bottom row: start with a screenful so CUU has somewhere to go.
			return Buffer.concat([Buffer.from("\n".repeat(rows)), fill(bytes, (i) => frameBytes(name, i, cols, rows))]);
	}
}

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};

if (import.meta.main) {
	const out = arg("out");
	if (!out) throw new Error("usage: term-corpus-gen.ts --out <dir> [--mib 16] [--only a,b] [--cols 120] [--rows 40]");
	const mib = Number(arg("mib", "16"));
	const only = arg("only")?.split(",");
	const unknown = only?.filter((o) => !(CORPORA as readonly string[]).includes(o));
	if (unknown?.length) throw new Error(`unknown corpus ${unknown.join(", ")}; one of ${CORPORA.join(", ")}`);
	mkdirSync(out, { recursive: true });
	for (const name of CORPORA) {
		if (only && !only.includes(name)) continue;
		const buf = generate(name, mib, Number(arg("cols", "120")), Number(arg("rows", "40")));
		writeFileSync(join(out, `${name}.bin`), buf);
		console.error(`${name}: ${(buf.length / (1 << 20)).toFixed(1)} MiB`);
	}
}
