<script lang="ts">
	import { useNow } from "#lib/clock.svelte.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { Button, EmptyState, toast } from "@codeatlasdev/ui";
	import { Copy, RotateCw, Terminal } from "@lucide/svelte";
	import { onMount } from "svelte";
	import type { ProjectView } from "#lib/api.ts";
	import { runAction } from "#lib/actions.svelte.ts";
	import { stripAnsi } from "#lib/ansi.ts";
	import { register } from "#lib/keymap.svelte.ts";
	import { hms } from "#lib/logs/text.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { actionOutput } from "./actionOutput.svelte.ts";
	import ActionOutputLog from "./ActionOutputLog.svelte";
	import { duration } from "./actionRuns.ts";
	import DrawerRow from "./DrawerRow.svelte";
	import DrawerShell from "./DrawerShell.svelte";
	import StatusDot from "./StatusDot.svelte";

	let { project }: { project: ProjectView } = $props();

	let live = $derived(store.live[project.id]);
	let cfg = $derived(store.configs[project.id]);
	const t = useNow(1000);

	// Reading `rev` tracks every change of the (non-reactive) rings.
	let runs = $derived.by(() => {
		void live?.runs.rev;
		return [...(live?.runs.list ?? [])].reverse();
	});
	let run = $derived.by(() => {
		const id = actionOutput.selected[project.id];
		return runs.find((r) => r.id === id) ?? runs[0];
	});
	// The action definition of the selected run (by label), for "Run again".
	let def = $derived(cfg?.actions.find((a) => a.label === run?.label && !a.session));
	let busy = $derived(!!def && !!store.busy[`action:${project.id}:${def.key}`]);

	async function copy() {
		if (!run) return;
		const text = run.lines.items.map((l) => stripAnsi(l.text)).join("\n");
		await navigator.clipboard.writeText(text);
		toast.success(`Copied ${run.lines.items.length.toLocaleString()} lines`);
	}

	const space = useSpace();
	let wtId = $derived(space.worktree?.id);
	let root = $derived(ws.worktree(wtId)?.path ?? project.root);

	onMount(() => {
		const unregister = register([
			{
				id: "atlas::ShowActionOutput",
				label: "Show action output",
				run: () => actionOutput.show(project.id),
				when: () => space.current && store.current?.id === project.id,
			},
		]);
		return () => {
			unregister();
			actionOutput.open = false;
		};
	});

	const dot = { running: "run", ok: "ok", failed: "fail" } as const;
</script>

<DrawerShell bind:open={actionOutput.open} title="Action output" width="560px">
	{#if !runs.length}
		<div class="empty">
			<EmptyState
				icon={Terminal}
				title="No action output yet"
				description="Run an action from the Actions menu; its output shows up here."
			/>
		</div>
	{:else}
		<div class="runs" role="listbox" aria-label="Action runs">
			{#each runs as r (r.id)}
				<DrawerRow
					tone={dot[r.status]}
					pulse={r.status === "running"}
					on={r.id === run?.id}
					role="option"
					aria-selected={r.id === run?.id}
					onclick={() => (actionOutput.selected[project.id] = r.id)}
				>
					<span class="label">{r.label}</span>
					<span class="time mono" title="Started {hms(r.start)}">{hms(r.start)}</span>
					<span class="dur mono">{duration(r, t.now)}</span>
				</DrawerRow>
			{/each}
		</div>

		{#if run}
			<div class="head">
				<StatusDot tone={dot[run.status]} pulse={run.status === "running"} />
				<b class="title">{run.label}</b>
				<span class="state">
					{run.status === "running" ? "running" : run.status === "ok" ? "finished" : "failed"}
				</span>
				<span class="spacer"></span>
				<Button variant="glass" size="sm" onclick={copy} disabled={!run.lines.items.length}>
					{#snippet leading()}<Copy size={13} />{/snippet}
					Copy
				</Button>
				<Button
					variant="glass"
					size="sm"
					onclick={() => def && runAction(project.id, def.key)}
					loading={busy}
					disabled={!def || !project.running || run.status === "running"}
					title={!def
						? "This action is no longer in atlas.yaml"
						: !project.running
							? "Start the stack first"
							: run.status === "running"
								? "Already running"
								: undefined}
				>
					{#snippet leading()}<RotateCw size={13} />{/snippet}
					Run again
				</Button>
			</div>

			<ActionOutputLog {run} rev={live?.runs.rev} active={actionOutput.open} {wtId} {root} />
		{/if}
	{/if}
</DrawerShell>

<style>
	.empty {
		flex: 1;
		display: grid;
		place-items: center;
	}
	.runs {
		flex: none;
		max-height: 168px;
		overflow-y: auto;
		margin: 0 -8px;
		padding: 0 8px;
	}
	.label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.time,
	.dur {
		flex: none;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.dur {
		min-width: 52px;
		text-align: right;
		color: var(--ca-text-secondary);
	}
	.head {
		flex: none;
		display: flex;
		align-items: center;
		gap: 8px;
		padding-top: 8px;
		border-top: 1px solid var(--ca-outline-subtle);
		font-size: 12.5px;
	}
	.title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.state {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.spacer {
		flex: 1;
	}
</style>
