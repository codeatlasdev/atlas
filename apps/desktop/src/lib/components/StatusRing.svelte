<script lang="ts">
	import { Spring } from "svelte/motion";

	let { state }: { state: string } = $props();

	const R = 11;
	const C = 2 * Math.PI * R;
	const FILL: Record<string, number> = { running: 1, failed: 1, starting: 0.35, suspended: 0.6 };

	const fill = new Spring(0, { stiffness: 0.12, damping: 0.5 });
	$effect(() => {
		fill.target = FILL[state] ?? 0;
	});
</script>

<svg class="ring s-{state}" width="28" height="28" viewBox="0 0 28 28" aria-label={state}>
	<circle cx="14" cy="14" r={R} class="track" />
	<circle cx="14" cy="14" r={R} class="arc" stroke-dasharray={C} stroke-dashoffset={C * (1 - fill.current)} />
	<circle cx="14" cy="14" r="4" class="core" />
</svg>

<style>
	.ring {
		flex-shrink: 0;
		transform: rotate(-90deg);
		--c: var(--ca-text-disabled);
	}
	.s-running {
		--c: var(--ca-semantic-success);
	}
	.s-starting {
		--c: var(--ca-semantic-warning);
	}
	.s-failed {
		--c: var(--ca-semantic-error);
	}
	.s-suspended {
		--c: var(--ca-semantic-info);
	}
	.track {
		fill: none;
		stroke: var(--ca-outline-subtle);
		stroke-width: 2.5;
	}
	.arc {
		fill: none;
		stroke: var(--c);
		stroke-width: 2.5;
		stroke-linecap: round;
		transition: stroke 200ms ease-out;
	}
	.s-starting .arc {
		animation: spin 1s linear infinite;
		transform-origin: 14px 14px;
	}
	.core {
		fill: var(--c);
		transition: fill 200ms ease-out;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
