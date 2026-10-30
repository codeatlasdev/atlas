/**
 * The menu-bar popover (`/tray`) is a second webview with no workspace of its own: the main
 * window builds a `TraySnapshot` from its live stores and pushes it through Rust
 * (`tray_publish`), the popover renders it and sends clicks back as `TrayAction`s. Everything
 * decidable without the DOM lives here, so it is testable.
 */
import { summarize, type Tone } from "#lib/panel/metrics.ts";

interface TrayService {
	name: string;
	state: string;
}

/** `image.src` is a data URL (the popover is another webview and cannot read the main window's blobs). */
export type TrayIcon = { kind: "emoji" | "monogram"; text: string } | { kind: "image"; src: string };

export interface TrayProject {
	id: string;
	name: string;
	/** Hue (0–359) of the project's space accent. */
	hue: number;
	/** Identity tile for the popover (no hostd there): emoji, image URL, or monogram text. */
	icon: TrayIcon;
	running: boolean;
	branch: string | null;
	/** Id of the main checkout when this is a linked worktree. */
	parent: string | null;
	/** Checkout folder, to tell apart rows that share a name and a branch. */
	path: string;
	pinned: boolean;
	/** When the project was last opened or its agents last moved (ms epoch, 0 = never seen): recents first. */
	recentAt: number;
	services: TrayService[];
	/** Process-tree CPU (% of a core) and memory (MB), summed over the services. */
	cpu: number;
	mem: number;
	/** Recent total CPU, oldest first, for the sparkline. */
	cpuSeries: number[];
	/** First failed service and when the main window saw it fail (ms epoch). */
	failed: { service: string; at: number } | null;
}

/** One agent terminal, as much of `DockAgent` as a popover row shows. */
export interface TrayAgent {
	tabId: string;
	agent: string;
	project: string;
	worktree: string;
	state: "idle" | "working" | "blocked" | "waiting" | "done";
	lastMessage: string | null;
	updatedAt: number;
}

export interface TraySnapshot {
	/** Epoch ms the main window built it; the popover shows ages relative to now. */
	at: number;
	projects: TrayProject[];
	agents: { working: number; needsYou: number; focusTab: string | null; list: TrayAgent[] };
	terminals: { live: number };
	/** `outdated`: the terminal host needs a restart that the user has not agreed to yet. */
	host: {
		status: "current" | "upgrading" | "upgraded" | "outdated" | "restarting" | null;
		version: string | null;
	};
	/** Newer release than the installed app/CLI, if any. */
	update: { version: string } | null;
	/** Dev build (cargo tauri dev or `atlas link`): updates are paused. */
	dev?: boolean;
}

export const EMPTY_SNAPSHOT: TraySnapshot = {
	at: 0,
	projects: [],
	agents: { working: 0, needsYou: 0, focusTab: null, list: [] },
	terminals: { live: 0 },
	host: { status: null, version: null },
	update: null,
};

/** What a click in the popover asks the main window to do (`tray://action`). */
export type TrayAction =
	| { kind: "open" }
	| { kind: "settings" }
	| { kind: "project"; id: string }
	| { kind: "logs"; id: string; service: string }
	| { kind: "agents"; tab: string | null }
	| { kind: "terminal" }
	/** Start an agent in the selected project (the main window's own "new agent"). */
	| { kind: "new-agent" }
	| { kind: "update" }
	/** Open the "Restart the terminal host?" prompt. */
	| { kind: "host" };

export interface Overall {
	tone: Tone;
	label: string;
}

