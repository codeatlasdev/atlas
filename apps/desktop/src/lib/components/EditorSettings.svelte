<script lang="ts">
	/** Settings ▸ Editor ▸ Editor: the code font and editing prefs. The tab strip is drawn by Settings. */
	import { toast } from "@codeatlasdev/ui";
	import { homeDir } from "@tauri-apps/api/path";
	import { errText } from "#lib/api.ts";
	import { relativeFrom } from "#lib/settings/index.ts";
	import CodeTab from "#lib/settings/CodeTab.svelte";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	/** Opens settings.json in an editor tab of the current worktree (the editor takes worktree-relative paths). */
	async function editJson() {
		const w = ws.selected;
		if (!w) {
			toast.info(m.settings_editor_open_worktree_first());
			return;
		}
		try {
			const file = `${await homeDir()}/.atlas/desktop/settings.json`.replace(/\/{2,}/g, "/");
			openFile(w.id, relativeFrom(w.path, file));
			updates.open = false;
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

<CodeTab />

<p class="foot">
	{m.settings_editor_advanced()} <button class="link" onclick={editJson}>{m.settings_editor_edit_settings_json()}</button>
</p>

<style>
	.foot {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.link {
		border: 0;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--ca-text-secondary);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.link:hover {
		color: var(--ca-text-primary);
	}
</style>
