<script lang="ts" generics="P extends Record<string, unknown>">
	/**
	 * Renders a component whose module is fetched on first use, so heavy views (editor, diff, chat,
	 * charts) stay out of the startup bundle. Nothing is drawn until it arrives: the pane behind it
	 * is already themed, so there is no placeholder frame to flicker. Pass `import("…svelte")` as
	 * `load` and the component's props as `props`.
	 */
	import { untrack, type Component } from "svelte";
	import { messageOf } from "#lib/errors/report.ts";
	import PaneNotice from "./panes/PaneNotice.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { load, props }: { load: () => Promise<{ default: Component<P> }>; props: P } = $props();

	// One import per instance: the loader is a fixed expression at the use site, and a fresh arrow
	// on every parent render must not restart the await (that would blank the view).
	const loaded = untrack(() => load());
</script>

{#await loaded then mod}
	{@const View = mod.default}
	<View {...props} />
{:catch err}
	<PaneNotice>{m.workbench_lazy_load_failed({ error: messageOf(err) })}</PaneNotice>
{/await}
