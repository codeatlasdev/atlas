<script lang="ts" generics="P extends Record<string, unknown>">
	/**
	 * Renders a component whose module is fetched on first use, so heavy views (editor, diff, chat,
	 * charts) stay out of the startup bundle. Nothing is drawn until it arrives: the pane behind it
	 * is already themed, so there is no placeholder frame to flicker. Pass `import("…svelte")` as
	 * `load` and the component's props as `props`. A view that sits in a flex column and would shift
	 * its siblings while absent passes `pending`: an empty box of the final size, drawn until it arrives.
	 */
	import { untrack, type Component, type Snippet } from "svelte";
	import { messageOf } from "#lib/errors/report.ts";
	import PaneNotice from "./panes/PaneNotice.svelte";

	let {
		load,
		props,
		pending,
	}: { load: () => Promise<{ default: Component<P> }>; props: P; pending?: Snippet } = $props();

	// One import per instance: the loader is a fixed expression at the use site, and a fresh arrow
	// on every parent render must not restart the await (that would blank the view).
	const loaded = untrack(() => load());
</script>

{#await loaded}
	{@render pending?.()}
{:then mod}
	{@const View = mod.default}
	<View {...props} />
{:catch err}
	<PaneNotice>Could not load this view: {messageOf(err)}</PaneNotice>
{/await}
