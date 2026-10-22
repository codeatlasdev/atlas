<script lang="ts">
	/** open=true → screen raised; open=false → screen tilted closed */
	let { size = 80, open = true }: { size?: number; open?: boolean } = $props();

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
	</g>
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	.screen {
		transition: transform 600ms cubic-bezier(0.4, 0, 0.2, 1);
	}
	.screen.closed {
		transform: rotate(78deg);
	}

	@media (prefers-reduced-motion: reduce) {
		.screen {
			transition: none;
		}
	}
</style>
