<script lang="ts">
	import { Button, Kbd } from "@codeatlasdev/ui";
	import { Compass, Keyboard, ListChecks } from "@lucide/svelte";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { progress } from "./checklist.ts";
	import { onboarding } from "./state.svelte.ts";

	let p = $derived(progress(onboarding.state));

	/** Settings is a dialog over the app: close it first so the tour can spotlight the UI. */
	function run(fn: () => void) {
		updates.open = false;
		setTimeout(fn, 180);
	}
</script>

<div class="list">
	<div class="item" data-setting="tour">
		<span class="ico"><Compass size={16} /></span>
		<div class="text">
			<b>Guided tour</b>
			<small>A two-minute walk through spaces, the header, the side panel and search.</small>
		</div>
		<Button variant="glass" size="sm" onclick={() => run(() => onboarding.startTour())}>Take the tour</Button>
	</div>
	<div class="item" data-setting="help-keys">
		<span class="ico"><Keyboard size={16} /></span>
		<div class="text">
			<b>Shortcut cheat sheet</b>
			<small
				>Every key in one searchable list. Open it anywhere with <Kbd>{shortcutFor("atlas::Help") ?? "?"}</Kbd
				> anywhere.</small
			>
		</div>
		<Button variant="glass" size="sm" onclick={() => run(() => (onboarding.helpOpen = true))}>
			Open cheat sheet
		</Button>
	</div>
	<div class="item" data-setting="checklist">
		<span class="ico"><ListChecks size={16} /></span>
		<div class="text">
			<b>Getting started checklist</b>
			<small>
				{p.done} of {p.total} first steps done{onboarding.state.checklist.dismissed
					? ". The card is hidden."
					: "."}
			</small>
		</div>
		{#if onboarding.state.checklist.dismissed}
			<Button variant="glass" size="sm" onclick={() => run(() => void onboarding.showChecklist())}
				>Show the card</Button
			>
		{/if}
		<Button variant="ghost" size="sm" onclick={() => run(() => void onboarding.restart())}>Start over</Button>
	</div>
</div>

<style>
	.list {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px;
		border-radius: 11px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.ico {
		display: grid;
		place-items: center;
		flex-shrink: 0;
		width: 32px;
		height: 32px;
		border-radius: 9px;
		color: var(--ca-brand-main);
		background: color-mix(in oklab, var(--ca-brand-main) 12%, transparent);
	}
	.text {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		line-height: 1.35;
	}
	.text b {
		font-size: 13px;
		font-weight: 600;
	}
	.text small {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
