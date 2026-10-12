/**
 * The menu-bar popover (`/tray`) is a second webview with no workspace of its own: the main
 * window builds a `TraySnapshot` from its live stores and pushes it through Rust
 * (`tray_publish`), the popover renders it and sends clicks back as `TrayAction`s. Everything
 * decidable without the DOM lives here, so it is testable.
 */
import { summarize, type Tone } from "#lib/panel/metrics.ts";

export interface TrayService {
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
	services: TrayService[];
	/** Process-tree CPU (% of a core) and memory (MB), summed over the services. */
	cpu: number;
	mem: number;
	/** Recent total CPU, oldest first, for the sparkline. */
	cpuSeries: number[];
	/** First failed service and when the main window saw it fail (ms epoch). */
	failed: { service: string; at: number } | null;
}

export interface TraySnapshot {
	/** Epoch ms the main window built it; the popover shows ages relative to now. */
	at: number;
	projects: TrayProject[];
	agents: { working: number; needsYou: number; focusTab: string | null };
	terminals: { live: number };
	host: { status: "current" | "upgraded" | "outdated" | null; version: string | null };
	/** Newer release than the installed app/CLI, if any. */
	update: { version: string } | null;
	/** Dev build (cargo tauri dev or `atlas link`): updates are paused. */
	dev?: boolean;
}

export const EMPTY_SNAPSHOT: TraySnapshot = {
	at: 0,
	projects: [],
	agents: { working: 0, needsYou: 0, focusTab: null },
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
	| { kind: "update" };

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

/** Idle projects offered a Start button in the compact list under the cards. */
export function idleProjects(projects: TrayProject[]): TrayProject[] {
	return projects.filter((p) => !p.running).sort((a, b) => a.name.localeCompare(b.name));
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
			title: "Terminal host is outdated",
			detail: "Restart it when no terminal is busy",
			cta: "Open settings",
			action: { kind: "settings" },
		};
	}
	return null;
}

/** `3/4 up`-style services line. */
export function upLine(p: TrayProject): string {
	const up = p.services.filter((s) => s.state === "running").length;
	return `${up}/${p.services.length} up`;
}

/** Agents card subtitle. */
export function agentsLine(a: TraySnapshot["agents"]): string {
	if (!a.working && !a.needsYou) return "All quiet";
	const parts: string[] = [];
	if (a.needsYou) parts.push(`${a.needsYou} need${a.needsYou === 1 ? "s" : ""} you`);
	if (a.working) parts.push(`${a.working} working`);
	return parts.join(" · ");
}

/** Window height for the content: the popover grows with its cards up to a cap. */
export const TRAY_WIDTH = 360;
export const TRAY_MAX_HEIGHT = 560;
export const TRAY_MIN_HEIGHT = 220;

export function trayHeight(content: number): number {
	return Math.min(TRAY_MAX_HEIGHT, Math.max(TRAY_MIN_HEIGHT, Math.ceil(content)));
}
