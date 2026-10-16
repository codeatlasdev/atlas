<script lang="ts">
	// Test host: a Dialog and, DECLARED OUTSIDE IT, the overlays Atlas opens from inside Settings (a menu whose trigger
	// lives in the dialog's portal-less sibling markup, a Select). The kit declares them where the app does, so only its
	// layer engine can put them inside the modal that `showModal()` made the only interactive subtree.
	import { Dialog, DropdownMenu, DropdownMenuItem, Popover } from "@codeatlasdev/ui";

	let { open = $bindable(true) }: { open?: boolean } = $props();
	let anchor: HTMLElement | undefined = $state();
</script>

<Dialog bind:open title="Settings" chromeless>
	<button class="in-dialog" bind:this={anchor}>anchor</button>
</Dialog>

<!-- Outside the dialog in the source, opened from a trigger that sits inside it (see the test). -->
<DropdownMenu>
	{#snippet triggerChild(props)}<button class="menu-t" {...props}>menu</button>{/snippet}
	<DropdownMenuItem>one</DropdownMenuItem>
</DropdownMenu>
<Popover>
	{#snippet triggerChild(props)}<button class="pop-t" {...props}>pop</button>{/snippet}
	<span>body</span>
</Popover>
