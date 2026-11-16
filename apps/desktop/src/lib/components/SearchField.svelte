<script lang="ts">
	/** Rounded search input with a leading icon and an optional clear button (sidebar, settings, shortcuts). */
	import { Search, X } from "@lucide/svelte";
	import type { HTMLInputAttributes } from "svelte/elements";

	let {
		value = $bindable(""),
		compact = false,
		clearable = false,
		label,
		onkeydown,
		...rest
	}: {
		value?: string;
		/** Sidebar density: 26px tall, 12px text, tinted focus fill. */
		compact?: boolean;
		/** Show an ✕ that empties the field. */
		clearable?: boolean;
		/** Accessible name of the clear button. */
		label?: string;
		onkeydown?: (e: KeyboardEvent & { currentTarget: HTMLInputElement }) => void;
	} & Omit<HTMLInputAttributes, "value" | "onkeydown"> = $props();
</script>

<label class="search" class:compact>
	<Search size={12} />
	<!-- The placeholder is not a name: a field with only one reads as unlabeled; an explicit aria-label wins. -->
	<input aria-label={rest.placeholder} bind:value {onkeydown} {...rest} />
	{#if clearable && value}
		<button class="clear" aria-label={label ?? "Clear search"} onclick={() => (value = "")}>
			<X size={compact ? 11 : 12} />
		</button>
	{/if}
</label>

<style>
	.search {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 8px;
		border-radius: 7px;
		border: 0.5px solid var(--atlas-control-border, var(--ca-outline-default));
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		color: var(--ca-text-secondary);
	}
	.search:focus-within {
		border-color: color-mix(
			in oklab,
			var(--ca-brand-main) 55%,
			var(--atlas-control-border, var(--ca-outline-default))
		);
	}
	.search input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12.5px;
		padding: 0;
		border: 0;
		outline: none;
		background: transparent;
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.clear {
		display: inline-grid;
		place-items: center;
		border: 0;
		padding: 0;
		background: none;
		color: var(--ca-text-disabled);
	}
	.clear:hover {
		color: var(--ca-text-primary);
	}
	.compact {
		height: 26px;
		padding: 0 6px 0 8px;
		transition:
			border-color 120ms ease-out,
			background 120ms ease-out;
	}
	.compact:focus-within {
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.compact input {
		font-size: 12px;
	}
	.compact .clear {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: color-mix(in oklab, var(--ca-text-primary) 14%, transparent);
		color: var(--ca-text-secondary);
	}
	.compact .clear:hover {
		color: var(--ca-text-primary);
	}
</style>
