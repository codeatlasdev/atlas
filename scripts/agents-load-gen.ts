// bun scripts/agents-load-gen.ts [--hz 30] [--lines 40] [--hook-every 150] [--title-hz 10] [--seconds 0]
//
// A synthetic coding agent for `scripts/agents-load-bench.ts`: what omp does to a terminal (#110), with no model behind it.
// Run inside an Atlas terminal; it reads the same env a real agent gets from hostd (`ATLAS_PANE_KEY`, `ATLAS_HOOK_ENDPOINT`).
//  - a full-screen TUI redraw at --hz (alternate screen, cursor home, every row rewritten with SGR colors),
//  - streaming lines under it, --lines per second, as a tool's output scrolls,
//  - a spinner in the OSC 0 window title at --title-hz, mostly repeating the previous text, like omp's braille frames,
//  - a hook post every --hook-every ms to `/hook/omp`, the events omp's extension posts (tool_execution_start/end...).
// Exits after --seconds (0 = until killed) and leaves the alternate screen.
import { readFileSync, writeFileSync } from "node:fs";

const arg = (k: string, d: number) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? Number(process.argv[i + 1]) : d;
};
const hz = arg("hz", 30);
const linesPerSec = arg("lines", 40);
const hookEvery = arg("hook-every", 150);
const titleHz = arg("title-hz", 10);
const seconds = arg("seconds", 0);

const cols = process.stdout.columns || 120;
const rows = process.stdout.rows || 40;
let bytes = 0;
const out = (s: string) => {
	bytes += s.length;
	process.stdout.write(s);
};
/** Where the bench reads how much this agent printed: `<dir>/<pane key>.bytes` (a bench-owned dir, never the repo). */
const countFile = process.env.LOADGEN_COUNT_DIR ? `${process.env.LOADGEN_COUNT_DIR}/${process.env.ATLAS_PANE_KEY ?? process.pid}.bytes` : undefined;
const SPIN = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";

function endpoint(): { port: string; token: string } | undefined {
	try {
		const env: Record<string, string> = {};
		for (const l of readFileSync(process.env.ATLAS_HOOK_ENDPOINT!, "utf8").split("\n")) {
			const i = l.indexOf("=");
			if (i > 0) env[l.slice(0, i)] = l.slice(i + 1).trim();
		}
		return env.ATLAS_HOOK_PORT && env.ATLAS_HOOK_TOKEN ? { port: env.ATLAS_HOOK_PORT, token: env.ATLAS_HOOK_TOKEN } : undefined;
	} catch {
		return undefined;
	}
}

/** One hook, shaped like omp's extension (`setup/atlas_status.ts`): the pane key routes it to this tab's agent. */
function hook(event: string, extra: Record<string, unknown> = {}) {
	const ep = process.env.ATLAS_HOOK_ENDPOINT ? endpoint() : undefined;
	if (!ep) return;
	const body = {
		pane_key: process.env.ATLAS_PANE_KEY,
		tab_id: process.env.ATLAS_TAB_ID,
		worktree_id: process.env.ATLAS_WORKTREE_ID,
		agent: "omp",
		payload: { event, model: "synthetic", session_file: `/tmp/loadgen-${process.env.ATLAS_PANE_KEY}.jsonl`, ...extra },
	};
	fetch(`http://127.0.0.1:${ep.port}/hook/omp`, {
		method: "POST",
		headers: { "content-type": "application/json", "x-atlas-hook-token": ep.token },
		body: JSON.stringify(body),
		signal: AbortSignal.timeout(1000),
	}).catch(() => {});
}

let frame = 0;
/** The TUI: every row of the screen rewritten with a few colors, as a Ink/bubbletea redraw does. */
function redraw() {
	frame++;
	let s = "\x1b[?2026h\x1b[H";
	for (let r = 0; r < rows - 1; r++) {
		const c = 31 + ((r + frame) % 6);
		s += `\x1b[${c}m${String(frame).padStart(6)} \x1b[0m\x1b[1mrow ${String(r).padStart(2)}\x1b[0m `;
		s += "x".repeat(Math.max(0, cols - 20)) + "\x1b[K\r\n";
	}
	s += `\x1b[7m ${SPIN[frame % SPIN.length]} working \x1b[0m\x1b[K\x1b[?2026l`;
	out(s);
}

let line = 0;
/** Streaming output: a tool's lines scrolling into the region above the TUI. */
function stream() {
	out(`\x1b[${rows};1H\n\x1b[2m[${line++}] ${"stream ".repeat(10)}\x1b[0m`);
}

let tick = 0;
/** omp redraws its title on every spinner frame, usually with the text it already had. */
function title() {
	out(`\x1b]0;π : task ${SPIN[Math.floor(tick++ / 3) % SPIN.length]}\x07`);
}

out("\x1b[?1049h\x1b[?25l");
hook("agent_start", { prompt: "synthetic load" });
const report = () => countFile && writeFileSync(countFile, String(bytes));
const timers = [
	setInterval(redraw, 1000 / hz),
	setInterval(stream, 1000 / Math.max(1, linesPerSec)),
	setInterval(title, 1000 / Math.max(1, titleHz)),
	setInterval(() => hook(tick % 2 ? "tool_execution_start" : "tool_execution_end", { tool_name: "bash" }), hookEvery),
	setInterval(report, 1000),
];

function stop() {
	timers.forEach(clearInterval);
	hook("agent_end", { will_continue: false });
	out("\x1b[?25h\x1b[?1049l");
	report();
	process.exit(0);
}
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
if (seconds > 0) setTimeout(stop, seconds * 1000);
