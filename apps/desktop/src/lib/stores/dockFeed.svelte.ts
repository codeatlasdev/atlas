/**
 * Main window → agent dock. The dock (`/dock`, a separate webview) owns no stores: this module builds a
 * `DockSnapshot` of the agent terminals from the live ones and hands it to Rust (`dock_publish`), which caches it
 * and delivers the dock's text to a pty itself, so a send works with this window closed. The dock's "Open in
 * Atlas" comes back as the tray's `tray://action` (`trayFeed`), which already knows how to focus a tab.
 */
import { invoke } from "@tauri-apps/api/core";
import { buildSnapshot, type DockSnapshot } from "#lib/dock.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

/** Agent status changes are rare; this only folds the burst of a worktree switch into one publish. */
const PUBLISH_MS = 120;

/** Most recently active first across worktrees: the newest hook update, then the tab's place in its worktree's recents. */
function snapshot(): DockSnapshot {
	const ranked = [...ws.doc.tabs]
		.filter((t) => t.agent && t.pty && !t.chat)
		.sort((a, b) => (agents.status[b.id]?.updated_at ?? 0) - (agents.status[a.id]?.updated_at ?? 0));
	const order = new Map(ranked.map((t, i) => [t.id, i]));
	return buildSnapshot({
		now: Date.now(),
		tabs: ws.doc.tabs,
		worktree: (id) => {
			const w = ws.worktree(id);
			return w && { name: w.name, project: store.projects.find((p) => p.id === w.repoId)?.name ?? "" };
		},
		status: (id) => agents.status[id],
		recent: (id) => order.get(id) ?? Number.MAX_SAFE_INTEGER,
	});
}

/** Start publishing; returns cleanup. */
export function startDockFeed(): () => void {
	return $effect.root(() => {
		let timer: number | undefined;
		$effect(() => {
			const snap = snapshot();
			window.clearTimeout(timer);
			timer = window.setTimeout(
				() => void invoke("dock_publish", { snapshot: snap }).catch(() => {}),
				PUBLISH_MS,
			);
			return () => window.clearTimeout(timer);
		});
	});
}
