<script lang="ts">
	/** Number of drive bays visible */
	let { size = 100, bays = 5 }: { size?: number; bays?: number } = $props();

	const h = $derived((size * 140) / 100);
	const w = $derived(size);
</script>

<!-- Server rack with blinking LEDs -->
<svg viewBox="0 0 100 140" width={w} height={h} aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
	<!-- Rack chassis -->
	<rect x="4" y="4" width="92" height="132" rx="6" fill="currentColor" opacity=".12" />
	<rect
		x="4"
		y="4"
		width="92"
		height="132"
		rx="6"
		stroke="currentColor"
		stroke-width="2"
		fill="none"
		opacity=".35"
	/>

	<!-- Drive bays -->
	{#each Array(bays) as _, i}
		{@const y = 12 + i * 24}
		<rect x="10" {y} width="80" height="18" rx="3" fill="currentColor" opacity=".18" />
		<rect
			x="10"
			{y}
			width="80"
			height="18"
			rx="3"
			stroke="currentColor"
			stroke-width="1"
			fill="none"
			opacity=".25"
		/>
		<!-- Drive label slot -->
		<rect x="14" y={y + 4} width="52" height="10" rx="2" fill="currentColor" opacity=".1" />
		<!-- Power LED — each blinks at a different phase -->
		<circle class="led led-{i}" cx="76" cy={y + 9} r="4" fill="#22c55e" />
		<!-- Activity LED -->
		<circle class="led-act led-act-{i}" cx="86" cy={y + 9} r="3" fill="#f97316" opacity="0" />
	{/each}

	<!-- Bottom ventilation slots -->
	{#each [0, 1, 2, 3, 4] as j}
		<rect x={14 + j * 15} y="126" width="10" height="4" rx="2" fill="currentColor" opacity=".2" />
	{/each}
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}

	/* Power LEDs: each has a slightly different on/off blink cycle */
	.led {
		animation: blink-on 2s ease-in-out infinite;
	}
	.led-1 {
		animation-delay: 0.4s;
	}
	.led-2 {
		animation-delay: 0.9s;
	}
	.led-3 {
		animation-delay: 1.5s;
	}
	.led-4 {
		animation-delay: 0.2s;
	}

	@keyframes blink-on {
		0%,
		40%,
		60%,
		100% {
			opacity: 1;
		}
		48%,
		52% {
			opacity: 0.15;
		}
	}

	/* Activity LEDs: quick orange flashes at irregular intervals */
	.led-act {
		animation: blink-act 3.2s ease-in-out infinite;
	}
	.led-act-0 {
		animation-delay: 0.7s;
	}
	.led-act-1 {
		animation-delay: 1.8s;
	}
	.led-act-2 {
		animation-delay: 0.3s;
	}
	.led-act-3 {
		animation-delay: 2.4s;
	}
	.led-act-4 {
		animation-delay: 1.1s;
	}

	@keyframes blink-act {
		0%,
		12%,
		100% {
			opacity: 0;
		}
		4%,
		8% {
			opacity: 0.85;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.led,
		.led-act {
			animation: none;
		}
		.led {
			opacity: 1;
		}
		.led-act {
			opacity: 0;
		}
	}
</style>