/** Header line: worst state wins (failed > starting > healthy), else nothing is running. */
export function overall(projects: TrayProject[]): Overall {
	const running = projects.filter((p) => p.running);
	if (!running.length) return { tone: "off", label: "Nothing running" };
	let failed = 0;
	let starting = 0;
	for (const p of running) {
		for (const s of p.services) {
			if (s.state === "failed") failed++;
			else if (s.state === "starting") starting++;
		}
	}
	if (failed) return { tone: "fail", label: failed === 1 ? "1 service failed" : `${failed} services failed` };
	if (starting)
		return { tone: "warn", label: starting === 1 ? "1 service starting" : `${starting} services starting` };
	return running.some((p) => projectTone(p) !== "ok")
		? { tone: "warn", label: "Some services are not up" }
		: { tone: "ok", label: running.length === 1 ? "Stack healthy" : "All stacks healthy" };
}

/** Per-project tone, same rule as the panel's health pill. */
export function projectTone(p: TrayProject): Tone {
	return summarize(
		p.services.map((s) => s.state),
		p.running,
	).tone;
}

const RANK: Record<Tone, number> = { fail: 0, warn: 1, ok: 2, off: 3 };

/** Cards to show: running projects only, failed first, then starting, healthy; ties by name. */
export function runningCards(projects: TrayProject[]): TrayProject[] {
	return projects
		.filter((p) => p.running)
		.sort((a, b) => RANK[projectTone(a)] - RANK[projectTone(b)] || a.name.localeCompare(b.name));
}

export interface Recommendation {
	id: string;
	icon: "fail" | "update" | "host";
	title: string;
	detail: string;
	cta: string;
	action: TrayAction;
}

const FRESH_FAILURE_MS = 60 * 60_000;

/** "just now", "2 min ago", "3 h ago". */
export function ago(ms: number): string {
	const s = Math.max(0, Math.round(ms / 1000));
	if (s < 45) return "just now";
	const m = Math.round(s / 60);
	if (m < 60) return `${m} min ago`;
	return `${Math.round(m / 60)} h ago`;
}

/**
 * The one thing worth your attention, CleanMyMac's "Today's recommendation":
 * a failed service beats an update, which beats an outdated terminal host.
 */
export function recommend(s: TraySnapshot, now: number): Recommendation | null {
	const failing = runningCards(s.projects)
		.filter((p) => p.failed && now - p.failed.at < FRESH_FAILURE_MS)
		.sort((a, b) => b.failed!.at - a.failed!.at)[0];
	if (failing?.failed) {
		const { service, at } = failing.failed;
		return {
			id: `fail:${failing.id}:${service}`,
			icon: "fail",
			title: `${service} failed ${ago(now - at)}`,
			detail: failing.name,
			cta: "View logs",
			action: { kind: "logs", id: failing.id, service },
		};
	}
	if (s.update) {
		return {
			id: `update:${s.update.version}`,
			icon: "update",
			title: `Atlas ${s.update.version} is available`,
			detail: "Update the CLI and the app together",
			cta: "Update",
			action: { kind: "update" },
		};
	}
	if (s.host.status === "outdated") {
		return {
			id: "host",
			icon: "host",
			title: "Terminal host needs a restart",
			detail: "Language features are off until it restarts",
			cta: "Review",
			action: { kind: "host" },
		};
	}
	return null;
}

/** `3/4 up`-style services line. */
export function upLine(p: TrayProject): string {
	const up = p.services.filter((s) => s.state === "running").length;
	return `${up}/${p.services.length} up`;
}

/** Dot tone of one service in a project card. */
export function serviceTone(state: string): Tone {
	return state === "running" ? "ok" : state === "failed" ? "fail" : state === "starting" ? "warn" : "off";
}

/** Window height for the content: the popover grows with its cards up to a cap. */
export const TRAY_MAX_HEIGHT = 560;
export const TRAY_MIN_HEIGHT = 220;

/**
 * Height of the window for `content` px of page: grown to fit, between the min and the max, and never taller than the
 * `room` the screen leaves under the menu bar (a short display must not push the popover off its bottom edge).
 */
export function trayHeight(content: number, room = Infinity): number {
	const fit = Math.min(TRAY_MAX_HEIGHT, Math.max(TRAY_MIN_HEIGHT, Math.ceil(content)));
	return Math.min(fit, Math.max(0, Math.floor(room)));
}

