<!--
  The install progress rail under the connect scene: every phase as a compact pill (done ✓ in the success colour, the
  active one in the accent with a pulse, pending muted, the failed one in the error colour) and, below, one line saying
  what the active (or failed) phase does. Same phases and order as the scene lights.
-->
<script lang="ts">
	import { Check } from "@lucide/svelte";
	import { installSteps, type InstallPhase } from "#lib/crew/installSteps.ts";

	let {
		done,
		active,
		failed,
		browsers = false,
	}: {
		done: Set<InstallPhase>;
		active: InstallPhase | null;
		failed: InstallPhase | null;
		/** The install also sets up the capture browsers: show that step. */
		browsers?: boolean;
	} = $props();

	const steps = $derived(installSteps(browsers));
	const now = $derived(steps.find((s) => s.phase === (failed ?? active)));
</script>

<div class="install-checklist">
	<div class="rail">
		{#each steps as s (s.phase)}
			{@const isDone = done.has(s.phase)}
			{@const isActive = active === s.phase && failed !== s.phase}
			{@const isFailed = failed === s.phase}
			<div class="checklist-row" class:done={isDone} class:active={isActive} class:failed={isFailed}>
				<span class="checklist-icon" aria-hidden="true">
					{#if isDone}
						<Check size={13} />
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
	<p class="rail-detail" class:bad={failed !== null}>
		{now ? now.detail : "About two minutes. Nothing on the server changes until its check passes."}
	</p>
</div>

<style>
	.install-checklist {
		display: flex;
		flex-direction: column;
		gap: 10px;
		width: 100%;
	}

	.rail {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 6px 20px;
	}

	.checklist-row {
		display: flex;
		align-items: center;
		gap: 9px;
		font-size: 13.5px;
		color: var(--ca-text-secondary);
		transition: color 250ms;
	}

	.checklist-row.done {
		color: var(--ob-ok, var(--ca-semantic-success));
	}

	.checklist-row.active {
		color: var(--ob-accent, var(--ca-accent));
		font-weight: 600;
	}

	.checklist-row.failed {
		color: var(--ob-danger, var(--ca-danger));
		font-weight: 600;
	}

	.checklist-icon {
		flex-shrink: 0;
		width: 18px;
		height: 18px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.idle-dot,
	.fail-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.idle-dot {
		background: var(--ca-text-secondary);
		opacity: 0.45;
	}

	.fail-dot {
		background: var(--ob-danger, var(--ca-danger));
	}

	.rail-detail {
		margin: 0;
		min-height: 2.6em;
		font-size: 12.5px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}

	.rail-detail.bad {
		color: var(--ob-danger, var(--ca-danger));
	}
</style>
