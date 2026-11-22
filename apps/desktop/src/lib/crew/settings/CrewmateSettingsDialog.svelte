<script lang="ts">
	import Lazy from "#lib/components/Lazy.svelte";

	/**
	 * The lazy door of the crewmate settings dialog: its code and its messages load the first time it opens, so the
	 * startup bundle does not carry a dialog nobody has opened (`CrewmateSettingsDialogBody.svelte`).
	 */
	let { open = $bindable(false) }: { open?: boolean } = $props();

	let opened = $state(false);
	$effect(() => {
		if (open) opened = true;
	});
</script>

{#if opened}
	<Lazy
		load={() => import("#lib/crew/settings/CrewmateSettingsDialogBody.svelte")}
		props={{
			get open() {
				return open;
			},
			set open(v: boolean) {
				open = v;
			},
		}}
	/>
{/if}
