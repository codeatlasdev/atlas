<script lang="ts">
	/**
	 * The loose files open in the light editor, as one compact group like an editor's "Open Editors": newest first, the
	 * latest few shown, each closable (the x, or a middle click). Nothing here is saved: they are forgotten with the window.
	 */
	import { ChevronRight, FileText, X } from "@lucide/svelte";
	import { baseName } from "#lib/openPath.ts";
	import { loose } from "#lib/stores/loose.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	/** Rows shown before "Show all". */
	const SHOWN = 5;

	let open = $state(true);
	let all = $state(false);
	let shown = $derived(all ? loose.recent : loose.recent.slice(0, SHOWN));
</script>

{#if loose.recent.length}
	<div class="group" data-loose-group>
		<button class="head" aria-expanded={open} onclick={() => (open = !open)}>
			<ChevronRight size={11} class="chev {open ? 'open' : ''}" />
			<span>{m.sidebar_loose_files_files()}</span>
			<span class="count">{loose.recent.length}</span>
		</button>
		{#if open}
			{#each shown as f (f)}
				<div
					class="row"
					class:active={ws.view === "file" && loose.active === f}
					data-loose-row
					title={f}
					role="presentation"
					onauxclick={(e) => {
						if (e.button !== 1) return;
						e.preventDefault();
						loose.close(f);
					}}
				>
					<button class="name" onclick={() => loose.open(f)}>
						<FileText size={13} /><span>{baseName(f)}</span>
					</button>
					<button
						class="x"
						aria-label={m.sidebar_loose_files_close_file({ name: baseName(f) })}
						onclick={() => loose.close(f)}
					>
						<X size={11} />
					</button>
				</div>
			{/each}
			{#if loose.recent.length > SHOWN}
				<button class="more" onclick={() => (all = !all)}>
					{all
						? m.sidebar_loose_files_show_fewer()
						: m.sidebar_loose_files_show_all({ count: loose.recent.length })}
				</button>
			{/if}
		{/if}
	</div>
{/if}

<style>
	.group {
		display: flex;
		flex-direction: column;
		margin: 2px 0;
	}
	.head,
	.more,
	.name,
	.x {
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 6px;
		font-size: 11.5px;
		font-weight: 600;
		color: var(--ca-text-secondary);
		text-align: left;
	}
	.head :global(.chev) {
		transition: transform var(--dur-fast) var(--ease);
	}
	.head :global(.chev.open) {
		transform: rotate(90deg);
	}
	.count {
		margin-left: auto;
		font-weight: 500;
		font-size: 11px;
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
	}
	.row {
		display: flex;
		align-items: center;
		border-radius: 6px;
		color: var(--ca-text-secondary);
		font-size: 12.5px;
	}
	.row:hover,
	.row.active {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.name {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 4px 6px 4px 14px;
		text-align: left;
	}
	.name span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.x {
		display: inline-flex;
		padding: 4px;
		margin-right: 4px;
		border-radius: 4px;
		opacity: 0;
	}
	.row:hover .x,
	.x:focus-visible {
		opacity: 1;
	}
	.x:hover {
		background: var(--ca-action-hover);
	}
	.more {
		padding: 3px 6px 3px 14px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		text-align: left;
	}
	.more:hover {
		color: var(--ca-text-primary);
	}
</style>
