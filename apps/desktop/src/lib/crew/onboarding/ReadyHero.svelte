<!--
  Ready step panel: the first crewmate online on the server, with its first chat bubble. The face hops, the rack
  lights are green, the bubble types in once. Transform and opacity only.
-->
<script lang="ts">
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import { agentLogo } from "#lib/agentLogos.ts";
	import { reducedMotion } from "#lib/crew/motion.ts";
	import type { CrewmateLook } from "#lib/crew/look.ts";
	import ServerRack from "./ServerRack.svelte";

	let { engine, look }: { engine: string; look: CrewmateLook } = $props();

	const logo = $derived(agentLogo(engine || "claude"));
</script>

<div class="ready-hero" class:still={reducedMotion} data-testid="ready-scene">
	<div class="bubble">
		<span class="dots"><i></i><i></i><i></i></span>
		<span class="msg">Hi! What should we work on first?</span>
	</div>
	<div class="who">
		<div class="face-wrap">
			<div class="bob"><CrewmateFace seed="onb-preview" {look} state="idle" size={132} /></div>
			{#if logo}<span class="engine">{@html logo.dark ?? logo.light}</span>{/if}
			<span class="online" title="Online"></span>
		</div>
		<svg class="rack" viewBox="0 0 150 176" aria-hidden="true"><ServerRack state="ok" /></svg>
	</div>
	<p class="cap"><b>My First Crewmate</b> is online on your server.</p>
</div>

<style>
	.ready-hero {
		width: min(400px, 90%);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 18px;
		animation: ob-rise 520ms cubic-bezier(0.34, 1.4, 0.64, 1) both;
	}

	.bubble {
		position: relative;
		align-self: flex-end;
		max-width: 80%;
		padding: 12px 16px;
		border-radius: 16px 16px 4px 16px;
		background: color-mix(in srgb, var(--ob-accent, var(--ca-accent)) 20%, var(--ca-surface-base));
		border: 1.5px solid var(--ob-accent, var(--ca-accent));
		color: var(--ca-text-primary);
		font-size: 14px;
		animation: ob-rise 500ms 500ms cubic-bezier(0.34, 1.4, 0.64, 1) both;
	}

	.dots {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 5px;
		background: inherit;
		border-radius: inherit;
		animation: ob-fade 1.6s 0.5s ease forwards;
	}

	.dots i {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ob-accent, var(--ca-accent));
		animation: ob-dot 700ms ease-in-out infinite;
	}

	.dots i:nth-child(2) {
		animation-delay: 120ms;
	}

	.dots i:nth-child(3) {
		animation-delay: 240ms;
	}

	.who {
		display: flex;
		align-items: flex-end;
		justify-content: center;
		gap: 28px;
	}

	.face-wrap {
		position: relative;
	}

	.bob {
		animation: ob-hop 3.6s ease-in-out infinite;
	}

	.engine {
		position: absolute;
		right: -8px;
		bottom: 4px;
		width: 34px;
		height: 34px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ob-line-idle, var(--ca-border-subtle));
	}

	.engine :global(svg) {
		width: 20px;
		height: 20px;
	}

	.online {
		position: absolute;
		left: 6px;
		top: 6px;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--ob-ok, var(--ca-semantic-success));
		border: 2px solid var(--ca-surface-sunken);
	}

	.rack {
		width: 92px;
		flex-shrink: 0;
	}

	.cap {
		margin: 0;
		font-size: 14px;
		color: var(--ca-text-secondary);
	}

	.cap b {
		color: var(--ca-text-primary);
	}

	@keyframes ob-rise {
		from {
			opacity: 0;
			transform: translateY(18px) scale(0.94);
		}
	}

	@keyframes ob-fade {
		to {
			opacity: 0;
			visibility: hidden;
		}
	}

	@keyframes ob-dot {
		50% {
			transform: translateY(-4px);
			opacity: 0.5;
		}
	}

	@keyframes ob-hop {
		0%,
		85%,
		100% {
			transform: none;
		}
		92% {
			transform: translateY(-10px);
		}
	}

	.still *,
	.still {
		animation: none !important;
	}
</style>
