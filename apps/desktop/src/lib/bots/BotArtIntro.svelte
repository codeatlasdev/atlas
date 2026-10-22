<!--
  BotArtIntro — step 1 illustration.
  The Atlas mark tiles fade in staggered, then a soft glow pulses.
  Pure CSS animation; respects prefers-reduced-motion.
-->
<script lang="ts">
	import { MARK, MARK_GRADIENT, MARK_VIEW } from "#lib/onboarding/intro/mark.ts";

	// Gradient id must be unique per page instance to avoid SVG id collisions
	const gradId = `art-intro-g-${Math.random().toString(36).slice(2, 8)}`;
</script>

<div class="art-intro" aria-hidden="true">
	<div class="mark-wrap">
		<svg
			class="mark"
			viewBox="0 0 {MARK_VIEW.w} {MARK_VIEW.h}"
			xmlns="http://www.w3.org/2000/svg"
			role="img"
			aria-label="Atlas mark"
		>
			<defs>
				<linearGradient
					id={gradId}
					gradientUnits="userSpaceOnUse"
					x1={MARK_GRADIENT.x1}
					y1={MARK_GRADIENT.y1}
					x2={MARK_GRADIENT.x2}
					y2={MARK_GRADIENT.y2}
				>
					{#each MARK_GRADIENT.stops as s}
						<stop offset={s.offset} stop-color={s.color} />
					{/each}
				</linearGradient>
			</defs>
			{#each MARK as tile}
				<path d={tile.d} fill="url(#{gradId})" class="tile" style:animation-delay="{tile.id * 90}ms" />
			{/each}
		</svg>
	</div>
</div>

<style>
	.art-intro {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		height: 100%;
	}

	.mark-wrap {
		animation: glow-pulse 3.6s ease-in-out infinite;
		animation-delay: 1s;
	}

	.mark {
		width: 220px;
		height: 220px;
		display: block;
	}

	.tile {
		opacity: 0;
		animation: tile-in 480ms both;
	}

	@keyframes tile-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	@keyframes glow-pulse {
		0%,
		100% {
			filter: drop-shadow(0 0 14px rgba(61, 91, 255, 0.25));
		}
		50% {
			filter: drop-shadow(0 0 32px rgba(61, 91, 255, 0.5));
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.tile {
			animation: none;
			opacity: 1;
		}
		.mark-wrap {
			animation: none;
		}
	}
</style>
