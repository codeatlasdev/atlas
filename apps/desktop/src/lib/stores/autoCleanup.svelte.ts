/**
 * Auto clean up of linked worktrees (#58), off by default. Settings → Workspaces picks `merged` (the worktree's PR
 * was merged) or `idle` (no agent activity for N days). Rust (`worktree_auto_remove`) has the last word: it keeps a
 * dirty or unpushed worktree, never forces, and keeps the branch, so a cleaned worktree is restorable for 7 days
 * from the list in Settings.
 */
import { toast } from "@codeatlasdev/ui";
import { api, errText } from "#lib/api.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { candidates, cleanupPrefs, restorable, type Cleaned } from "#lib/stores/autoCleanup.ts";

const EVERY_MS = 15 * 60_000;

let sweeping = false;

export async function sweep(now = Date.now()) {
	if (sweeping) return;
	sweeping = true;
	try {
		const s = await api.settings();
		const prefs = cleanupPrefs(s);
		const before = restorable(s.cleanedWorktrees, now);
		const log = [...before];
		// A worktree whose stack is up is in use, whatever its agents or its PR say: auto clean up never stops it.
		const busy = new Set(store.projects.filter((p) => p.running).map((p) => p.id));
		for (const w of candidates(ws.doc.worktrees, prefs, ws.doc.selected, now, busy)) {
			const r = await api.worktreeAutoRemove(w.id, prefs.mode === "merged").catch((e) => {
				toast.error(errText(e));
				return null;
			});
			if (!r || !("removed" in r)) continue;
			ws.removeWorktree(w.id);
			if (store.selected === w.id) store.selected = null;
			log.push({ repoId: w.repoId, name: w.name, branch: r.branch ?? w.branch, at: now });
			const why = prefs.mode === "merged" ? "its PR was merged" : `idle for ${prefs.days} days`;
			toast.info(
				`Cleaned up ${w.name}: ${why}. Restore it from Settings → Workspaces within 7 days.`,
				10_000,
			);
		}
		const raw = Array.isArray(s.cleanedWorktrees) ? s.cleanedWorktrees.length : 0;
		if (log.length !== raw) await api.settingsPatch({ cleanedWorktrees: log.length ? log : null });
		if (log.length > before.length) await store.refresh();
	} catch (e) {
		toast.error(errText(e));
	} finally {
		sweeping = false;
	}
}

/** First sweep a minute after launch (the workspace and gh auth settle first), then every 15 minutes. */
export function startAutoCleanup(): () => void {
	const first = setTimeout(() => void sweep(), 60_000);
	const every = setInterval(() => void sweep(), EVERY_MS);
	return () => {
		clearTimeout(first);
		clearInterval(every);
	};
}
