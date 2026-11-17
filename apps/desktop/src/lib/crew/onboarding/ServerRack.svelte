<svelte:options namespace="svg" />

<!--
  A server rack drawn as SVG (150 x 176 in its own coordinates), shared by the connect, engine and ready art: three
  units with rack lights that blink while `state` is "active", settle green when "ok" and go red when "failed".
  Only transform and opacity animate; `reducedMotion` leaves the lights still.
-->
<script lang="ts">
	let { state = "idle" }: { state?: "idle" | "active" | "ok" | "failed" } = $props();
</script>

<g class="rack {state}">
	{#each [0, 1, 2] as i (i)}
		<g transform="translate(0 {i * 60})">
			<rect class="unit" x="0" y="0" width="150" height="52" rx="9" />
			<path class="vent" d="M70 14h66M70 22h66M70 30h66M70 38h66" />
			{#each [0, 1, 2] as j (j)}
				<circle class="led l{(i + j) % 3}" cx={16 + j * 14} cy="26" r="4" />
			{/each}
		</g>
	{/each}
</g>

<style>
	.unit {
		fill: var(--ob-panel, var(--ca-surface-base));
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 2;
		transition: stroke 300ms;
	}

	.vent {
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 1.5;
		stroke-linecap: round;
		opacity: 0.8;
	}

	.led {
		fill: var(--ob-line-idle, var(--ca-border-subtle));
		transition: fill 300ms;
	}

	.active .unit {
		stroke: var(--ob-accent, var(--ca-accent));
	}

	.active .led {
		fill: var(--ob-accent, var(--ca-accent));
		animation: ob-led 1100ms ease-in-out infinite;
	}

	.active .led.l1 {
		animation-delay: 220ms;
	}

	.active .led.l2 {
		animation-delay: 560ms;
	}

	.ok .unit {
		stroke: var(--ob-ok, var(--ca-semantic-success));
	}

	.ok .led {
		fill: var(--ob-ok, var(--ca-semantic-success));
	}

	.failed .unit {
		stroke: var(--ob-danger, var(--ca-danger));
	}

	.failed .led {
		fill: var(--ob-danger, var(--ca-danger));
	}

	@keyframes ob-led {
		50% {
			opacity: 0.25;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.led {
			animation: none !important;
		}
	}
</style>
