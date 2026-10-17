/** HTML live preview: start the Rust live server, show it in an embedded browser tab to the right. */
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { rememberUrl } from "#lib/stores/browser.svelte.ts";
import { isHtml, originOf } from "#lib/stores/preview.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { saveEditor } from "#lib/symbols.ts";

/** Below the server's 2 min idle limit, so open previews never expire. */
const TOUCH_MS = 30_000;

/** Server roots by `host:port`, for keep-alive pings while a tab shows one. */
const roots = new Map<string, string>();
let timer: number | undefined;

function touch() {
	const live = new Set(ws.doc.tabs.filter((t) => t.kind === "browser").map((t) => originOf(t.url)));
	for (const [origin, root] of roots) {
		if (live.has(origin)) void invoke("live_touch", { root });
		// No tab left: let the server expire on its own, stop pinging it.
		else roots.delete(origin);
	}
	if (!roots.size) {
		clearInterval(timer);
		timer = undefined;
	}
}

/** Open `relPath` (relative to the worktree) of an HTML file in the live preview. */
export async function previewHtml(worktreeId: string, relPath: string) {
	const w = ws.worktree(worktreeId);
	if (!w || !isHtml(relPath)) return;
	try {
		await saveEditor(`${worktreeId}:${relPath}`);
		const { url, root } = await invoke<{ url: string; root: string }>("live_open", {
			path: `${w.path}/${relPath}`,
			worktree: w.path,
		});
		roots.set(originOf(url)!, root);
		timer ??= window.setInterval(touch, TOUCH_MS);
		rememberUrl(worktreeId, url);
		// `path` marks a preview tab and which file it shows. After an app restart the tab is
		// restored with a dead port: reuse it and point it at the fresh server.
		const existing = ws.tabsOf(worktreeId).find((t) => t.kind === "browser" && t.path === relPath);
		if (existing) {
			if (existing.url !== url) {
				existing.url = url;
				await invoke("browser_navigate", { tab: existing.id, url }).catch(() => {});
			}
			ws.activate(existing.id);
			return;
		}
		const from = ws.focusedGroup(worktreeId);
		const target = ws.sideGroup(worktreeId);
		ws.openTab(
			worktreeId,
			{ kind: "browser", label: relPath.split("/").pop() ?? relPath, url, path: relPath },
			target,
		);
		// Keep the editor focused: the preview is a side view of what is being typed.
		ws.focusGroup(from.id);
	} catch (e) {
		toast.error(`Preview failed: ${errText(e)}`);
	}
}
