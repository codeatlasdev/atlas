<script lang="ts">
	let {
		value,
		label,
		placeholder,
		mono = false,
		width = "240px",
		onchange,
	}: {
		value: string;
		label: string;
		placeholder?: string;
		mono?: boolean;
		width?: string;
		onchange: (v: string) => void;
	} = $props();
</script>

<input
	class:mono
	style:width
	aria-label={label}
	{placeholder}
	{value}
	spellcheck="false"
	onchange={(e) => onchange(e.currentTarget.value)}
/>

<style>
	input {
		max-width: 100%;
		height: 28px;
		padding: 0 8px;
		font: inherit;
		font-size: 12.5px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
	}
	.mono {
		font-family: ui-monospace, monospace;
		font-size: 12px;
	}
	input:focus {
		outline: none;
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, var(--ca-outline-subtle));
	}
</style>