/** Failed services of a RUNNING stack, in service order (a stopped project's last states are history, not news). */
export function failedServices(p: TrayProject): string[] {
	return p.running ? p.services.filter((s) => s.state === "failed").map((s) => s.name) : [];
}

/** The `service_cmd` invocations that restart exactly the failed services of `p` (never the whole stack). */
export function restartFailedCalls(
	p: TrayProject,
): { id: string; cmd: "restart_service"; service: string }[] {
	return failedServices(p).map((service) => ({ id: p.id, cmd: "restart_service", service }));
}

export type CardState = "healthy" | "starting" | "failed" | "idle";

/** The card's one-line state: what is wrong, by name, or that nothing is. Failed beats starting beats healthy. */
export function projectHeadline(p: TrayProject): { state: CardState; text: string } {
	if (!p.running) return { state: "idle", text: "Stopped" };
	if (!p.services.length) return { state: "healthy", text: "No services" };
	const failed = failedServices(p);
	if (failed.length) {
		const shown = failed.slice(0, 2).join(", ");
		return { state: "failed", text: `${shown}${failed.length > 2 ? ` +${failed.length - 2}` : ""} failed` };
	}
	const starting = p.services.filter((s) => s.state === "starting").length;
	if (starting) return { state: "starting", text: `${starting} starting` };
	const up = p.services.filter((s) => s.state === "running").length;
	if (up === p.services.length) return { state: "healthy", text: `All ${up} services up` };
	return { state: "starting", text: `${up}/${p.services.length} up` };
}

/**
 * SVG path data for a sparkline in a `w`×`h` box with `pad` px kept free at top and bottom (so the stroke is never
 * clipped). Newest sample on the right; the y axis runs from 0 to the series peak, so a flat zero series is a baseline.
 * `area` closes the line down to the bottom edge for the soft fill.
 */
export function sparkPaths(
	data: number[],
	w: number,
	h: number,
	pad: number,
): { line: string; area: string } {
	if (!data.length) return { line: "", area: "" };
	const pts = data.length === 1 ? [data[0]!, data[0]!] : data;
	const peak = Math.max(...pts, 0);
	const top = pad;
	const bottom = h - pad;
	const pt = (v: number, i: number) => {
		const x = (i / (pts.length - 1)) * w;
		const y = peak > 0 ? bottom - (v / peak) * (bottom - top) : bottom;
		return `${+x.toFixed(2)} ${+y.toFixed(2)}`;
	};
	const line = pts.map((v, i) => `${i ? "L" : "M"}${pt(v, i)}`).join(" ");
	return { line, area: `${line} L${w} ${h} L0 ${h} Z` };
}

/** Next focused index (`-1` = none) in a list of `count` items for an arrow/Home/End key; unrelated keys keep it. */
export function moveFocus(count: number, current: number, key: string): number {
	if (count <= 0) return -1;
	if (key === "ArrowDown") return Math.min(count - 1, current + 1);
	if (key === "ArrowUp") return Math.max(0, current - 1);
	if (key === "Home") return 0;
	if (key === "End") return count - 1;
	return current;
}

/** Rank of an agent that needs you: a blocked one (approval) before a plain wait. */
const NEED_RANK = { blocked: 0, waiting: 1 } as const;

/**
 * The agents worth a row: those waiting on you (blocked first, longest wait first, since that one has been
 * stuck longest) and those working (newest first). Done and idle agents are not news.
 */
export function agentSections(list: readonly TrayAgent[]): { needsYou: TrayAgent[]; working: TrayAgent[] } {
	const needsYou = list
		.filter((a) => a.state === "blocked" || a.state === "waiting")
		.sort(
			(a, b) =>
				NEED_RANK[a.state as keyof typeof NEED_RANK] - NEED_RANK[b.state as keyof typeof NEED_RANK] ||
				a.updatedAt - b.updatedAt,
		);
	const working = list.filter((a) => a.state === "working").sort((a, b) => b.updatedAt - a.updatedAt);
	return { needsYou, working };
}

