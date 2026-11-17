<!--
  Engine step panel: the crew server with the four engines around it. An engine that installs sends a packet into the
  rack (progress ring on its card); one that needs a sign-in shows a key; a ready one gets a green check; the chosen
  one docks into the rack. The last line of the install log runs under it.
-->
<script lang="ts">
	import { Check, KeyRound } from "@lucide/svelte";
	import { agentLogo } from "#lib/agentLogos.ts";
	import { reducedMotion } from "#lib/crew/motion.ts";
	import ServerRack from "./ServerRack.svelte";
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();

	const rack = $derived(ob.installingAny ? "active" : ob.engineReady ? "ok" : "idle");
	const lastLog = $derived(ob.installLog.split("\n").filter(Boolean).at(-1) ?? "");
</script>

<div class="engine-art" class:still={reducedMotion} data-testid="engine-scene" data-state={rack}>
	<div class="stage">
		<svg class="rack-svg" viewBox="0 0 150 176" aria-hidden="true"><ServerRack state={rack} /></svg>
		{#each ob.integrations as agent, i (agent.id)}
			{@const logo = agentLogo(agent.id)}
			{@const phase = ob.engineStatus[agent.id]?.phase ?? "idle"}
			{@const sel = ob.selectedEngine === agent.id}
			<div
				class="card pos{i}"
				class:sel
				class:installing={phase === "installing"}
				class:needs={phase === "needs_signin"}
				class:ready={phase === "ready"}
				data-phase={phase}
			>
				<span class="logo">
					{#if logo}{@html logo.dark ?? logo.light}{:else}<span class="fb">{agent.label[0]}</span>{/if}
				</span>
				{#if phase === "installing"}<span class="ring"></span>{/if}
				{#if phase === "ready"}<span class="badge ok"><Check size={12} /></span>{/if}
				{#if phase === "needs_signin"}<span class="badge key"><KeyRound size={12} /></span>{/if}
			</div>
		{/each}
	</div>
	<p class="caption">
		{#if ob.installingAny}
			{lastLog || "Installing on the server…"}
		{:else if ob.engineReady}
			Ready on your server.
		{:else}
			Pick one: Atlas installs it on the server and signs you in.
		{/if}
	</p>
</div>

<style>
	.engine-art {
		box-sizing: border-box;
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 16px;
		padding: 24px;
	}

	.stage {
		position: relative;
		width: min(420px, 100%);
		aspect-ratio: 1 / 0.9;
		flex-shrink: 1;
		min-height: 0;
	}

	.rack-svg {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 36%;
		transform: translate(-50%, -50%);
	}

	.card {
		position: absolute;
		width: 24%;
		aspect-ratio: 1;
		border-radius: 18px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--ob-panel, var(--ca-surface-base));
		border: 2px solid var(--ob-line-idle, var(--ca-border-subtle));
		transition:
			transform 500ms cubic-bezier(0.34, 1.2, 0.64, 1),
			border-color 250ms,
			opacity 250ms;
	}

	.pos0 {
		left: 4%;
		top: 6%;
	}
	.pos1 {
		right: 4%;
		top: 6%;
	}
	.pos2 {
		left: 4%;
		bottom: 6%;
	}
	.pos3 {
		right: 4%;
		bottom: 6%;
	}

	.logo {
		display: flex;
		width: 54%;
		height: 54%;
	}

	.logo :global(svg) {
		width: 100%;
		height: 100%;
	}

	.fb {
		font-size: 24px;
		font-weight: 700;
		color: var(--ca-text-secondary);
	}

	.card.installing,
	.card.needs {
		border-color: var(--ob-accent, var(--ca-accent));
		animation: ob-nudge 1300ms ease-in-out infinite;
	}

	.card.needs {
		border-color: var(--ob-warm, var(--ca-semantic-warning));
	}

	.card.ready {
		border-color: var(--ob-ok, var(--ca-semantic-success));
	}

	/* the chosen engine docks into the rack: it slides to the centre and shrinks onto it */
	.card.sel {
		border-color: var(--ob-ok, var(--ca-semantic-success));
		z-index: 2;
	}

	.card.sel.pos0 {
		transform: translate(60%, 70%) scale(0.7);
	}
	.card.sel.pos1 {
		transform: translate(-60%, 70%) scale(0.7);
	}
	.card.sel.pos2 {
		transform: translate(60%, -70%) scale(0.7);
	}
	.card.sel.pos3 {
		transform: translate(-60%, -70%) scale(0.7);
	}

	.ring {
		position: absolute;
		inset: -7px;
		border-radius: 22px;
		border: 2px solid transparent;
		border-top-color: var(--ob-accent, var(--ca-accent));
		animation: ob-spin 900ms linear infinite;
	}

	.badge {
		position: absolute;
		right: -7px;
		bottom: -7px;
		width: 22px;
		height: 22px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--ca-surface-base);
	}

	.badge.ok {
		background: var(--ob-ok, var(--ca-semantic-success));
	}

	.badge.key {
		background: var(--ob-warm, var(--ca-semantic-warning));
	}

	.caption {
		margin: 0;
		min-height: 2.6em;
		max-width: 420px;
		text-align: center;
		font-size: 13px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
	}

	@keyframes ob-nudge {
		50% {
			transform: translateY(-4px);
		}
	}

	@keyframes ob-spin {
		to {
			transform: rotate(360deg);
		}
	}

	.still *,
	.still {
		animation: none !important;
		transition: none !important;
	}
</style>
