/** Show a language server's log: open the file in an editor tab when it lives under a worktree. */
import { toast } from "@codeatlasdev/ui";
import { serverLog } from "#lib/lsp/client.svelte.ts";
import { locate } from "#lib/lsp/workspace.ts";
import { openFile } from "#lib/stores/files.svelte.ts";
import { m } from "#lib/i18n/m.ts";

/**
 * Logs normally sit under `~/.atlas`, outside every worktree, so the editor cannot open them: the
 * path is copied instead and said so. Returns the text for callers with a place to show it.
 */
export async function openLog(name: string): Promise<{ path: string; text: string } | undefined> {
	try {
		const log = await serverLog(name);
		const hit = locate(log.path);
		if (hit) {
			openFile(hit.id, hit.rel);
			return log;
		}
		await navigator.clipboard.writeText(log.path).catch(() => {});
		toast.info(m.workbench_lsp_log_path_copied({ path: log.path }));
		return log;
	} catch {
		toast.error(m.workbench_lsp_log_unreadable());
		return undefined;
	}
}
