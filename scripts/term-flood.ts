// bun scripts/term-flood.ts [--sync 1|0] [--hz 60] [--seconds 20] [--chunk 97] [--slow 0]
//
// An agent-TUI redraw flood for flicker measurements in a real Atlas terminal (see docs/terminal.md "Flicker").
// Every frame erases and rewrites a block of boxes (cursor moves, line clears, box drawing: what omp does), written
// in small chunks so a terminal that draws per read shows half-updated states. A frame is one 2026 update when
// --sync 1. --slow N holds the update open N ms between its halves (a slow agent frame: the sync timeout).
//
// Oracle: every frame paints the same frame number in every row of the block, so a screenshot that shows two
// different numbers inside the block is a half-drawn frame. The number is also printed to --log (frames written).
import { writeFileSync } from "node:fs";

const arg = (k: string, d: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1]! : d;
};
const sync = arg("sync", "1") === "1";
const hz = Number(arg("hz", "60"));
const seconds = Number(arg("seconds", "20"));
const chunk = Number(arg("chunk", "97"));
const slow = Number(arg("slow", "0"));
const log = arg("log", "");
const cols = process.stdout.columns || 120;
const rows = Math.min((process.stdout.rows || 40) - 2, 30);

let n = 0;
const write = (s: string) => process.stdout.write(s);
/** Writes `s` in `chunk`-byte pieces with a tick between them, so the pty delivers it as many reads. */
async function dribble(s: string) {
	for (let i = 0; i < s.length; i += chunk) {
		write(s.slice(i, i + chunk));
		// a repaint takes a few ms, as an Ink frame does under load: long enough for a draw to land inside it
		if ((i / chunk) % 4 === 3) await Bun.sleep(1);
	}
}

function frame(): [string, string] {
	n++;
	const id = String(n).padStart(6, "0");
	let erase = sync ? "\x1b[?2026h" : "";
	// erase-then-draw, the way a producer without 2026 repaints
	for (let r = 0; r < rows; r++) erase += `\x1b[${r + 1};1H\x1b[2K`;
	let draw = "";
	for (let r = 0; r < rows; r++) {
		const w = Math.max(10, cols - 4);
		const edge =
			r === 0
				? `┌${"─".repeat(w)}┐`
				: r === rows - 1
					? `└${"─".repeat(w)}┘`
					: `│${` frame ${id} row ${String(r).padStart(2)} `.padEnd(w)}│`;
		draw += `\x1b[${r + 1};1H\x1b[${31 + (n % 6)}m${edge}\x1b[0m`;
	}
	draw += sync ? "\x1b[?2026l" : "";
	return [erase, draw];
}

write("\x1b[?25l\x1b[2J");
const end = Date.now() + seconds * 1000;
const period = 1000 / hz;
while (Date.now() < end) {
	const t0 = Date.now();
	const [erase, draw] = frame();
	await dribble(erase);
	if (slow) await Bun.sleep(slow);
	await dribble(draw);
	const left = period - (Date.now() - t0);
	if (left > 0) await Bun.sleep(left);
}
write("\x1b[?25h\x1b[2J\x1b[H");
if (log) writeFileSync(log, String(n));
