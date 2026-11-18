<script lang="ts">
	import Lazy from "#lib/components/Lazy.svelte";
	import { machine } from "./machine.svelte.ts";

	/**
	 * The lazy door of the Machine drawer: its code and its messages load the first time it opens, so the startup
	 * bundle does not carry a drawer nobody has opened (`MachineDrawerBody.svelte`).
	 */
	let opened = $state(false);
	$effect(() => {
		if (machine.open) opened = true;
	});
</script>

{#if opened}
	<Lazy load={() => import("./MachineDrawerBody.svelte")} props={{}} />
{/if}
