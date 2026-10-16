<script lang="ts">
	/** Settings ▸ Editor: the code font and editing prefs, and the language servers, in two tabs. */
	import { Tabs, TabsContent, TabsList, TabsTrigger, toast } from "@codeatlasdev/ui";
	import { homeDir } from "@tauri-apps/api/path";
	import { errText } from "#lib/api.ts";
	import { SECTION_TABS, relativeFrom, type SettingsTab } from "#lib/settings/index.ts";
	import CodeTab from "#lib/settings/CodeTab.svelte";
	import LanguagesTab from "#lib/settings/LanguagesTab.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";

	/** Opens settings.json in an editor tab of the current worktree (the editor takes worktree-relative paths). */
	async function editJson() {
		const w = ws.selected;
		if (!w) {
			toast.info("Open a worktree first, then Atlas can show settings.json in its editor.");
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

<PageHead
	title="Editor"
	description="The code editor and diffs: font, indentation and display, and the language servers behind completion and diagnostics."
/>

<Tabs bind:value={updates.tab as SettingsTab}>
	<TabsList>
		{#each SECTION_TABS.editor ?? [] as t (t.id)}
			<TabsTrigger value={t.id}>{t.label}</TabsTrigger>
		{/each}
	</TabsList>
	<TabsContent value="editor"><div class="tab"><CodeTab /></div></TabsContent>
	<TabsContent value="languages"><div class="tab"><LanguagesTab /></div></TabsContent>
</Tabs>

<p class="foot">
	Advanced: <button class="link" onclick={editJson}>edit settings.json</button>
</p>

<style>
	.tab {
		display: flex;
		flex-direction: column;
		gap: 20px;
		padding-top: 16px;
	}
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
