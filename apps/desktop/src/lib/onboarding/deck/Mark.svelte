<script lang="ts">
	/**
	 * The Atlas mark assembling: its real tiles (mark.ts, generated from assets/logo.svg) drop in from slightly above, in
	 * order from the centre out, then the whole mark settles with a soft glow. One SVG, transforms and opacity only.
	 */
	import { MARK, MARK_GRADIENT, MARK_VIEW } from "../intro/mark.ts";

	let { size = 96 }: { size?: number } = $props();

	const cx = MARK_VIEW.w / 2;
	const cy = MARK_VIEW.h / 2;
	// Centre first: the delay grows with each tile's distance from the middle of the mark.
	const far = Math.max(...MARK.map((t) => Math.hypot(t.cx - cx, t.cy - cy)));
	const tiles = MARK.map((t) => ({
		...t,
		delay: Math.round((Math.hypot(t.cx - cx, t.cy - cy) / far) * 520),
	}));
</script>

<svg class="mark" width={size} height={size} viewBox="0 0 {MARK_VIEW.w} {MARK_VIEW.h}" aria-hidden="true">
	<defs>
		<linearGradient
			id="deck-mark"
			gradientUnits="userSpaceOnUse"
			x1={MARK_GRADIENT.x1}
			y1={MARK_GRADIENT.y1}
			x2={MARK_GRADIENT.x2}
			y2={MARK_GRADIENT.y2}
		>
			{#each MARK_GRADIENT.stops as s (s.offset)}<stop offset={s.offset} stop-color={s.color} />{/each}
		</linearGradient>
	</defs>
	<g fill="url(#deck-mark)">
		{#each tiles as t (t.id)}<path d={t.d} style:animation-delay="{t.delay}ms" />{/each}
	</g>
</svg>

<style>
	.mark {
		overflow: visible;
		filter: drop-shadow(0 10px 28px color-mix(in oklab, #3d5bff 45%, transparent));
		animation: settle 900ms 500ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}
	path {
		transform-box: fill-box;
		transform-origin: center;
		animation: tile 620ms cubic-bezier(0.2, 1.2, 0.3, 1) both;
	}
	@keyframes tile {
		from {
			opacity: 0;
			transform: translateY(-14px) scale(0.6);
		}
	}
	@keyframes settle {
		from {
			scale: 0.94;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.mark,
		path {
			animation: none;
		}
	}
</style>
