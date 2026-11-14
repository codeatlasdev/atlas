<!-- Engine step list: one card per engine with its status badge; a click installs, signs in or selects it. -->
<script lang="ts">
	import { agentLogo } from "#lib/agentLogos.ts";
	import { AGENT_KEY_LABEL } from "#lib/crew/remoteAgent.ts";
	import { engineBadge, engineBusy, engineStatusLabel } from "./engines.ts";
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();
</script>

<div class="engine-list" style:--stagger={3}>
	{#each ob.integrations as agent (agent.id)}
		{@const status = ob.engineStatus[agent.id] ?? { phase: "idle" }}
		{@const logo = agentLogo(agent.id)}
		{@const isSel = ob.selectedEngine === agent.id}
		{@const isActive = ob.signinActive === agent.id}
		<button
			class="engine-card"
			class:selected={isSel || isActive}
			onclick={() => ob.handleEngineCardClick(agent.id)}
			aria-pressed={isSel}
			aria-label="{agent.label} – {engineStatusLabel(status.phase)}"
		>
			<div class="engine-logo">
				{#if logo}
					{@html logo.dark ?? logo.light}
				{:else}
					<div class="engine-logo-fallback">{agent.label[0]}</div>
				{/if}
			</div>
			<div class="engine-info">
				<span class="engine-name">{agent.label}</span>
				{#if status.phase === "install_error"}
					<span class="engine-auth err-text">{status.error?.slice(0, 60)}</span>
				{:else}
					<span class="engine-auth">{AGENT_KEY_LABEL[agent.id] ?? ""}</span>
				{/if}
			</div>
			<span class="engine-status badge-{engineBadge(status.phase)}" aria-hidden="true">
				{#if engineBusy(status.phase)}
					<span class="spin-dot"></span>
				{/if}
				{engineStatusLabel(status.phase)}
			</span>
		</button>
	{/each}
</div>

<style>
	.engine-list {
		display: flex;
		flex-direction: column;
		gap: 10px;
		margin-bottom: 28px;
	}

	.engine-card {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 14px 16px;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-border-subtle);
		border-radius: 10px;
		cursor: pointer;
		text-align: left;
		transition:
			border-color 160ms,
			box-shadow 160ms,
			transform 120ms;
	}

	.engine-card:hover:not(:disabled) {
		border-color: var(--ca-accent, #4f8ef7);
		box-shadow: 0 2px 12px rgba(79, 142, 247, 0.12);
	}

	.engine-card:active:not(:disabled) {
		transform: scale(0.99);
	}

	.engine-card.selected {
		border-color: var(--ca-accent, #4f8ef7);
		box-shadow:
			0 0 0 3px rgba(79, 142, 247, 0.15),
			0 4px 16px rgba(79, 142, 247, 0.12);
		transform: translateY(-1px);
	}

	.engine-card:disabled {
		cursor: default;
		opacity: 0.6;
	}

	.engine-logo {
		flex-shrink: 0;
		width: 40px;
		height: 40px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.engine-logo :global(svg) {
		width: 40px;
		height: 40px;
	}

	.engine-logo-fallback {
		width: 40px;
		height: 40px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 700;
		font-size: 18px;
		color: var(--ca-text-secondary);
	}

	.engine-info {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.engine-name {
		font-size: 15px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}

	.engine-auth {
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}

	.err-text {
		color: var(--ca-danger, #ef4444) !important;
	}

	.engine-status {
		flex-shrink: 0;
		font-size: 11.5px;
		font-weight: 600;
		padding: 4px 9px;
		border-radius: 5px;
		border: none;
		cursor: default;
	}

	.engine-status.badge-ready {
		background: rgba(34, 197, 94, 0.12);
		color: var(--ca-semantic-success);
	}
	.engine-status.badge-action {
		background: rgba(79, 142, 247, 0.12);
		color: var(--ca-accent, #4f8ef7);
		cursor: pointer;
	}
	.engine-status.badge-busy {
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.engine-status.badge-error {
		background: rgba(239, 68, 68, 0.12);
		color: var(--ca-danger, #ef4444);
		cursor: pointer;
	}
	.engine-card.selected .engine-status.badge-action {
		background: rgba(79, 142, 247, 0.2);
	}
</style>
