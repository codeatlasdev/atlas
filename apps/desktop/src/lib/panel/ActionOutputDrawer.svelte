<script lang="ts">
	import { onMount } from "svelte";
	import type { ProjectView } from "#lib/api.ts";
	import Lazy from "#lib/components/Lazy.svelte";
	import { m } from "#lib/i18n/m.ts";
	import { named, register } from "#lib/keymap.svelte.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { actionOutput } from "./actionOutput.svelte.ts";

	/**
	 * The "Show action output" command and the lazy door of the drawer: its code and its messages load the first time
	 * it opens, so the startup bundle does not carry a drawer nobody has opened (`ActionOutputDrawerBody.svelte`).
	 */
	let { project }: { project: ProjectView } = $props();

	const space = useSpace();
	let opened = $state(false);
	$effect(() => {
		if (actionOutput.open) opened = true;
	});

	onMount(() =>
		register(
			named([
				{
					id: "atlas::ShowActionOutput",
					label: () => m.panel_action_output_drawer_show_action_output(),
					run: () => actionOutput.show(project.id),
					when: () => space.current && store.current?.id === project.id,
				},
			]),
		),
	);
</script>

{#if opened}
	<Lazy load={() => import("./ActionOutputDrawerBody.svelte")} props={{ project }} />
{/if}
