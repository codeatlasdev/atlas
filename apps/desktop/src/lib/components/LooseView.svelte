<script lang="ts">
	/**
	 * The main area while a loose file is open (`ws.view === "file"`): a strip of the open files, the light editor and
	 * the two ways out: its folder as a plain folder, or as a project. Nothing here touches the project registry until
	 * one of those is pressed.
	 */
	import { Button, toast } from "@codeatlasdev/ui";
	import { X } from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { openLooseInFolder } from "#lib/ctl.ts";
	import { tilde } from "#lib/format.ts";
	import { baseName, dirOf } from "#lib/openPath.ts";
	import { docks } from "#lib/stores/docks.svelte.ts";
	import { loose } from "#lib/stores/loose.svelte.ts";
	import LooseEditor from "./LooseEditor.svelte";

	let busy = $state(false);

	async function openFolder(path: string, asProject: boolean) {
		busy = true;
		try {
			await openLooseInFolder(path, asProject);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = false;
		}
	}
</script>

<header data-tauri-drag-region class:inset={!docks.sidebar}>
	<div class="tabs" role="tablist" aria-label="Open files">
		{#each loose.files as f (f)}
			<div class="tab" class:on={f === loose.active}>
				<button
					type="button"
					role="tab"
					aria-selected={f === loose.active}
					title={tilde(f)}
					onclick={() => loose.open(f)}>{baseName(f)}</button
				>
				<button type="button" class="x" aria-label="Close {baseName(f)}" onclick={() => loose.close(f)}>
					<X size={11} />
				</button>
			</div>
		{/each}
	</div>
	<div class="grow" data-tauri-drag-region></div>
	{#if loose.active}
		{@const f = loose.active}
		<span class="where" title={dirOf(f)}>{tilde(dirOf(f))}</span>
		<Button variant="ghost" size="sm" disabled={busy} onclick={() => openFolder(f, false)}>Open folder</Button
		>
		<Button size="sm" disabled={busy} onclick={() => openFolder(f, true)}>Open as project</Button>
	{/if}
</header>
<div class="rule"></div>
{#if loose.active}
	{#key loose.active}
		<LooseEditor path={loose.active} />
	{/key}
{/if}

<style>
	header {
		display: flex;
		align-items: center;
		gap: 8px;
		height: var(--title-row);
		flex: none;
		padding: 0 10px;
		transition: padding-left var(--dur-slow) var(--ease);
	}
	/* Sidebar hidden: the content reaches the window's left edge, so keep clear of the traffic lights. */
	header.inset {
		padding-left: 70px;
	}
	:global(html.opaque-window) header.inset {
		padding-left: 10px;
	}
	:global(html.custom-chrome) header {
		padding-right: var(--caption-controls);
	}
	.grow {
		flex: 1;
		align-self: stretch;
	}
	.rule {
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.tabs {
		display: flex;
		gap: 2px;
		min-width: 0;
		overflow: hidden;
	}
	.tab {
		display: flex;
		align-items: center;
		border-radius: 6px;
		color: var(--ca-text-secondary);
		font-size: 12.5px;
	}
	.tab.on {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.tab button {
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		padding: 3px 8px;
		max-width: 220px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tab .x {
		display: inline-flex;
		padding: 3px;
		margin-right: 3px;
		border-radius: 4px;
	}
	.tab .x:hover {
		background: var(--ca-action-hover);
	}
	.where {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 260px;
	}
</style>
