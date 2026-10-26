// bun scripts/term-agent-gen.ts --pattern <id> [--seconds 10] [--hz 60] [--count-file <path>]
// bun scripts/term-agent-gen.ts --dump <dir>        (write each pattern's first frames as raw bytes, for the VT contract test)
//
// A synthetic agent TUI for `scripts/term-agent-bench.ts`: it prints what Claude Code, omp and codex print, with no model
// behind it, so the terminal's behaviour under each redraw style can be measured. Six patterns, each stating what it imitates:
//   ink-2026       Ink (Claude Code classic): erase the dynamic region with CUU + EL and rewrite it, wrapped in 2026.
//   ink-no-2026    the same without 2026: what a terminal that does not answer DECRQM 2026 gets; tears unless the
//                  terminal paces its draws to frames that arrived whole.
//   ratatui-diff   ratatui / codex: 2026 around a diff of changed cells only, every 8.3 ms.
//   omp-boxes      omp: a transcript of bordered boxes, rewritten line by line (CUP + EL), 2026, cursor hidden, autowrap off.
//   slow-2026      one frame of a 2026 update split by a 300 ms gap (a hook, a GC pause): the terminal's timeout decides
//                  whether the half frame is ever drawn. Ghostty waits 1 s; the 150 ms of vte tears it.
//   scroll-stream  50 MB-class streaming output (`cat`) with the user scrolling: the viewport must hold (#97).
//
// Torn-frame oracle: every row of a frame starts with a cell whose background is the frame's colour
// (`term-agent-judge.ts` FRAME_COLORS[id % 6]). One capture showing two colours down that column mixed two frames.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { FRAME_COLORS } from "./term-agent-judge.ts";

export interface PatternDef {
	id: string;
	/** What it imitates and why it is in the bench. */
	covers: string;
	/** Frames per second the producer aims at. */
	hz: number;
	/** Has the per-frame colour oracle (a full-viewport frame whose rows all carry the frame colour). */
	oracle: boolean;
}

export const PATTERNS: PatternDef[] = [
	{ id: "ink-2026", covers: "Ink (Claude Code classic): CUU + EL rewrite of the last 40 lines inside 2026", hz: 30, oracle: true },
	{ id: "ink-no-2026", covers: "the same redraw without 2026, as a terminal that fails DECRQM 2026 gets it", hz: 30, oracle: true },
	{ id: "ratatui-diff", covers: "ratatui / codex: a 2026 frame of changed cells only every 8.3 ms", hz: 120, oracle: true },
	{ id: "omp-boxes", covers: "omp: bordered transcript boxes rewritten with CUP + EL inside 2026, cursor hidden, autowrap off", hz: 30, oracle: true },
	{ id: "slow-2026", covers: "one 2026 frame split by a 300 ms gap: the safety timeout decides if the half frame shows", hz: 2, oracle: true },
	{ id: "scroll-stream", covers: "streaming output while the user scrolls up (#97), and a mouse program counting wheel reports", hz: 0, oracle: false },
];

const BSU = "\x1b[?2026h";
const ESU = "\x1b[?2026l";
/** Marker `frameBytes` leaves where the producer must pause; `run` turns it into the gap. Never written to a terminal. */
const GAP = "\x00GAP";
/** Pause of the slow frame, ms: over vte's 150 ms 2026 timeout, under Ghostty's 1000 ms. */
export const SLOW_GAP_MS = 300;

const color = (id: number) => FRAME_COLORS[id % FRAME_COLORS.length]!;
const sgrBg = (id: number) => `\x1b[48;2;${color(id).join(";")}m`;
const RESET = "\x1b[0m";
/** The first cell of a row: the frame's colour (the probe column), then the row's text. */
const tag = (id: number) => `${sgrBg(id)} ${RESET}`;
const text = (id: number, row: number, cols: number) => `f${id} r${row} ${"·".repeat(Math.max(0, cols - 12))}`.slice(0, Math.max(0, cols - 2));

