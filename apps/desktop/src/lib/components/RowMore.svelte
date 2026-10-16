<script lang="ts">
	/**
	 * The "…" menu of a row that is itself clickable (project head, worktree row, explorer row).
	 *
	 * The row is a `role="button"` with an `onclick` that selects it. The kit DropdownMenu trigger does not stop
	 * its click, so a bare trigger inside the row also selected the row, and switching re-rendered the sidebar
	 * under the menu (codeatlasdev/atlas#67). This is a real `<button>` that keeps its press and its keys to
	 * itself: Enter / Space open the menu, never the row.
	 */
	import { DropdownMenu } from "@codeatlasdev/ui";
	import type { Snippet } from "svelte";

	let {
		label,
		icon,
		children,
		class: className = "",
	}: {
		/** Accessible name of the button. */
		label: string;
		icon: Snippet;
		/** Menu items. */
		children: Snippet;
		class?: string;
	} = $props();
</script>

<DropdownMenu align="end">
	{#snippet triggerChild(props)}
		{@const { onclick, onkeydown, ...rest } = props as Record<string, any>}
		<button
			type="button"
			{...rest}
			class={className}
			aria-label={label}
			onclick={(e) => {
				e.stopPropagation();
				onclick?.(e);
			}}
			onkeydown={(e) => {
				e.stopPropagation();
				onkeydown?.(e);
			}}
		>
			{@render icon()}
		</button>
	{/snippet}
	{@render children()}
</DropdownMenu>
