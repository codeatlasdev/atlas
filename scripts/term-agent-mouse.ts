// bun scripts/term-agent-mouse.ts --count-file <path> [--seconds 60]
//
// The "mouse-reporting program" of `scripts/term-agent-bench.ts` (what Claude Code's fullscreen renderer or vim is to the
// wheel): it turns on DECSET 1000 + 1006 (+ 1016 with --pixels), counts every wheel report the terminal sends and checks
// they arrive in the order they were sent. The bench sends notches with a rising column in the report (the pointer
// column is the notch's sequence number), so a dropped, duplicated or reordered report shows in the count and in `inOrder`.
// The count file is rewritten every 100 ms: `{"seen":N,"inOrder":bool,"last":[col,row]}`.
import { writeFileSync } from "node:fs";

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const file = arg("count-file");
const seconds = Number(arg("seconds", "60"));
const pixels = process.argv.includes("--pixels");

let seen = 0;
let inOrder = true;
let last: [number, number] = [0, 0];
let prevCol = -1;
let buf = "";

/** SGR report: `ESC [ < b ; x ; y M|m`. Wheel is button 64/65. */
const SGR = /\x1b\[<(\d+);(-?\d+);(-?\d+)([Mm])/g;

process.stdin.setRawMode?.(true);
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk: string) => {
	buf += chunk;
	let end = 0;
	for (const m of buf.matchAll(SGR)) {
		end = (m.index ?? 0) + m[0].length;
		const button = Number(m[1]) & ~(4 | 8 | 16);
		if (button !== 64 && button !== 65) continue;
		seen++;
		const col = Number(m[2]);
		// The bench's notches carry a rising column (1-based in cells, 0-based in pixels): a smaller one than before is a reorder.
		if (col < prevCol) inOrder = false;
		prevCol = col;
		last = [col, Number(m[3])];
	}
	buf = buf.slice(end).slice(-64);
});

process.stdout.write(`\x1b[?1049h\x1b[?1000h\x1b[?1006h${pixels ? "\x1b[?1016h" : ""}\x1b[2J\x1b[Hmouse counter ready\r\n`);
const timer = setInterval(() => file && writeFileSync(file, JSON.stringify({ seen, inOrder, last })), 100);
setTimeout(() => {
	clearInterval(timer);
	process.stdout.write(`\x1b[?1016l\x1b[?1006l\x1b[?1000l\x1b[?1049l`);
	if (file) writeFileSync(file, JSON.stringify({ seen, inOrder, last }));
	process.exit(0);
}, seconds * 1000);
