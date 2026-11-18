<script lang="ts">
	/** SidebarHeader — the sidebar's window drag band and the search / new-crewmate buttons. */
	import { Plus, Search } from "@lucide/svelte";
	import { kbd } from "#lib/keys.ts";
	import { palettes } from "#lib/shell/palettes.svelte.ts";
	import { recruit } from "#lib/crew/workspace/actions.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
</script>

<!-- Drag region -->
<div class="col-drag" data-tauri-drag-region aria-hidden="true"></div>

<!-- Top bar: search + add -->
<div class="top-bar">
	<button
		class="icon-btn-round"
		onclick={() => (palettes.crew = true)}
		aria-label={m.crew_workspace_sidebar_header_search({ key: kbd("cmd-k") })}
	>
		<Search size={16} />
	</button>
	<button
		class="icon-btn-round"
		onclick={() => void recruit.start()}
		aria-label={m.crew_workspace_sidebar_header_new_crewmate()}
		disabled={recruit.creating}
	>
		<Plus size={16} />
	</button>
</div>

<style>
	/* Drag region at top */
	.col-drag {
		height: 12px;
		flex: none;
	}

	/* Top bar */
	.top-bar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 4px;
		padding: 4px 8px;
		flex: none;
	}

	.icon-btn-round {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		border-radius: 50%;
		border: none;
		background: var(--ca-surface-raised, var(--ca-vibrancy-fill));
		color: var(--ca-text-secondary);
		cursor: default;
		flex: none;
		transition: background 150ms;
		padding: 0;
	}
	.icon-btn-round:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.icon-btn-round:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
