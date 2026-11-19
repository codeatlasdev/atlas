import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { m } from "#lib/i18n/m.ts";
import { openFile } from "#lib/stores/files.svelte.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { relativeFrom } from "./path.ts";

/**
 * Open an absolute `file` in an editor tab of the selected worktree and close Settings so
 * it is visible. Without a worktree there is no editor to show it in, so say so.
 */
export function openOutsideWorktree(file: string, name: string): boolean {
	const w = ws.selected;
	if (!w) {
		toast.info(m.settings_general_open_outside_no_worktree({ file: name }));
		return false;
	}
	try {
		openFile(w.id, relativeFrom(w.path, file.replace(/\/{2,}/g, "/")));
		updates.open = false;
		return true;
	} catch (e) {
		toast.error(errText(e));
		return false;
	}
}
