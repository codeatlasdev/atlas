<script lang="ts">
	import { PanelRight } from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { docks, peek, toggleDock } from "#lib/stores/docks.svelte.ts";

	/** Header button that pins or hides the right sidebar (⌘R). Always visible, with its shortcut in the tooltip. */
	const label = $derived(docks.right ? "Hide right sidebar" : "Show right sidebar");
</script>

<Tip {label} command="workspace::ToggleRightDock">
	{#snippet child(props)}
		<button
			{...props}
			class="right-toggle"
			class:on={docks.right}
			class:peeking={peek.right && !docks.right}
			data-tour="right-toggle"
			aria-label={label}
			aria-pressed={docks.right}
			onclick={() => toggleDock("right")}
		>
			<PanelRight size={15} />
		</button>
	{/snippet}
</Tip>

<style>
	.right-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 24px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		transition:
			background var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease);
	}
	.right-toggle:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.right-toggle.on {
		color: var(--space-accent);
		background: color-mix(in oklab, var(--space-accent) 14%, transparent);
	}
	.right-toggle.peeking {
		color: var(--ca-text-primary);
	}
</style>
