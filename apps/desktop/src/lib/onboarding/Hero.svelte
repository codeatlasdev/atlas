<script lang="ts">
	/**
	 * The standing empty state (no project yet, welcome already done): the Atlas globe as a night sky, with the project
	 * steps (Phases) under it. The first run is the deck (deck/Deck.svelte), not this.
	 */
	import { ACTIONS, testId } from "./flow.ts";
	import { copyScrim, STAGE_HEX, STAGE_LAYOUT, STAGE_RGB } from "./legibility.ts";
	import Globe from "./intro/Globe.svelte";
	import Phases from "./Phases.svelte";
	import { welcome as w } from "./welcome.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { cssVars } from "#lib/theme.ts";

	// The stage is always a night sky, whatever the app theme: everything on it (copy, buttons, chips) uses the
	// user's DARK theme of the same family, so light themes stay readable here (2026-10-13 light-theme incident).
	const darkVars = $derived(
		themes.currentDark
			? Object.entries(cssVars(themes.currentDark.style))
					.map(([k, v]) => `${k}:${v}`)
					.join(";")
			: "",
	);

	let { onfinish, onskip }: { onfinish: () => void; onskip?: () => void } = $props();

	/**
	 * The scrim under the copy. Its strength comes from the brightest pixel the globe can put behind the subtitle, measured
	 * on a real shot of the un-dimmed globe ([125, 143, 220] on the tiles' light side), so it holds even if the globe is
	 * ever moved back under the text; the subtitle colour is --ca-text-secondary on the dark stage.
	 */
	const scrim = copyScrim({ worstBackdrop: [125, 143, 220], subtitle: [166, 168, 171] });

	let globe: ReturnType<typeof Globe> | undefined = $state();
	let mode = $derived<"progress" | "calm">(w.phase === "welcome" ? "calm" : "progress");

	$effect(() => {
		w.clock = () => globe?.now?.() ?? 0;
	});
</script>

<div
	class="hero dark"
	style={darkVars}
	data-contrast-bg={STAGE_HEX}
	data-phase={w.phase}
	data-testid={testId(w.phase)}
	role="presentation"
>
	<Globe bind:this={globe} {mode} lit={w.summary.projects} doneAt={w.upAt} />

	<div
		class="scrim"
		style:--scrim={scrim.alpha}
		style:--inset="{STAGE_LAYOUT.copyBottomInset}px"
		style:--stage-rgb={STAGE_RGB.join(", ")}
		aria-hidden="true"
	></div>
	{#key w.phase}
		<div class="stage" style:--inset="{STAGE_LAYOUT.copyBottomInset}px">
			<Phases {onfinish} />
		</div>
	{/key}

	{#if onskip && w.phase !== "up"}
		<button class="skip" data-testid={ACTIONS.skip} onclick={onskip}>Skip for now</button>
	{/if}
</div>

<style>
	.hero {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		overflow: hidden;
		isolation: isolate;
		/* Inherited text (the h1) must come from the dark vars set on this element, not the app body. */
		color: var(--ca-text-primary);
	}
	/* A soft pool of the stage colour under the copy: opaque enough for AA at its centre, feathered so it never reads as a card. */
	.scrim {
		position: absolute;
		z-index: 0;
		left: 50%;
		bottom: calc(var(--inset) - 90px);
		width: min(640px, 92%);
		height: 420px;
		translate: -50% 0;
		pointer-events: none;
		background: radial-gradient(
			ellipse 50% 50% at 50% 50%,
			rgba(var(--stage-rgb), var(--scrim)) 0%,
			rgba(var(--stage-rgb), calc(var(--scrim) * 0.9)) 55%,
			rgba(var(--stage-rgb), 0) 100%
		);
	}
	/* The copy block sits at the bottom of the hero so the globe above it never meets a letter (legibility.ts). Each
	   phase re-lands: the copy changes, the sky does not. `translate` composes with the entrance's `transform`. */
	.stage {
		position: absolute;
		left: 50%;
		bottom: var(--inset);
		translate: -50% 0;
		z-index: 1;
		width: min(460px, calc(100% - 56px));
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 10px;
		animation: land 520ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}
	.skip {
		position: absolute;
		right: 20px;
		bottom: 18px;
		z-index: 2;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 5px 8px;
		border-radius: 7px;
	}
	.skip:hover {
		background: var(--ca-action-hover);
	}
	@keyframes land {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.stage {
			animation: none;
		}
	}
</style>
