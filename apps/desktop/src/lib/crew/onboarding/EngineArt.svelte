<!-- Engine step right panel at rest: the engine install log while one installs, else the engine logo grid. -->
<script lang="ts">
	import { Check } from "@lucide/svelte";
	import { agentLogo } from "#lib/agentLogos.ts";
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();
</script>

{#if ob.installingAny}
	<div class="engine-install-log">
		<pre>{ob.installLog || "Starting…"}</pre>
	</div>
{:else}
	<div class="engine-art">
		{#each ob.integrations as agent (agent.id)}
			{@const logo = agentLogo(agent.id)}
			{@const sel = ob.selectedEngine === agent.id}
			<div class="ea-card" class:sel>
				<div class="ea-logo">
					{#if logo}
						{@html logo.dark ?? logo.light}
					{:else}
						<span class="ea-fallback">{agent.label[0]}</span>
					{/if}
				</div>
				{#if sel}
					<div class="ea-check"><Check size={14} /></div>
				{/if}
			</div>
		{/each}
	</div>
{/if}

<style>
	.engine-art {
		display: grid;
		grid-template-columns: repeat(2, 80px);
		gap: 20px;
	}

	.ea-card {
		width: 80px;
		height: 80px;
		border-radius: 18px;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-border-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		position: relative;
		transition:
			border-color 200ms,
			box-shadow 200ms,
			transform 200ms;
	}

	.ea-card.sel {
		border-color: var(--ca-accent, #4f8ef7);
		box-shadow: 0 0 0 3px rgba(79, 142, 247, 0.2);
		transform: scale(1.06);
	}

	.ea-card :global(svg) {
		width: 44px;
		height: 44px;
	}

	.ea-fallback {
		font-size: 24px;
		font-weight: 700;
		color: var(--ca-text-secondary);
	}

	.ea-check {
		position: absolute;
		bottom: -6px;
		right: -6px;
		width: 22px;
		height: 22px;
		border-radius: 50%;
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		display: flex;
		align-items: center;
		justify-content: center;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
	}

	.engine-install-log {
		width: 90%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}
	.engine-install-log pre {
		width: 100%;
		max-height: 80%;
		overflow-y: auto;
		font-family: monospace;
		font-size: 11.5px;
		line-height: 1.6;
		color: var(--ca-text-secondary);
		white-space: pre-wrap;
		word-break: break-all;
		background: var(--ca-surface-sunken);
		border-radius: 8px;
		padding: 12px 14px;
	}
</style>
