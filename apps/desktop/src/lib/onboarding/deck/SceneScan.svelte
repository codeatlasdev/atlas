<script lang="ts">
	/** Slide "your projects", before the scan: a sweep crosses a grid of folders and the ones with an atlas.yaml light up. */
	import { onMount } from "svelte";
	import { Beats } from "./beats.svelte.ts";

	const COLS = 9;
	const ROWS = 3;
	// The folders that "have" an atlas.yaml, lit in this order as the sweep passes them.
	const FOUND = [3, 11, 22, 7, 16];
	const beats = new Beats([500, 420, 420, 420, 420, 420, 2400]);
	onMount(() => beats.start());
</script>

<div class="scan" aria-hidden="true">
	<div class="grid" style:--cols={COLS}>
		{#each { length: COLS * ROWS } as _, i (i)}
			{@const k = FOUND.indexOf(i)}
			<span class="dir" class:hit={k >= 0 && beats.n > k}>
				<svg viewBox="0 0 24 20"
					><path d="M2 4a2 2 0 0 1 2-2h5l2 2.5h9a2 2 0 0 1 2 2V16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2Z" /></svg
				>
			</span>
		{/each}
	</div>
	<span class="sweep" class:run={beats.n > 0 && beats.n < beats.last}></span>
	<span class="count" class:on={beats.n > 0}>{Math.min(beats.n, FOUND.length)} found</span>
</div>

<style>
	.scan {
		--ca-brand-main: #3d5bff;
		position: relative;
		width: min(440px, 100%);
		height: 138px;
		margin-bottom: 8px;
		overflow: hidden;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(var(--cols), 1fr);
		gap: 10px 8px;
		padding: 6px 4px;
	}
	.dir svg {
		width: 100%;
		height: 26px;
		fill: var(--ca-surface-raised);
		stroke: var(--ca-outline-strong);
		stroke-width: 1;
		transition:
			fill 360ms ease,
			stroke 360ms ease;
	}
	.dir {
		display: block;
		transition: transform 420ms cubic-bezier(0.2, 1.4, 0.3, 1);
	}
	.dir.hit {
		transform: translateY(-3px);
	}
	.dir.hit svg {
		fill: color-mix(in oklab, #3d5bff 26%, var(--ca-surface-raised));
		stroke: #3d5bff;
	}
	/* The scanner: a soft vertical light crossing left to right (transform only). */
	.sweep {
		position: absolute;
		top: 0;
		bottom: 26px;
		left: 0;
		width: 60px;
		background: linear-gradient(
			90deg,
			transparent,
			color-mix(in oklab, #3d5bff 22%, transparent),
			transparent
		);
		transform: translateX(-80px);
		opacity: 0;
	}
	.sweep.run {
		opacity: 1;
		animation: sweep 2.2s linear infinite;
	}
	@keyframes sweep {
		to {
			transform: translateX(480px);
		}
	}
	.count {
		position: absolute;
		left: 50%;
		bottom: 0;
		translate: -50% 0;
		font: 600 11px var(--mono);
		color: var(--ca-brand-main);
		opacity: 0;
		transition: opacity 300ms;
	}
	.count.on {
		opacity: 1;
	}
	@media (prefers-reduced-motion: reduce) {
		.sweep.run {
			animation: none;
			opacity: 0;
		}
	}
</style>
