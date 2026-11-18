/** The editor behind the focused pane's active tab (file or atlas.yaml), if it is one. */
import { toast } from "@codeatlasdev/ui";
import { m } from "#lib/i18n/m.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { editors, saveEditor } from "#lib/symbols.ts";

/** Registry key of the focused pane's editor, if its active tab is one. */
function activeEditorKey() {
	const w = ws.selected;
	const t = w && ws.tab(ws.focusedGroup(w.id).activeTab);
	if (t?.kind === "config") return `config:${ws.worktree(t.worktreeId)?.repoId}`;
	return t?.kind === "editor" && t.path ? `${t.worktreeId}:${t.path}` : undefined;
}

export function activeEditor() {
	const key = activeEditorKey();
	return key ? editors.get(key) : undefined;
}

/** ⌘S: write the focused editor now (the autosave timer would do it in 0.5 s). */
export async function saveActive() {
	if (!(await saveEditor(activeEditorKey()))) toast.info(m.shell_commands_nothing_to_save());
}
