<script lang="ts">
	import { CircleCheck, CircleX, X } from "@lucide/svelte";
	import { corner } from "#lib/shell/cornerStack.ts";
	import type { ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { actionOutput } from "./actionOutput.svelte.ts";

	let { project }: { project: ProjectView } = $props();

	/** How long the "View output" shortcut stays after an action finished. */
	const SHOW_MS = 8000;

	let live = $derived(store.live[project.id]);
	/** Run ids dismissed or already looked at. */
	let dismissed = $state<number[]>([]);
	let now = $state(Date.now());

	// The newest finished run, while fresh and unseen. Opening the drawer counts as seen.
	let run = $derived.by(() => {
		void live?.runs.rev;
		const r = live?.runs.list.findLast((x) => x.status !== "running");
		if (!r || r.end === null || dismissed.includes(r.id)) return null;
		return now - r.end < SHOW_MS ? r : null;
	});

	$effect(() => {
		const r = run;
		if (!r || r.end === null) return;
		const left = SHOW_MS - (Date.now() - r.end);
		now = Date.now();
		const t = setTimeout(() => (now = Date.now()), left + 50);
		return () => clearTimeout(t);
	});

	$effect(() => {
		if (actionOutput.open && run) dismissed.push(run.id);
	});

	function view() {
		if (!run) return;
		actionOutput.show(project.id, run.id);
	}
</script>

{#if run && !actionOutput.open && store.current?.id === project.id}
	<div use:corner class="chip" class:fail={run.status === "failed"} role="status">
		{#if run.status === "ok"}<CircleCheck size={14} />{:else}<CircleX size={14} />{/if}
		<span class="msg">{run.label} {run.status === "ok" ? "finished" : "failed"}</span>
		<button class="view" onclick={view}>View output</button>
		<button class="x" aria-label="Dismiss" onclick={() => dismissed.push(run!.id)}><X size={12} /></button>
	</div>
{/if}

<style>
	.chip {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px 6px 12px;
		border-radius: var(--ca-radius-md);
		border: 1px solid var(--ca-outline-subtle);
		border-left: 3px solid var(--ca-semantic-success);
		/* Opaque: it floats over the pager, and a blur over moving content re-renders every frame of a swipe. */
		background: var(--ca-surface-raised);
		box-shadow: var(--ca-shadow-md);
		font-size: 12.5px;
		color: var(--ca-text-primary);
	}
	.chip.fail {
		border-left-color: var(--ca-semantic-error);
	}
	.msg {
		max-width: 200px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.view {
		border: 0;
		border-radius: 6px;
		padding: 3px 8px;
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 12px;
	}
	.view:hover {
		background: var(--ca-surface-raised);
	}
	.x {
		display: inline-flex;
		border: 0;
		background: none;
		padding: 3px;
		border-radius: 4px;
		color: var(--ca-text-disabled);
	}
	.x:hover {
		color: var(--ca-text-primary);
	}
</style>
