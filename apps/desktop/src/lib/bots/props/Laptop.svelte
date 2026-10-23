<script lang="ts">
	/** open=true → screen raised; open=false → screen tilted closed */
	let { size = 80, open = true, glow = false }: { size?: number; open?: boolean; glow?: boolean } = $props();

	const h = $derived((size * 60) / 80);
</script>

<!-- Laptop side-view: screen + keyboard base -->
<svg viewBox="0 0 80 60" width={size} height={h} aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
	<!-- Keyboard base -->
	<rect x="4" y="44" width="72" height="14" rx="4" fill="currentColor" opacity=".8" />
	<!-- Trackpad -->
	<rect x="28" y="47" width="24" height="8" rx="3" fill="currentColor" opacity=".35" />
	<!-- Key rows hint -->
	{#each [0, 1, 2] as row}
		<rect
			x={10 + row * 1}
			y={46 + row * 2}
			width={56 - row * 2}
			height="1.5"
			rx=".75"
			fill="currentColor"
			opacity=".2"
		/>
	{/each}

	<!-- Screen — rotates around hinge at (40, 44) -->
	<g class="screen" class:closed={!open} style="transform-origin: 40px 44px;">
		<rect x="8" y="6" width="64" height="40" rx="4" fill="currentColor" opacity=".85" />
		<!-- Bezel inner -->
		<rect x="12" y="10" width="56" height="32" rx="2.5" fill="currentColor" opacity=".45" />
		<!-- Screen glow -->
		<rect x="14" y="12" width="52" height="28" rx="2" fill="currentColor" opacity=".12" />
		<!-- Menu-bar hint -->
		<rect x="14" y="12" width="52" height="5" rx="1" fill="currentColor" opacity=".22" />
		<!-- Camera dot -->
		<circle cx="40" cy="8" r="1.5" fill="currentColor" opacity=".4" />
		<!-- Typing shimmer lines (glow mode) -->
		{#if glow}
			<rect class="shimmer s1" x="18" y="21" width="36" height="2" rx="1" fill="currentColor" />
			<rect class="shimmer s2" x="18" y="26" width="28" height="2" rx="1" fill="currentColor" />
			<rect class="shimmer s3" x="18" y="31" width="32" height="2" rx="1" fill="currentColor" />
		{/if}
	</g>
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	.screen {
		transition:
			transform 800ms cubic-bezier(0.4, 0, 0.2, 1),
			opacity 800ms ease;
	}
	.screen.closed {
		transform: rotate(82deg);
		opacity: 0.5;
	}

	@media (prefers-reduced-motion: reduce) {
		.screen {
			transition: none;
		}
	}

	.shimmer {
		opacity: 0;
		animation: shimmer-pulse 2.4s ease-in-out infinite;
	}
	.shimmer.s1 {
		animation-delay: 0s;
	}
	.shimmer.s2 {
		animation-delay: 0.6s;
	}
	.shimmer.s3 {
		animation-delay: 1.2s;
	}
	@keyframes shimmer-pulse {
		0%,
		100% {
			opacity: 0.08;
		}
		50% {
			opacity: 0.45;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.shimmer {
			animation: none;
			opacity: 0.2;
		}
	}
</style>
