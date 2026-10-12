<script lang="ts">
	import { Tabs, TabsList, TabsTrigger, TabsContent, toast } from "@codeatlasdev/ui";
	import { homeDir } from "@tauri-apps/api/path";
	import { errText } from "#lib/api.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { APPEARANCE_TABS, relativeFrom, type AppearanceTab } from "#lib/settings/index.ts";
	import PageHead from "#lib/settings/PageHead.svelte";
	import ThemeTab from "#lib/settings/ThemeTab.svelte";
	import CodeTab from "#lib/settings/CodeTab.svelte";
	import InterfaceTab from "#lib/settings/InterfaceTab.svelte";
	import TerminalTab from "#lib/settings/TerminalTab.svelte";

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
	title="Appearance"
	description="Colors, fonts and layout. Changes apply right away in every Atlas window."
/>

<Tabs bind:value={updates.tab as AppearanceTab} class="atabs">
	<TabsList>
		{#each APPEARANCE_TABS as t (t.id)}
			<TabsTrigger value={t.id}>{t.label}</TabsTrigger>
		{/each}
	</TabsList>
	<TabsContent value="appearance"><div class="tab"><ThemeTab /></div></TabsContent>
	<TabsContent value="editor"><div class="tab"><CodeTab /></div></TabsContent>
	<TabsContent value="interface"><div class="tab"><InterfaceTab /></div></TabsContent>
	<TabsContent value="terminal"><div class="tab"><TerminalTab /></div></TabsContent>
</Tabs>

<p class="foot">
	Advanced: <button class="link" onclick={editJson}>edit settings.json</button>
</p>

<style>
	.tab {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding-top: 14px;
	}
	.foot {
		margin: 0;
		font-size: 11.5px;
		color: var(--ca-text-hint);
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
