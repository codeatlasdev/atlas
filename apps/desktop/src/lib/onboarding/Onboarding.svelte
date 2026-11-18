<script lang="ts">
	import { onMount } from "svelte";
	import { register } from "#lib/keymap.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import Lazy from "#lib/components/Lazy.svelte";
	import Checklist from "./Checklist.svelte";
	import Shortcuts from "./Shortcuts.svelte";
	import { onboarding } from "./state.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	onMount(() => {
		void onboarding.init();
		const stop = onboarding.observe();
		const unregister = register([
			{
				id: "atlas::Help",
				get label() {
					return m.onboarding_commands_help();
				},
				run: () => (onboarding.helpOpen = true),
			},
			{
				id: "atlas::TakeTour",
				get label() {
					return m.onboarding_welcome_take_the_tour();
				},
				run: () => onboarding.startTour(),
			},
			{
				id: "atlas::ShowChecklist",
				get label() {
					return m.onboarding_commands_show_checklist();
				},
				run: () => void onboarding.showChecklist(),
			},
		]);
		return () => {
			stop();
			unregister();
		};
	});

	/** `?` outside text fields opens the cheat sheet (⌘/ is bound through the keymap). */
	function onkeydown(e: KeyboardEvent) {
		if (e.key !== "?" || e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented) return;
		const t = e.target as HTMLElement | null;
		if (t?.closest("input, textarea, select, [contenteditable], .xterm, .cm-editor, [data-terminal]")) return;
		e.preventDefault();
		onboarding.helpOpen = !onboarding.helpOpen;
	}

	// The tour spotlights the worktree view; leave agents/dashboard first.
	$effect(() => {
		if (onboarding.tourOpen && ws.view !== "worktree") ws.view = "worktree";
	});
</script>

<svelte:window {onkeydown} />

<!-- The deck and the tour run once in a person's life: their code and their copy load when they are first shown. -->
{#if onboarding.firstRun}<Lazy load={() => import("./FirstRun.svelte")} props={{}} />{/if}
{#if onboarding.tourOpen}<Lazy load={() => import("./Tour.svelte")} props={{}} />{/if}
<Checklist />
<Shortcuts />
