<script lang="ts">
	/**
	 * Small number box with a unit. Commits on change/blur; an empty or invalid entry
	 * is sent as NaN so the setting falls back to its default.
	 */
	let {
		value,
		unit,
		label,
		onchange,
	}: { value: number; unit?: string; label: string; onchange: (v: number) => void } = $props();

	let text = $derived(String(value));
</script>

<label class="num">
	<input
		inputmode="decimal"
		aria-label={label}
		value={text}
		onchange={(e) => {
			const v = e.currentTarget.value.trim();
			onchange(v === "" ? NaN : Number(v));
		}}
	/>
	{#if unit}<span>{unit}</span>{/if}
</label>

<style>
	.num {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	input {
		width: 64px;
		height: 28px;
		padding: 0 8px;
		text-align: right;
		font: inherit;
		font-size: 12.5px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
	}
	input:focus {
		outline: none;
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, var(--ca-outline-subtle));
	}
</style>
