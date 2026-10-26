// The verdict of `scripts/agents-load-bench.ts`, pure so it is unit-tested and `--judge <report.json>` needs no app.
//
// "The terminal freezes out of nowhere with 7+ agents" (#110) is three different failures, and each has its own number:
//  - the UI thread stalls (`stallMs`, the longest main-thread gap a MessageChannel ping-pong saw) or paints late
//    (`frameGapMs` from the page's rAF),
//  - a terminal's renderer stops presenting (`frames` of a tab did not advance while its program kept printing),
//  - memory climbs behind the stream (`rssMb` of the app, hostd and the WebContent process, growth over the run).

export interface Stat {
	p50: number;
	p95: number;
	max: number;
}

/** What one run measured. Every list is one value per sample (≈ once a second). */
export interface Load {
	/** Longest main-thread stall in each sampling window, ms. */
	stallMs: number[];
	/** rAF gaps of the page, ms (all windows, concatenated). */
	frameGapMs: number[];
	/** Per terminal: presented frames at the start and at the end of the measured window, and the pty's byte count. */
	terms: { tab: string; framesStart: number; framesEnd: number; bytes: number; shown?: boolean }[];
	/** Resident set per process kind, MB, one entry per sample. */
	rssMb: { app: number[]; hostd: number[]; web: number[] };
	/** `agent://status` events the page received in the window, and `host://event`s (title/activity/exit). */
	statusEvents: number;
	hostEvents: number;
	seconds: number;
}

export interface Budget {
	/** One terminal-heavy second must not stall the UI longer than this at p95 / ever. */
	stallP95Ms: number;
	stallMaxMs: number;
	/** The page keeps painting: p95 rAF gap (a dropped frame at 60 Hz is 33 ms). */
	frameGapP95Ms: number;
	/** A terminal whose program printed must have presented at least this many frames per second while visible. */
	minFps: number;
	/** The app, hostd and WebContent may grow this much over the run before it is a leak behind the stream. */
	rssGrowthMb: number;
	/** Events the page may receive per agent per second (status + host events): the storm limit. */
	eventsPerAgentPerSec: number;
}

/** Calibrated on the #110 user report: 7 agents, TUI redraw 30 Hz + streaming + hooks; see docs/terminal.md. */
export const BUDGET: Budget = {
	stallP95Ms: 16,
	stallMaxMs: 100,
	frameGapP95Ms: 34,
	minFps: 20,
	rssGrowthMb: 400,
	eventsPerAgentPerSec: 8,
};

const round = (n: number) => Math.round(n * 10) / 10;

/** Nearest-rank percentile (0 for an empty list: a bench that measured nothing must not pass). */
export function pct(xs: number[], p: number): number {
	if (xs.length === 0) return 0;
	const s = [...xs].sort((a, b) => a - b);
	return s[Math.min(s.length - 1, Math.max(0, Math.ceil(s.length * p) - 1))]!;
}

export function stat(xs: number[]): Stat {
	return { p50: round(pct(xs, 0.5)), p95: round(pct(xs, 0.95)), max: round(Math.max(0, ...xs)) };
}

/** Growth of a series: last minus first, 0 for fewer than two samples. */
export function growth(xs: number[]): number {
	return xs.length < 2 ? 0 : round(xs[xs.length - 1]! - xs[0]!);
}

export function summarize(l: Load) {
	return {
		seconds: l.seconds,
		agents: l.terms.length,
		stallMs: stat(l.stallMs),
		frameGapMs: stat(l.frameGapMs),
		fps: l.terms.map((t) => ({ tab: t.tab, fps: round((t.framesEnd - t.framesStart) / Math.max(1, l.seconds)), bytes: t.bytes, shown: t.shown })),
		rssGrowthMb: {
			app: growth(l.rssMb.app),
			hostd: growth(l.rssMb.hostd),
			web: growth(l.rssMb.web),
		},
		eventsPerAgentPerSec: round((l.statusEvents + l.hostEvents) / Math.max(1, l.seconds) / Math.max(1, l.terms.length)),
	};
}

