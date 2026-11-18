<script lang="ts">
	import Lazy from "#lib/components/Lazy.svelte";
	import { activity } from "./activity.svelte.ts";

	/**
	 * The lazy door of the Activity drawer: its code and its messages load the first time it opens, so the startup
	 * bundle does not carry a drawer nobody has opened (`ActivityDrawerBody.svelte`).
	 */
	let { projectId }: { projectId: string } = $props();

	let opened = $state(false);
	$effect(() => {
		if (activity.open) opened = true;
	});
</script>

{#if opened}
	<Lazy load={() => import("./ActivityDrawerBody.svelte")} props={{ projectId }} />
{/if}
