<script lang="ts">
	import Contain from "#lib/errors/Contain.svelte";
	import RightSidebar from "#lib/components/RightSidebar.svelte";
	import DockResizer from "#lib/components/DockResizer.svelte";
	import EdgePeek from "#lib/shell/EdgePeek.svelte";
	import { docks } from "#lib/stores/docks.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	/**
	 * The right sidebar in both modes: pinned (a resizable dock beside the panes) or hidden
	 * (floating over the content, revealed from the window's right edge). Render it once as
	 * the last child of the worktree body; it renders nothing while the dock is zoomed away.
	 */
	let { zoomed = false }: { zoomed?: boolean } = $props();
</script>

{#snippet sidebar()}
	<Contain where="Right sidebar" title={m.shell_chrome_right_dock_error_title()}>
		<RightSidebar />
	</Contain>
{/snippet}

{#if !zoomed}
	{#if docks.right}
		<DockResizer side="right" />
		<div class="dock" style:width="{docks.rightSize}px">{@render sidebar()}</div>
	{/if}
	<EdgePeek edge="right" size={docks.rightSize} label={m.shell_chrome_right_dock_right_sidebar()} active={!docks.right}>
		{@render sidebar()}
	</EdgePeek>
{/if}

<style>
	.dock {
		flex: none;
		display: flex;
		min-width: 0;
		min-height: 0;
	}
</style>