/** Violations of `budget`, one readable line each; empty = within budget. A run that measured nothing is itself one. */
export function check(l: Load, budget: Budget = BUDGET): string[] {
	if (l.terms.length === 0) return ["no terminals were opened: nothing was measured"];
	if (l.stallMs.length === 0) return ["no main-thread samples were taken"];
	const s = summarize(l);
	const v: string[] = [];
	if (s.stallMs.p95 > budget.stallP95Ms) v.push(`main-thread stall p95 ${s.stallMs.p95} ms > ${budget.stallP95Ms} ms`);
	if (s.stallMs.max > budget.stallMaxMs) v.push(`main-thread stall max ${s.stallMs.max} ms > ${budget.stallMaxMs} ms`);
	if (s.frameGapMs.p95 > budget.frameGapP95Ms) v.push(`page frame gap p95 ${s.frameGapMs.p95} ms > ${budget.frameGapP95Ms} ms`);
	for (const t of s.fps) {
		// A program that printed nothing has no frames to owe; one that printed and shows < minFps is frozen or starved.
		if (t.shown !== false && t.bytes > 0 && t.fps < budget.minFps) v.push(`terminal ${t.tab} presented ${t.fps} fps < ${budget.minFps} fps while its program printed ${t.bytes} bytes`);
	}
	for (const k of ["app", "hostd", "web"] as const) {
		if (s.rssGrowthMb[k] > budget.rssGrowthMb) v.push(`${k} RSS grew ${s.rssGrowthMb[k]} MB > ${budget.rssGrowthMb} MB`);
	}
	if (s.eventsPerAgentPerSec > budget.eventsPerAgentPerSec)
		v.push(`${s.eventsPerAgentPerSec} page events per agent per second > ${budget.eventsPerAgentPerSec} (an event storm)`);
	return v;
}

const isNums = (v: unknown): v is number[] => Array.isArray(v) && v.every((n) => typeof n === "number");

/** `v[k]` of an object, `undefined` for anything else: a checked read, not a cast. */
const field = (v: unknown, k: string): unknown => (v && typeof v === "object" && k in v ? (v as Record<string, unknown>)[k] : undefined);

const isTerm = (t: unknown): t is Load["terms"][number] =>
	typeof field(t, "tab") === "string" && ["framesStart", "framesEnd", "bytes"].every((k) => typeof field(t, k) === "number");

/**
 * The `load` of a saved report, or a reason it is not one. A report is a file anyone can edit or truncate, and a
 * list that is not numbers would sort and compare as NaN and pass every budget.
 */
export function parseLoad(raw: unknown): { load: Load } | { error: string } {
	const l = field(raw, "load");
	if (l === undefined) return { error: "the report has no `load`" };
	for (const k of ["stallMs", "frameGapMs", "terms", "rssMb", "statusEvents", "hostEvents", "seconds"]) {
		if (field(l, k) === undefined) return { error: `\`load.${k}\` is missing` };
	}
	const [stallMs, frameGapMs, terms, rss] = ["stallMs", "frameGapMs", "terms", "rssMb"].map((k) => field(l, k));
	const [statusEvents, hostEvents, seconds] = ["statusEvents", "hostEvents", "seconds"].map((k) => field(l, k));
	if (!isNums(stallMs) || !isNums(frameGapMs)) return { error: "`stallMs` and `frameGapMs` must be lists of numbers" };
	if (typeof statusEvents !== "number" || typeof hostEvents !== "number" || typeof seconds !== "number")
		return { error: "`statusEvents`, `hostEvents` and `seconds` must be numbers" };
	if (!Array.isArray(terms) || !terms.every(isTerm)) return { error: "`terms` must be a list of {tab, framesStart, framesEnd, bytes}" };
	const [app, hostd, web] = ["app", "hostd", "web"].map((k) => field(rss, k));
	if (!isNums(app) || !isNums(hostd) || !isNums(web)) return { error: "`rssMb` needs numeric `app`, `hostd` and `web` lists" };
	return { load: { stallMs, frameGapMs, terms, rssMb: { app, hostd, web }, statusEvents, hostEvents, seconds } };
}