/** `api`, or `api · feat-login` when the agent runs in a worktree that is not the project's own checkout. */
export function agentWhere(a: Pick<TrayAgent, "project" | "worktree">): string {
	const own = !a.worktree || a.worktree === "main" || a.worktree === a.project;
	if (!a.project) return a.worktree;
	return own ? a.project : `${a.project} · ${a.worktree}`;
}

/** What the agent is doing in one line: its last message's first line, else the state in words. */
export function agentDoing(a: Pick<TrayAgent, "state" | "lastMessage">): string {
	const first = a.lastMessage
		?.split("\n")
		.map((l) => l.trim())
		.find(Boolean);
	if (first) return first;
	return a.state === "blocked" ? "Needs approval" : a.state === "waiting" ? "Waiting for you" : "Working";
}

/** A project row of the popover: the project plus the name that tells it apart from its siblings. */
export interface TrayRow extends TrayProject {
	label: string;
}

export interface ProjectGroup {
	repo: TrayProject;
	/** The repo's name, with its folder appended when another repo has the same name (clones, forks). */
	repoLabel: string;
	worktrees: TrayRow[];
	running: boolean;
	/** False once the repo itself is listed elsewhere (as a running stack) and only its worktrees remain here. */
	repoShown?: boolean;
}

/** Last two path segments (`one/atlas`): enough to tell two checkouts apart without printing the home folder. */
function pathTail(path: string, n = 2): string {
	return path.split("/").filter(Boolean).slice(-n).join("/");
}

/**
 * Worktrees of one repo with the label that tells them apart. A branch is the name people know; without one the
 * folder name stands in; the id is the last resort. Two rows that still collide (same branch, different folders)
 * get the folder appended, so the list never shows N identical rows.
 */
function labelled(worktrees: TrayProject[]): TrayRow[] {
	const base = worktrees.map((w) => ({ w, label: w.branch || pathTail(w.path, 1) || w.id }));
	const count = new Map<string, number>();
	for (const { label } of base) count.set(label, (count.get(label) ?? 0) + 1);
	return base.map(({ w, label }) => ({
		...w,
		label: (count.get(label) ?? 0) > 1 && w.path ? `${label} · ${pathTail(w.path)}` : label,
	}));
}

const live = (p: TrayProject) => p.running;
const byRecent = (a: TrayProject, b: TrayProject) => b.recentAt - a.recentAt;

/**
 * Repos with their worktrees nested. A worktree whose repo is not in the list (removed, or filtered out) stands as
 * its own group rather than vanishing. Order: pinned, running, most recent, name; inside a group: running, recent, label.
 */
export function groupProjects(projects: readonly TrayProject[]): ProjectGroup[] {
	const ids = new Set(projects.map((p) => p.id));
	const children = new Map<string, TrayProject[]>();
	const repos: TrayProject[] = [];
	for (const p of projects) {
		if (p.parent && ids.has(p.parent)) children.set(p.parent, [...(children.get(p.parent) ?? []), p]);
		else repos.push(p);
	}
	const names = new Map<string, number>();
	for (const r of repos) names.set(r.name, (names.get(r.name) ?? 0) + 1);
	return repos
		.map((repo): ProjectGroup => {
			const worktrees = labelled(children.get(repo.id) ?? []).sort(
				(a, b) => Number(live(b)) - Number(live(a)) || byRecent(a, b) || a.label.localeCompare(b.label),
			);
			const twin = (names.get(repo.name) ?? 0) > 1 && repo.path;
			return {
				repo,
				repoLabel: twin ? `${repo.name} · ${pathTail(repo.path)}` : repo.name,
				worktrees,
				running: live(repo) || worktrees.some(live),
			};
		})
		.sort(
			(a, b) =>
				Number(b.repo.pinned) - Number(a.repo.pinned) ||
				Number(b.running) - Number(a.running) ||
				byRecent(a.repo, b.repo) ||
				a.repo.name.localeCompare(b.repo.name),
		);
}

