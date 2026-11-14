<!-- Ready step right panel: the first crewmate's card (large face, name, engine badge). -->
<script lang="ts">
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import { agentLogo } from "#lib/agentLogos.ts";
	import type { CrewmateLook } from "#lib/crew/look.ts";

	let { engine, look }: { engine: string; look: CrewmateLook } = $props();

	const logo = $derived(agentLogo(engine || "claude"));
</script>

<div class="ready-hero">
	<div class="hero-face-wrap">
		<div class="hero-bob">
			<CrewmateFace seed="onb-preview" {look} state="idle" size={140} />
		</div>
		{#if logo}
			<span class="hero-badge">{@html logo.dark ?? logo.light}</span>
		{/if}
	</div>
	<div class="hero-info">
		<span class="hero-name">My First Crewmate</span>
		<span class="hero-sub">No tasks yet</span>
	</div>
</div>

<style>
	.ready-hero {
		width: min(360px, 88%);
		padding: 36px 32px 28px;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-border-subtle);
		border-radius: 20px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 20px;
		box-shadow: 0 12px 48px rgba(0, 0, 0, 0.28);
		animation: hero-spring 560ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
	}

	@keyframes hero-spring {
		from {
			opacity: 0;
			transform: translateY(32px) scale(0.88);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.hero-face-wrap {
		position: relative;
		flex-shrink: 0;
	}

	/* periodic hop after the spring entrance */
	.hero-bob {
		animation: face-hop 3.6s ease-in-out infinite;
		animation-delay: 700ms;
	}

	@keyframes face-hop {
		0%,
		75%,
		100% {
			transform: translateY(0);
		}
		88% {
			transform: translateY(-9px);
		}
	}

	.hero-badge {
		position: absolute;
		bottom: -4px;
		right: -4px;
		width: 30px;
		height: 30px;
		background: var(--ca-surface-base);
		border: 2.5px solid var(--ca-surface-base);
		border-radius: 9px;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.18);
	}

	.hero-badge :global(svg) {
		width: 22px;
		height: 22px;
	}

	.hero-info {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 5px;
		text-align: center;
	}

	.hero-name {
		font-size: 26px;
		font-weight: 700;
		color: var(--ca-text-primary);
		letter-spacing: -0.02em;
	}

	.hero-sub {
		font-size: 14px;
		color: var(--ca-text-secondary);
	}

	@media (prefers-reduced-motion: reduce) {
		.ready-hero {
			animation: none;
		}
		.hero-bob {
			animation: none;
		}
	}
</style>
