<script lang="ts">
	/** Slide "worktrees": an SVG branch graph growing, one isolated workspace per branch, merging back. */
	import { onMount } from "svelte";
	import Frame from "./Frame.svelte";
	import { Beats } from "./beats.svelte.ts";

	// 0 main, 1 branch a, 2 branch b, 3 commits, 4 merge a, 5 hold
	const beats = new Beats([600, 600, 600, 700, 800, 2400]);
	onMount(() => beats.start());
</script>

<Frame title="shop · worktrees">
	<svg viewBox="0 0 420 200" aria-hidden="true">
		<defs>
			<linearGradient id="deck-wt" x1="0" x2="1">
				<stop offset="0" stop-color="#7f95ff" />
				<stop offset="1" stop-color="#3d5bff" />
			</linearGradient>
		</defs>
		<path class="ln main" class:on={beats.n >= 0} d="M30 100 H390" pathLength="1" />
		<path class="ln a" class:on={beats.n >= 1} d="M90 100 C120 100 120 50 150 50 H300" pathLength="1" />
		<path class="ln b" class:on={beats.n >= 2} d="M140 100 C170 100 170 150 200 150 H330" pathLength="1" />
		<path class="ln a" class:on={beats.n >= 4} d="M300 50 C330 50 330 100 360 100" pathLength="1" />
		<circle class="node" class:on={beats.n >= 0} cx="30" cy="100" r="6" />
		<circle class="node a" class:on={beats.n >= 3} cx="200" cy="50" r="5" />
		<circle class="node a" class:on={beats.n >= 3} cx="250" cy="50" r="5" />
		<circle class="node b" class:on={beats.n >= 3} cx="250" cy="150" r="5" />
		<circle class="node merge" class:on={beats.n >= 4} cx="360" cy="100" r="7" />
		<g class="tag" class:on={beats.n >= 1} transform="translate(170 22)">
			<rect width="104" height="20" rx="10" /><text x="52" y="14">feat/checkout</text>
		</g>
		<g class="tag b" class:on={beats.n >= 2} transform="translate(214 168)">
			<rect width="104" height="20" rx="10" /><text x="52" y="14">fix/cart-total</text>
		</g>
		<g class="tag ok" class:on={beats.n >= 4} transform="translate(318 66)">
			<rect width="70" height="20" rx="10" /><text x="35" y="14">merged</text>
		</g>
	</svg>
</Frame>

<style>
	svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		padding: 10px;
	}
	.ln {
		fill: none;
		stroke-width: 3;
		stroke-linecap: round;
		stroke-dasharray: 1;
		stroke-dashoffset: 1;
		transition: stroke-dashoffset 700ms cubic-bezier(0.6, 0, 0.2, 1);
	}
	.ln.on {
		stroke-dashoffset: 0;
	}
	.main {
		stroke: var(--ca-text-disabled);
	}
	.ln.a {
		stroke: url(#deck-wt);
	}
	.ln.b {
		stroke: var(--ca-semantic-warning);
	}
	.node {
		fill: var(--ca-surface-raised);
		stroke: var(--ca-text-secondary);
		stroke-width: 2;
		transform-box: fill-box;
		transform-origin: center;
		transform: scale(0);
		transition: transform 380ms cubic-bezier(0.2, 1.4, 0.3, 1);
	}
	.node.on {
		transform: scale(1);
	}
	.node.a {
		stroke: #5b74ff;
	}
	.node.b {
		stroke: var(--ca-semantic-warning);
	}
	.node.merge {
		fill: var(--ca-semantic-success);
		stroke: var(--ca-semantic-success);
	}
	.tag {
		opacity: 0;
		transition: opacity 360ms ease;
	}
	.tag.on {
		opacity: 1;
	}
	.tag rect {
		fill: var(--ca-surface-sunken);
		stroke: var(--ca-outline-default);
	}
	.tag text {
		font: 600 10.5px var(--mono);
		fill: var(--ca-text-primary);
		text-anchor: middle;
	}
	.tag.ok text {
		fill: var(--ca-semantic-success);
	}
	@media (prefers-reduced-motion: reduce) {
		.ln,
		.node,
		.tag {
			transition: none;
		}
	}
</style>
