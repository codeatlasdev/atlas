<script lang="ts" module>
	export interface PointerMenuItem {
		label: string;
		destructive?: boolean;
		/** A separator is drawn before this item when its group differs from the previous item's. */
		group?: number;
		run: () => void;
	}
</script>

<script lang="ts">
	/**
	 * A kit DropdownMenu opened at a point (right-click, Shift+F10, the `…` button) instead of under its own trigger:
	 * the kit positions a menu against its trigger, so the trigger is a zero-size fixed button parked at the point
	 * and `openAt` clicks it. One menu per list, so the `…` button and the context menu share the same items.
	 */
	import { tick } from "svelte";
	import { DropdownMenu, DropdownMenuItem, DropdownMenuSeparator } from "@codeatlasdev/ui";

	let { items }: { items: PointerMenuItem[] } = $props();

	let at = $state({ x: 0, y: 0 });
	let anchor: HTMLButtonElement | undefined = $state();

	/** Open the menu with its corner at viewport point (x, y). */
	export async function openAt(x: number, y: number) {
		at = { x, y };
		await tick();
		anchor?.click();
	}
</script>

<DropdownMenu align="start" side="bottom" sideOffset={0}>
	{#snippet triggerChild(props)}
		<button
			{...props}
			bind:this={anchor}
			type="button"
			class="pointer-anchor"
			aria-label="Actions"
			tabindex="-1"
			style:left="{at.x}px"
			style:top="{at.y}px"
		></button>
	{/snippet}
	{#each items as item, i (item.label)}
		{#if i > 0 && item.group !== items[i - 1].group}<DropdownMenuSeparator />{/if}
		<DropdownMenuItem variant={item.destructive ? "destructive" : "default"} onclick={item.run}
			>{item.label}</DropdownMenuItem
		>
	{/each}
</DropdownMenu>

<style>
	.pointer-anchor {
		position: fixed;
		width: 0;
		height: 0;
		padding: 0;
		border: 0;
		opacity: 0;
		pointer-events: none;
	}
</style>
