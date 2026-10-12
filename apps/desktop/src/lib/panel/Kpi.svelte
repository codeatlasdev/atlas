<script lang="ts">
	import type { Component } from "svelte";

	/** Compact metric tile: a number with its unit window, tinted when it needs attention. */
	let {
		icon: Icon,
		label,
		value,
		hint,
		tone = "off",
	}: {
		icon: Component<{ size?: number }>;
		label: string;
		value: string;
		hint: string;
		tone?: "ok" | "warn" | "fail" | "off";
	} = $props();
</script>

<div class="kpi t-{tone}">
	<div class="head"><Icon size={13} /><span>{label}</span></div>
	<div class="value">{value}</div>
	<div class="hint">{hint}</div>
</div>

<style>
	.kpi {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 10px 12px;
		border-radius: 12px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		min-width: 0;
		--c: var(--ca-text-secondary);
		transition:
			background 200ms ease-out,
			box-shadow 200ms ease-out;
	}
	.t-ok {
		--c: var(--ca-semantic-success);
	}
	.t-warn {
		--c: var(--ca-semantic-warning);
	}
	.t-fail {
		--c: var(--ca-semantic-error);
	}
	.t-warn,
	.t-fail {
		background: color-mix(in oklab, var(--c) 8%, var(--ca-surface-raised));
		box-shadow: 0 0 0 0.5px color-mix(in oklab, var(--c) 40%, transparent);
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.head :global(svg) {
		color: var(--c);
	}
	.value {
		font-size: 22px;
		font-weight: 650;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-primary);
		white-space: nowrap;
	}
	.hint {
		font-size: 11px;
		color: var(--ca-text-disabled);
	}
</style>
