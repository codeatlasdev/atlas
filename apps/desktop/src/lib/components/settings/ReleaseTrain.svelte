<script lang="ts">
	/**
	 * How a build travels: alpha → beta → stable, three lanes. The lane you follow is lit; lanes upstream of it
	 * feed it (a build is promoted only after it has run in the lane before). Theme-aware: currentColor and
	 * --ca-* tokens only. Informative, so it carries a title and a description.
	 */
	import { laneStates, type Channel } from "#lib/updates.ts";

	let { followed, pending = null }: { followed: Channel; pending?: Channel | null } = $props();

	const LANES: { id: Channel; label: string; y: number }[] = [
		{ id: "alpha", label: "Alpha", y: 14 },
		{ id: "beta", label: "Beta", y: 40 },
		{ id: "stable", label: "Stable", y: 66 },
	];
	let state = $derived(laneStates(followed, pending));
	let focus = $derived(pending ?? followed);
	let description = $derived(
		focus === "stable"
			? "Builds are promoted from alpha to beta to stable. You follow stable, the last lane."
			: focus === "beta"
				? "Builds are promoted from alpha to beta to stable. You follow beta, after alpha and before stable."
				: "Builds are promoted from alpha to beta to stable. You follow alpha, the first lane.",
	);
</script>

<svg
	class="train"
	viewBox="0 0 240 80"
	role="img"
	aria-labelledby="train-title train-desc"
	xmlns="http://www.w3.org/2000/svg"
>
	<title id="train-title">Release train</title>
	<desc id="train-desc">{description}</desc>
	{#each LANES as l, i (l.id)}
		{@const s = state[l.id]}
		<g class="lane {s}">
			<text x="0" y={l.y + 3.5} class="lbl">{l.label}</text>
			<line x1="46" x2="226" y1={l.y} y2={l.y} class="rail" />
			<circle cx="46" cy={l.y} r="3.5" class="stop" />
			<circle cx="226" cy={l.y} r="3.5" class="stop" />
			{#if s === "followed" || s === "pending"}
				<rect x="108" y={l.y - 5} width="64" height="10" rx="5" class="glow" />
				<circle cx="140" cy={l.y} r="3.5" class="build" />
			{/if}
		</g>
		{#if i < LANES.length - 1}
			<!-- promote: a build leaves the lane's end and joins the next lane's start -->
			<path
				class="promote {state[LANES[i + 1]!.id] === 'idle' ? 'dim' : ''}"
				d="M226 {l.y + 3.5} C 236 {l.y + 10}, 36 {l.y + 16}, 46 {LANES[i + 1]!.y - 3.5}"
			/>
		{/if}
	{/each}
</svg>

<style>
	.train {
		display: block;
		width: 100%;
		max-width: 260px;
		height: auto;
		color: var(--ca-text-secondary);
		overflow: visible;
	}
	.lbl {
		font-size: 9.5px;
		font-weight: 600;
		fill: currentColor;
	}
	.rail {
		stroke: currentColor;
		stroke-opacity: 0.28;
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-dasharray: 2 4;
	}
	.stop {
		fill: var(--ca-surface-raised);
		stroke: currentColor;
		stroke-opacity: 0.45;
		stroke-width: 1.25;
	}
	.promote {
		fill: none;
		stroke: currentColor;
		stroke-opacity: 0.3;
		stroke-width: 1;
		stroke-linecap: round;
	}
	.promote.dim {
		stroke-opacity: 0.12;
	}
	.glow {
		fill: var(--ca-brand-main);
		fill-opacity: 0.16;
	}
	.build {
		fill: var(--ca-brand-main);
	}
	.lane.followed .lbl,
	.lane.pending .lbl {
		fill: var(--ca-text-primary);
	}
	.lane.followed .rail,
	.lane.pending .rail {
		stroke: var(--ca-brand-main);
		stroke-opacity: 0.85;
		stroke-dasharray: none;
	}
	.lane.followed .stop,
	.lane.pending .stop {
		stroke: var(--ca-brand-main);
		stroke-opacity: 1;
	}
	.lane.pending .rail {
		stroke-dasharray: 5 3;
	}
	.lane.feeds .rail {
		stroke-opacity: 0.5;
		stroke-dasharray: none;
	}
	.lane.idle {
		opacity: 0.55;
	}
</style>
