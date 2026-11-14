<script lang="ts">
	/**
	 * CrewTyping — shared "dots gather → crewmate hops → Thinking" animation.
	 *
	 * Used in the chat (setup "Thinking", task "Working · 12s") and the onboarding art (CrewArtHow).
	 * The dots use the crewmate's own colour from normalizeLook; the shimmer label uses
	 * theme tokens so it reads in any theme.
	 */
	import { fly } from "svelte/transition";
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import { normalizeLook, type CrewmateLook } from "#lib/crew/look.ts";

	let {
		seed,
		look,
		label = "Thinking",
		detail,
		paused = false,
	}: {
		seed: string;
		look?: CrewmateLook;
		/** What it is doing; a change slides the new words in ("Thinking" → "Reading app.ts" → "Planning"…). */
		label?: string;
		/** Small fixed text after the label (elapsed time); does not animate with it. */
		detail?: string;
		paused?: boolean;
	} = $props();

	const crewColor = $derived(normalizeLook(look, seed).color);
</script>

<div class="crewmate-typing" class:paused>
	<span class="mark">
		<span class="dots" style:--crew={crewColor}><i></i><i></i><i></i></span>
		<span class="mini"><CrewmateFace {seed} {look} state="working" size={22} /></span>
	</span>
	<span class="label-slot">
		{#key label}
			<span class="crew-shimmer shimmer" in:fly={{ y: 7, duration: 260 }}>{label}</span>
		{/key}
	</span>
	{#if detail}<span class="detail">· {detail}</span>{/if}
</div>

<style>
	.crewmate-typing {
		display: flex;
		align-items: center;
		gap: 0.55em;
		font-size: 0.875em;
		min-height: 2.1em;
	}

	/* ── mark: dots and mini face share the same grid cell ────── */
	.mark {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: 2.4em;
		height: 1.6em;
	}
	.mark > * {
		grid-area: 1 / 1;
	}

	/* ── dots: bounce then gather into one point ──────────────── */
	.mark .dots i {
		opacity: 1;
		animation: 2.6s cubic-bezier(0.45, 0, 0.2, 1) infinite;
	}
	.mark .dots i:nth-child(1) {
		--gx: 0.75em;
		animation-name: gather;
		animation-delay: 0s;
	}
	.mark .dots i:nth-child(2) {
		--gx: 0em;
		animation-name: gather;
		animation-delay: 0s;
		--lift: -0.22em;
	}
	.mark .dots i:nth-child(3) {
		--gx: -0.75em;
		animation-name: gather;
		animation-delay: 0s;
	}
	@keyframes gather {
		0%,
		100% {
			transform: none;
		}
		10% {
			transform: translateY(-0.22em);
		}
		20% {
			transform: none;
		}
		30% {
			transform: translateY(var(--lift, 0));
		}
		38% {
			transform: none;
		}
		46% {
			opacity: 1;
			transform: translateX(var(--gx));
		}
		48%,
		88% {
			opacity: 0;
			transform: translateX(var(--gx));
		}
		95% {
			opacity: 1;
			transform: none;
		}
	}

	/* ── mini face: pops out of the gathered point, hops, shrinks back ── */
	.mark .mini {
		display: inline-flex;
		transform: scale(0);
		animation: crew-hop 2.6s cubic-bezier(0.45, 0, 0.2, 1) infinite;
	}
	@keyframes crew-hop {
		0%,
		44% {
			transform: scale(0);
		}
		50% {
			transform: scale(1.3, 0.75);
		}
		56% {
			transform: translateY(-0.35em) scale(0.85, 1.15);
		}
		62% {
			transform: scale(1.1, 0.9);
		}
		68% {
			transform: rotate(-14deg);
		}
		74% {
			transform: rotate(12deg);
		}
		80% {
			transform: rotate(-6deg);
		}
		85% {
			transform: none;
		}
		90%,
		100% {
			transform: scale(0);
		}
	}

	/* ── dots base styles ─────────────────────────────────────── */
	.dots {
		display: inline-flex;
		gap: 0.3em;
		padding: 0.25em 0;
	}
	.dots i {
		width: 0.55em;
		height: 0.55em;
		border-radius: 50%;
		background: var(--crew, currentColor);
		opacity: 1;
		animation: dot 1.2s ease-in-out infinite;
	}
	.dots i:nth-child(2) {
		animation-delay: 0.15s;
	}
	.dots i:nth-child(3) {
		animation-delay: 0.3s;
	}
	@keyframes dot {
		30% {
			opacity: 1;
			transform: translateY(-0.2em);
		}
	}

	/* The label swaps in place: one grid cell, so the outgoing and incoming words never push the row around. */
	.label-slot {
		display: inline-grid;
	}
	.label-slot > :global(*) {
		grid-area: 1 / 1;
	}
	.detail {
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}

	/* ── shimmer label: gradient and keyframes are the shared .crew-shimmer ── */
	.shimmer {
		--crew-shimmer-duration: 1.8s;
		--crew-shimmer-timing: linear;
		font-size: 13px;
		font-weight: 500;
	}

	/* ── pause support (visibility change) ───────────────────── */
	.paused .shimmer,
	.paused .mark .dots i,
	.paused .mark .mini {
		animation-play-state: paused;
	}

	/* ── reduced motion ───────────────────────────────────────── */
	@media (prefers-reduced-motion: reduce) {
		.mark .dots i,
		.mark .mini {
			animation: none;
		}
		.mark .mini {
			opacity: 0;
		}
	}
</style>
