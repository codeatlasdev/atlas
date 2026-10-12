/**
 * Main window ↔ menu-bar popover. The popover (`/tray`, a separate webview) owns no stores:
 * this module builds a `TraySnapshot` from the live ones (so runners are subscribed once,
 * by this window), pushes it through Rust, and carries out the clicks the popover sends back.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { metrics } from "#lib/panel/metrics.svelte.ts";
import { gpu } from "#lib/stores/gpu.svelte.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { shell } from "#lib/shell/shell.svelte.ts";
import { hueOf } from "#lib/shell/space.ts";
import { trayIconOf } from "#lib/shell/identity.ts";
import { identities } from "#lib/stores/identity.svelte.ts";
import type { TrayAction, TrayProject, TraySnapshot } from "#lib/tray.ts";

const SPARK_POINTS = 30;
/** The popover is not looking: at most one snapshot per this many ms. */
const PUBLISH_MS = 1000;

/** When each (project, service) was first seen failed, to say "failed 2 min ago". */
const failedAt = new Map<string, number>();

function projects(): TrayProject[] {
	void metrics.rev;
	const now = Date.now();
	return store.sorted
		.filter((p) => !p.host)
		.map((p) => {
			const live = store.live[p.id];
			const rows = (live?.services ?? []).filter((s) => !s.deselected && s.state !== "disabled");
			const names = rows.map((s) => s.name);
			let failed: TrayProject["failed"] = null;
			for (const s of rows) {
				const key = `${p.id}:${s.name}`;
				if (p.running && s.state === "failed") {
					const at = failedAt.get(key) ?? now;
					failedAt.set(key, at);
					if (!failed || at > failed.at) failed = { service: s.name, at };
				} else failedAt.delete(key);
			}
			return {
				id: p.id,
				name: p.name,
				hue: shell.hues[p.parent ?? p.id] ?? hueOf(p.parent ?? p.id),
				icon: trayIconOf(identities.iconOf(p.parent ?? p.id), p.name, identities.dataUrlOf(p.parent ?? p.id)),
				running: p.running,
				branch: p.branch,
				services: rows.map((s) => ({ name: s.name, state: s.state })),
				cpu: Math.round(rows.reduce((t, s) => t + (s.cpu ?? 0), 0)),
				mem: Math.round(rows.reduce((t, s) => t + (s.mem_mb ?? 0), 0)),
				cpuSeries: p.running ? metrics.stacked(p.id, names, "cpu", SPARK_POINTS) : [],
				failed,
			};
		});
}

function snapshot(): TraySnapshot {
	const states = Object.values(agents.status);
	const needs = ws.doc.tabs.filter(
		(t) => t.agent && ["waiting", "blocked"].includes(agents.status[t.id]?.state),
	);
	return {
		at: Date.now(),
		projects: projects(),
		agents: {
			working: ws.doc.tabs.filter((t) => t.agent && agents.status[t.id]?.state === "working").length,
			needsYou: needs.length || states.filter((s) => s.state === "waiting" || s.state === "blocked").length,
			focusTab: needs[0]?.id ?? null,
		},
		terminals: { live: Object.values(ws.ptys).filter((p) => !p.exited && !(p.pty in ws.exited)).length },
		host: { status: gpu.hostTooOld ? "outdated" : "current", version: updates.info?.cli.version ?? null },
		update: updates.pill && updates.release ? { version: updates.release.version } : null,
		dev: updates.dev,
	};
}

function carryOut(a: TrayAction) {
	switch (a.kind) {
		case "open":
			break;
		case "settings":
			updates.show();
			break;
		case "update":
			updates.showWhatsNew();
			break;
		case "project":
			ws.view = "worktree";
			store.selected = a.id;
			ws.select(a.id);
			break;
		case "logs": {
			ws.view = "worktree";
			store.selected = a.id;
			ws.select(a.id);
			store.focusedService = a.service;
			ws.reveal(a.id, (t) => t.kind === "logs", { kind: "logs", label: "Logs" });
			break;
		}
		case "agents": {
			const t = a.tab ? ws.tab(a.tab) : undefined;
			if (t) {
				ws.view = "worktree";
				store.selected = t.worktreeId;
				ws.select(t.worktreeId);
				ws.activate(t.id);
			} else ws.view = "agents";
			break;
		}
		case "terminal":
			ws.view = "worktree";
			if (ws.selected) void ws.newTerminal(ws.selected.id);
			break;
	}
}

/** Start publishing and listening; returns cleanup. */
export function startTrayFeed(): () => void {
	let off: UnlistenFn | null = null;
	let gone = false;
	void listen<TrayAction>("tray://action", (e) => carryOut(e.payload)).then((f) => (gone ? f() : (off = f)));
	const stop = $effect.root(() => {
		let last = 0;
		let timer: number | undefined;
		$effect(() => {
			const snap = snapshot();
			const wait = Math.max(0, last + PUBLISH_MS - Date.now());
			window.clearTimeout(timer);
			timer = window.setTimeout(() => {
				last = Date.now();
				void invoke("tray_publish", { snapshot: snap }).catch(() => {});
			}, wait);
			return () => window.clearTimeout(timer);
		});
	});
	return () => {
		gone = true;
		off?.();
		stop();
	};
}
