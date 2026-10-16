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

	/** Open the menu with its corner at viewport point (x, y). The kit has no `open` prop (codeatlasdev/ui#298) and a
	 * trigger click toggles, so an already open menu is closed first: a right-click on another row moves the menu. */
	export async function openAt(x: number, y: number) {
		if (anchor?.getAttribute("aria-expanded") === "true") {
			anchor.click();
			await tick();
		}
		// `fixed` is relative to the nearest transformed / will-change ancestor (the space view is one), not the
		// viewport: measure where (0, 0) lands and subtract it, so the anchor sits on the viewport point.
		at = { x: 0, y: 0 };
		await tick();
		const origin = anchor?.getBoundingClientRect() ?? { left: 0, top: 0 };
		at = { x: x - origin.left, y: y - origin.top };
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
			style:width="1px"
			style:height="1px"
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
		/* Sized inline to 1px, not 0: the kit's menus use `position-visibility: anchors-visible`, and WebKit counts an
		   empty anchor as not visible, so a 0x0 anchor left the menu open but unpainted. */
		padding: 0;
		border: 0;
		opacity: 0;
		pointer-events: none;
	}
</style>
