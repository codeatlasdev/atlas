<script lang="ts">
	import { onMount } from "svelte";
	import { toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { register } from "#lib/keymap.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import Checklist from "./Checklist.svelte";
	import FirstRun from "./FirstRun.svelte";
	import Shortcuts from "./Shortcuts.svelte";
	import Tour from "./Tour.svelte";
	import { onboarding } from "./state.svelte.ts";

	onMount(() => {
		onboarding.init().catch((e) => toast.error(errText(e)));
		const stop = onboarding.observe();
		const unregister = register([
			{ id: "atlas::Help", label: "Help: keyboard shortcuts", run: () => (onboarding.helpOpen = true) },
			{ id: "atlas::TakeTour", label: "Take the tour", run: () => onboarding.startTour() },
			{
				id: "atlas::ShowChecklist",
				label: "Show getting started checklist",
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

{#if onboarding.firstRun}<FirstRun />{/if}
{#if onboarding.tourOpen}<Tour />{/if}
<Checklist />
<Shortcuts />
