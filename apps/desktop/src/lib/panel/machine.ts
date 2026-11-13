import type { HeavyStatus } from "./heavy.ts";

/**
 * The Machine view (#129): why the machine is slow and what each agent runs. These are the shapes the
 * `machine_report` command returns (snake_case, as serialized by Rust; pinned by
 * `__fixtures__/machine-report.json`), and the pure logic the view draws from.
 */

export interface MachineService {
	project: string;
	name: string;
	state: string;
	/** Bytes of the process tree. */
	memory_rss: number | null;
	memory_trend: "rising" | "flat" | "falling" | null;
	memory_trend_mb_min: number | null;
	budget_state: "ok" | "warn" | "over";
	memory_leak: string | null;
	/** Percent of one core (can exceed 100). */
	cpu: number | null;
}

export interface MachineTerminal {
	pty: string;
	pid: number;
	cwd: string;
	command: string;
	created_at: number;
	/** Bytes of the process tree, null when it could not be sampled. */
	mem: number | null;
	/** Percent of one core. */
	cpu: number | null;
}

export interface MachineRunHistory {
	label: string;
	/** Seconds per run, oldest first. */
	secs: number[];
	avg_s: number;
	last_s: number;
	max_s: number;
}

export interface MachineReport {
	/** Unix ms. */
	at: number;
	cores: number;
	load: { one: number; five: number; fifteen: number } | null;
	mem_available_pct: number | null;
	mem_total_bytes: number | null;
	gates: { max_load: number | null; min_mem_pct: number | null; holding: "load" | "memory" | null };
	heavy: HeavyStatus;
	history: MachineRunHistory[];
	services: MachineService[];
	terminals: MachineTerminal[];
}

const MB = 1 << 20;
const GB = 1 << 30;

/** `40 MB`, `900 MB`, `1.3 GB`, `24 GB`. */
export function bytes(n: number): string {
	if (n < GB) return `${Math.round(n / MB)} MB`;
	const g = n / GB;
	return g < 10 ? `${g.toFixed(1)} GB` : `${Math.round(g)} GB`;
}

export interface Gauge {
	/** 0..1 of the bar, null when the OS gave no reading. */
	value: number | null;
	/** 0..1 position of the heavy gate's line, null when that gate is off. */
	limit: number | null;
	/** Past the gate: a heavy job waits. */
	over: boolean;
	label: string;
}

const clamp = (n: number) => Math.min(1, Math.max(0, n));
const NO_READING = "no reading";

export const gauge = {
	/** The 5-minute load against the cores; the gate (`max_load`) is where a heavy job starts waiting. */
	load(r: MachineReport): Gauge {
		if (!r.load) return { value: null, limit: null, over: false, label: NO_READING };
		const { five } = r.load;
		const max = r.gates.max_load;
		const cores = Math.max(r.cores, 0);
		return {
			value: cores ? clamp(five / cores) : 1,
			limit: max != null && cores ? clamp(max / cores) : null,
			over: max != null && five >= max,
			label: `${five.toFixed(1)} of ${r.cores} cores`,
		};
	},
	/** Memory in use, with the free floor (`min_mem_pct`) drawn as the line a heavy job waits at. */
	memory(r: MachineReport): Gauge {
		const free = r.mem_available_pct;
		if (free == null) return { value: null, limit: null, over: false, label: NO_READING };
		const floor = r.gates.min_mem_pct;
		const total = r.mem_total_bytes != null ? ` · ${bytes(r.mem_total_bytes)}` : "";
		return {
			value: clamp((100 - free) / 100),
			limit: floor != null ? clamp((100 - floor) / 100) : null,
			over: floor != null && free < floor,
			label: `${free}% free${total}`,
		};
	},
};

export interface Note {
	tone: "warn" | "error";
	text: string;
}

/**
 * The line a service row adds, or null: a leak suspicion first (the actionable one), then a budget overrun
 * (with the climb when there is one), then a climbing trend. A flat service in budget stays quiet.
 */
export function serviceNote(s: MachineService): Note | null {
	const rate =
		s.memory_trend === "rising" && s.memory_trend_mb_min != null
			? `+${Math.round(s.memory_trend_mb_min)} MB/min`
			: null;
	if (s.memory_leak) return { tone: "warn", text: s.memory_leak };
	if (s.budget_state === "over")
		return { tone: "error", text: rate ? `Over max_memory · rising ${rate}` : "Over max_memory" };
	if (s.budget_state === "warn")
		return { tone: "warn", text: rate ? `Over memory_warn · rising ${rate}` : "Over memory_warn" };
	return rate ? { tone: "warn", text: `Rising ${rate}` } : null;
}