/** The bytes of frame `n` for `pattern`, at `cols` x `rows`. Pure: the same frame is what the VT contract test parses. */
export function frameBytes(pattern: string, n: number, cols: number, rows: number): string {
	switch (pattern) {
		case "ink-2026":
		case "ink-no-2026": {
			// Ink: cursor up N, then for each line erase and rewrite; the dynamic region is the whole viewport here.
			const wrap = pattern === "ink-2026";
			const lines = Array.from({ length: rows }, (_, r) => `\x1b[2K\r${tag(n)} ${text(n, r, cols)}`).join("\n");
			return `${wrap ? BSU : ""}\x1b[${rows - 1}A\r${lines}${wrap ? ESU : ""}`;
		}
		case "ratatui-diff": {
			// Every row carries the frame colour, so a diff that moves the colour touches every row's first cell,
			// and a counter cell changes too.
			const cells = Array.from({ length: rows }, (_, r) => `\x1b[${r + 1};1H${tag(n)}\x1b[${r + 1};3H${text(n, r, cols)}`).join("");
			return `${BSU}${cells}${ESU}`;
		}
		case "omp-boxes": {
			const w = Math.max(10, cols - 2);
			const line = (r: number) => {
				const top = r % 6 === 0;
				const bot = r % 6 === 5;
				const body = top ? `╭${"─".repeat(w - 2)}╮` : bot ? `╰${"─".repeat(w - 2)}╯` : `│${text(n, r, w - 2).padEnd(w - 2)}│`;
				return `\x1b[${r + 1};1H\x1b[2K${tag(n)}${body}`;
			};
			return `${BSU}\x1b[?25l\x1b[?7l${Array.from({ length: rows }, (_, r) => line(r)).join("")}\x1b[?7h${ESU}`;
		}
		case "slow-2026": {
			const cells = (from: number, to: number) => Array.from({ length: to - from }, (_, i) => `\x1b[${from + i + 1};1H\x1b[2K${tag(n)} ${text(n, from + i, cols)}`).join("");
			const mid = Math.floor(rows / 2);
			return `${BSU}${cells(0, mid)}${GAP}${cells(mid, rows)}${ESU}`;
		}
		case "scroll-stream":
			return `${sgrBg(n)} ${RESET} stream line ${n} ${"x".repeat(60)}\r\n`;
		default:
			throw new Error(`unknown pattern ${pattern}`);
	}
}

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};

/** `--dump`: the first frames of each oracle pattern, one file per pattern, for the VT contract test (`cargo test -p atlas-term --test agent_frames`). */
function dump(dir: string) {
	mkdirSync(dir, { recursive: true });
	for (const p of PATTERNS.filter((x) => x.oracle)) {
		const frames = [0, 1].map((n) => frameBytes(p.id, n, 40, 12).replaceAll(GAP, ""));
		writeFileSync(join(dir, `${p.id}.0`), frames[0]!);
		writeFileSync(join(dir, `${p.id}.1`), frames[1]!);
	}
}

async function run(id: string, seconds: number, hz: number, countFile?: string) {
	const def = PATTERNS.find((p) => p.id === id);
	if (!def) throw new Error(`unknown pattern ${id}; one of ${PATTERNS.map((p) => p.id).join(", ")}`);
	const cols = process.stdout.columns || 120;
	const rows = process.stdout.rows || 40;
	let bytes = 0;
	let produced = 0;
	const out = (s: string) => {
		bytes += Buffer.byteLength(s);
		process.stdout.write(s);
	};
	const report = () => countFile && writeFileSync(countFile, JSON.stringify({ bytes, produced }));
	const alt = id !== "scroll-stream";
	if (alt) out("\x1b[?1049h\x1b[?25l\x1b[2J");
	const end = Date.now() + seconds * 1000;
	const every = def.hz > 0 ? 1000 / (hz || def.hz) : 0;
	let n = 0;
	const reporter = setInterval(report, 250);
	while (Date.now() < end) {
		const started = performance.now();
		if (id === "scroll-stream") {
			// Bursts of lines as fast as the pty takes them: `cat` of a big file.
			let burst = "";
			for (let i = 0; i < 200; i++) burst += frameBytes(id, n++, cols, rows);
			out(burst);
			produced += 200;
			await Bun.sleep(0);
			continue;
		}
		const parts = frameBytes(id, n, cols, rows).split(GAP);
		for (const [i, part] of parts.entries()) {
			out(part);
			if (i < parts.length - 1) await Bun.sleep(SLOW_GAP_MS);
		}
		produced++;
		n++;
		const wait = every - (performance.now() - started);
		if (wait > 0) await Bun.sleep(wait);
	}
	clearInterval(reporter);
	if (alt) out("\x1b[?25h\x1b[?1049l");
	report();
}

if (import.meta.main) {
	const d = arg("dump");
	if (d) dump(d);
	else await run(arg("pattern", "ink-2026")!, Number(arg("seconds", "10")), Number(arg("hz", "0")), arg("count-file"));
}
