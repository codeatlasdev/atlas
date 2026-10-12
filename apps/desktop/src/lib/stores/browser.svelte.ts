import { invoke } from "@tauri-apps/api/core";
import { api } from "#lib/api.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
export { toUrl } from "#lib/url.ts";

const HISTORY_MAX = 50;

export function rememberUrl(worktreeId: string, url: string) {
	const h = ws.doc.urlHistory[worktreeId] ?? [];
	ws.doc.urlHistory[worktreeId] = [url, ...h.filter((u) => u !== url)].slice(0, HISTORY_MAX);
}

class BrowserPrefs {
	/** Where services/ports/links open: `atlas` (embedded) or `system`. */
	openIn = $state<"atlas" | "system">("atlas");

	async init() {
		const s = await api.settings();
		if (s.openServicesIn === "system") this.openIn = "system";
	}

	async set(v: "atlas" | "system") {
		this.openIn = v;
		await api.settingsPatch({ openServicesIn: v });
	}
}

export const browserPrefs = new BrowserPrefs();

/** Open a URL for a worktree (service, port, link): embedded tab or system browser. */
export async function openPreview(worktreeId: string, url: string, external = false) {
	if (external || browserPrefs.openIn === "system" || !/^https?:/i.test(url)) {
		await api.openUrl(url);
		return;
	}
	const existing = ws.tabsOf(worktreeId).find((t) => t.kind === "browser" && t.url === url);
	if (existing) ws.activate(existing.id);
	else ws.openTab(worktreeId, { kind: "browser", label: new URL(url).host, url });
	rememberUrl(worktreeId, url);
}