/** What the app knows about a terminal: its tab, and the agent status the hooks reported for that tab. */
interface TabLike {
	id: string;
	pty?: string;
	agent?: string;
	label: string;
}
interface StatusLike {
	state: string;
	agent: string;
	prompt?: string | null;
	model?: string | null;
}

export interface AgentRow {
	pty: string;
	/** Tab label for a known terminal, else the command line. */
	title: string;
	/** The agent id (`omp`, `claude`), null for a plain shell. */
	agent: string | null;
	/** `working` | `idle` | …, null without an agent. */
	state: string | null;
	/** What the agent is doing (its prompt), else the command and folder. */
	detail: string;
	cwd: string;
	mem: number | null;
	cpu: number | null;
}

/**
 * The terminals of the report joined to the app's tabs and agent statuses. Agents first, then plain shells
 * (a shell may run a build, so it stays listed); inside each group the heaviest first, a terminal that could
 * not be sampled last. The Rust side cannot know tabs or agents, so the join is here.
 */
export function agentRows(
	terminals: MachineTerminal[],
	tabs: TabLike[],
	status: Record<string, StatusLike>,
): AgentRow[] {
	const byPty = new Map(tabs.filter((t) => t.pty).map((t) => [t.pty!, t]));
	const rows = terminals.map((t): AgentRow => {
		const tab = byPty.get(t.pty);
		const st = tab ? status[tab.id] : undefined;
		const agent = tab?.agent ?? null;
		return {
			pty: t.pty,
			title: tab?.label ?? t.command,
			agent,
			state: agent ? (st?.state ?? "idle") : null,
			detail: st?.prompt?.trim() || `${t.command} · ${t.cwd}`,
			cwd: t.cwd,
			mem: t.mem,
			cpu: t.cpu,
		};
	});
	const rank = (r: AgentRow) => [r.agent ? 0 : 1, -(r.mem ?? -1)];
	return rows.sort((a, b) => {
		const [ga, ma] = rank(a);
		const [gb, mb] = rank(b);
		return ga - gb || ma - mb;
	});
}

export interface RunTrend {
	kind: "slower" | "faster" | "steady";
	/** Whole percent against the average of the earlier runs. */
	pct: number;
}

/** A change within this fraction of the earlier average is noise (the machine's load varies run to run). */
const STEADY = 0.1;

/**
 * The last run of a label against the average of the ones before it, so a regression in tooling speed
 * shows. Null when there is nothing earlier to compare with (or the earlier runs took no time).
 */
export function runTrend(secs: number[]): RunTrend | null {
	if (secs.length < 2) return null;
	const earlier = secs.slice(0, -1);
	const avg = earlier.reduce((a, b) => a + b, 0) / earlier.length;
	if (avg <= 0) return null;
	const delta = (secs[secs.length - 1] - avg) / avg;
	const pct = Math.round(Math.abs(delta) * 100);
	return { kind: Math.abs(delta) <= STEADY ? "steady" : delta > 0 ? "slower" : "faster", pct };
}

/**
 * `hygiene_report` (#127, `atlas doctor --hygiene`) as the Machine view needs it. That command is its own
 * feature; until it exists in the app the section reads "not available" (`hygieneView(null)`).
 */
export interface HygieneReport {
	orphans: { pid: number; name: string; kind: string | null; project: string | null }[];
	/** Tagged processes of live sessions. */
	live: number;
	projects: {
		root: string;
		name: string;
		target: number;
		node_modules: number;
		logs: number;
		worktrees: [string, number][];
	}[];
	freeable: { dirs_removed: number; entries_removed: number; bytes: number };
}

export type HygieneView =
	| { available: false }
	| {
			available: true;
			orphans: number;
			live: number;
			/** Disk per project: `target/`, `node_modules`, logs and worktrees. */
			disk: { name: string; bytes: number }[];
			/** What a fix would free. */
			freeable: number;
	  };

export function hygieneView(r: HygieneReport | null): HygieneView {
	if (!r) return { available: false };
	return {
		available: true,
		orphans: r.orphans.length,
		live: r.live,
		disk: r.projects.map((p) => ({
			name: p.name,
			bytes: p.target + p.node_modules + p.logs + p.worktrees.reduce((a, [, b]) => a + b, 0),
		})),
		freeable: r.freeable.bytes,
	};
}
