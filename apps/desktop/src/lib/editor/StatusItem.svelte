<script lang="ts">
	/**
	 * The one box of every item in the editor status bar (position, problems, indent, encoding, line endings, language,
	 * language server): a text item is a `<span>`, an action a `<button>`, and both get the same height, padding, font and
	 * line box, so their text sits on one baseline whatever the element (#504). Nothing in the bar draws an item with its
	 * own element and style; a new item uses this. `statusBaseline.test.ts` pins the contract.
	 *
	 * Rest props go to the element, so a kit trigger can spread its props on it (`<StatusItem button {...props}>`).
	 */
	import type { Snippet } from "svelte";
	import type { HTMLAttributes } from "svelte/elements";

	let {
		button = false,
		tone,
		children,
		...rest
	}: {
		/** Clickable: a `<button>`; otherwise a plain `<span>`. */
		button?: boolean;
		tone?: "warning" | "error";
		children: Snippet;
	} & HTMLAttributes<HTMLElement> = $props();
</script>

{#if button}
	<button type="button" class="status-item" data-tone={tone} {...rest}>{@render children()}</button>
{:else}
	<span class="status-item" data-tone={tone} {...rest}>{@render children()}</span>
{/if}

<style>
	.status-item {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		flex: 0 1 auto;
		min-width: 0;
		box-sizing: border-box;
		height: 18px;
		margin: 0;
		padding: 0 6px;
		border: 0;
		border-radius: 4px;
		background: none;
		/* `font` is a shorthand: it resets `line-height`, so it must come BEFORE the line-height below. With the order
		   reversed every item fell back to the bar's `line-height: 1` (11.5px) in an 18px box, and a <span> (text at
		   the top of its box) sat 3px above a <button> (text centred by the browser). Equal line box and height make
		   the two elements agree. */
		font: inherit;
		line-height: 18px;
		color: inherit;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	button.status-item:hover,
	.status-item[data-state="open"] {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.status-item[data-tone="warning"] {
		color: var(--ca-semantic-warning);
	}
	.status-item[data-tone="error"] {
		color: var(--ca-semantic-error);
	}
</style>