/** More projects than fit without hunting: below this a search field is clutter. */
const SEARCH_FROM = 8;
export const showSearch = (count: number): boolean => count >= SEARCH_FROM;

/** A typed search opens the stopped section: a hit behind a closed disclosure would read as "not found". */
export const searchOpensStopped = (q: string): boolean => q.trim() !== "";

function hit(p: TrayProject & { label?: string }, q: string): boolean {
	return [p.name, p.branch ?? "", p.label ?? "", p.path].some((f) => f.toLowerCase().includes(q));
}

/** Running stacks that match the search (all of them without one): the same fields as the project list. */
export function runningMatches(stacks: readonly TrayProject[], query: string): TrayProject[] {
	const q = query.trim().toLowerCase();
	return q ? stacks.filter((p) => hit(p, q)) : [...stacks];
}

/**
 * The project list as the popover shows it. Stacks that run are already rows of the "Running" section, so they are
 * left out here (a project is never listed twice); a group stays while its repo or a worktree is still unlisted.
 * Groups with a running stack but unlisted siblings come first, the rest sit behind "Stopped (n)", `n` counting rows.
 * A search keeps the groups where the repo or a worktree matches; if only a worktree matches, only it is listed.
 */
export function projectSections(
	groups: readonly ProjectGroup[],
	query: string,
): { running: ProjectGroup[]; stopped: ProjectGroup[]; stoppedCount: number } {
	const q = query.trim().toLowerCase();
	const rest = groups.flatMap((g): ProjectGroup[] => {
		const shown = !live(g.repo);
		const worktrees = g.worktrees.filter((w) => !live(w));
		if (!shown && !worktrees.length) return [];
		return [{ ...g, worktrees, repoShown: shown }];
	});
	const kept = q
		? rest.flatMap((g): ProjectGroup[] => {
				if (g.repoShown && hit(g.repo, q)) return [g];
				const worktrees = g.worktrees.filter((w) => hit(w, q) || hit(g.repo, q));
				return worktrees.length ? [{ ...g, worktrees }] : [];
			})
		: rest;
	const stopped = kept.filter((g) => !g.running);
	return {
		running: kept.filter((g) => g.running),
		stopped,
		stoppedCount: stopped.reduce((n, g) => n + (g.repoShown ? 1 : 0) + g.worktrees.length, 0),
	};
}

/**
 * The header's one line: what needs you, what is working, what runs. A failed service is red and leads. Stacks
 * count repos and worktrees alike; zero parts are left out.
 */
export function headerSummary(s: TraySnapshot): { tone: Tone; text: string } {
	const failed = s.projects.reduce((n, p) => n + failedServices(p).length, 0);
	const up = s.projects.filter((p) => p.running).length;
	const { needsYou, working } = s.agents;
	// Most urgent first; when the line would not fit beside the quick actions the least urgent parts are dropped.
	const parts = [
		needsYou ? `${needsYou} ${needsYou === 1 ? "needs" : "need"} you` : "",
		failed ? `${failed} failed` : "",
		working ? `${working} working` : "",
		up ? `${up} up` : "",
	].filter(Boolean);
	while (parts.length > 1 && parts.join(" · ").length > HEADER_MAX) parts.pop();
	const tone: Tone = failed ? "fail" : needsYou ? "warn" : parts.length ? "ok" : "off";
	return { tone, text: parts.length ? parts.join(" · ") : "All quiet" };
}

/** Characters of the status line that fit beside the mark and four quick actions in the 360 px popover. */
const HEADER_MAX = 28;
