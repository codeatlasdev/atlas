<!-- Right panel of the connect step: the crew-server install checklist (ticked / spinning / failed phases). -->
<script lang="ts">
	import { Check } from "@lucide/svelte";
	import { INSTALL_STEPS, type InstallPhase } from "#lib/crew/installSteps.ts";

	let {
		done,
		active,
		failed,
	}: { done: Set<InstallPhase>; active: InstallPhase | null; failed: InstallPhase | null } = $props();
</script>

<div class="install-checklist">
	{#each INSTALL_STEPS as s (s.phase)}
		{@const isDone = done.has(s.phase)}
		{@const isActive = active === s.phase}
		{@const isFailed = failed === s.phase}
		<div class="checklist-row" class:done={isDone} class:active={isActive} class:failed={isFailed}>
			<span class="checklist-icon" aria-hidden="true">
				{#if isDone}
					<Check size={14} />
				{:else if isFailed}
					<span class="fail-dot"></span>
				{:else if isActive}
					<span class="spin-dot"></span>
				{:else}
					<span class="idle-dot"></span>
				{/if}
			</span>
			<span class="checklist-label">{s.label}</span>
		</div>
	{/each}
</div>

<style>
	.install-checklist {
		display: flex;
		flex-direction: column;
		gap: 14px;
		width: min(340px, 88%);
	}

	.checklist-row {
		display: flex;
		align-items: center;
		gap: 12px;
		font-size: 15px;
		color: var(--ca-text-secondary);
		transition: color 250ms;
	}

	.checklist-row.done {
		color: var(--ca-text-primary);
	}

	.checklist-row.active {
		color: var(--ca-text-primary);
	}

	.checklist-row.failed {
		color: var(--ca-danger, #ef4444);
	}

	.checklist-icon {
		flex-shrink: 0;
		width: 22px;
		height: 22px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.checklist-row.done .checklist-icon {
		color: var(--ca-semantic-success);
	}

	.idle-dot,
	.fail-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.idle-dot {
		background: var(--ca-outline-subtle);
	}

	.fail-dot {
		background: var(--ca-danger, #ef4444);
	}
</style>
