<script lang="ts">
	import { Button, Drawer, EmptyState, toast } from "@codeatlasdev/ui";
	import { Copy, RotateCw, Terminal } from "@lucide/svelte";
	import { onMount, tick } from "svelte";
	import { api, type ProjectView } from "#lib/api.ts";
	import { runAction } from "#lib/actions.svelte.ts";
	import { stripAnsi } from "#lib/ansi.ts";
	import { register } from "#lib/keymap.svelte.ts";
	import { hms, resolvePath } from "#lib/logs.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { actionOutput } from "./actionOutput.svelte.ts";
	import { duration } from "./actionRuns.ts";
	import { segments, type Seg } from "./actionSegments.ts";

	let { project }: { project: ProjectView } = $props();

	/** Rendered tail: a run keeps up to 5000 lines, the DOM only needs the end unless asked. */
	const TAIL = 1500;

	let live = $derived(store.live[project.id]);
	let cfg = $derived(store.configs[project.id]);
	let now = $state(Date.now());

	// Reading `rev` tracks every change of the (non-reactive) rings.
	let runs = $derived.by(() => {
		void live?.runs.rev;
		return [...(live?.runs.list ?? [])].reverse();
	});
	let run = $derived.by(() => {
		const id = actionOutput.selected[project.id];
		return runs.find((r) => r.id === id) ?? runs[0];
	});
	let all = $state(false);
	let rows = $derived.by(() => {
		void live?.runs.rev;
		const items = run?.lines.items ?? [];
		const shown = all ? items : items.slice(-TAIL);
		const hidden = items.length - shown.length;
		return { lines: shown, hidden, first: (run?.lines.base ?? 0) + hidden };
	});
	// The action definition of the selected run (by label), for "Run again".
	let def = $derived(cfg?.actions.find((a) => a.label === run?.label && !a.session));
	let busy = $derived(!!def && !!store.busy[`action:${project.id}:${def.key}`]);

	let viewport = $state<HTMLElement>();
	let follow = $state(true);

	function onscroll() {
		if (!viewport) return;
		follow = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 24;
	}

	// Auto-follow while the selected run is live and the reader is at the bottom.
	$effect(() => {
		void live?.runs.rev;
		if (!actionOutput.open || !follow || run?.status !== "running") return;
		tick().then(() => viewport && (viewport.scrollTop = viewport.scrollHeight));
	});

	// Switching runs starts at the end (what just happened), following again.
	$effect(() => {
		void run?.id;
		all = false;
		follow = true;
		tick().then(() => viewport && (viewport.scrollTop = viewport.scrollHeight));
	});

	$effect(() => {
		if (!actionOutput.open) return;
		now = Date.now();
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(t);
	});

	async function copy() {
		if (!run) return;
		const text = run.lines.items.map((l) => stripAnsi(l.text)).join("\n");
		await navigator.clipboard.writeText(text);
		toast.success(`Copied ${run.lines.items.length.toLocaleString()} lines`);
	}

	let wtId = $derived(ws.selected?.id);
	let root = $derived(ws.worktree(wtId)?.path ?? project.root);

	async function openLink(e: MouseEvent, seg: Seg) {
		const link = seg.link;
		if (!link) return;
		e.preventDefault();
		try {
			if (link.kind === "url" || link.kind === "osc") {
				if (/^https?:/i.test(link.target) && wtId) await openPreview(wtId, link.target, e.shiftKey);
				else await api.openUrl(link.target);
				return;
			}
			const rel = resolvePath(link.target, root, null);
			if (rel && wtId) openFile(wtId, rel, link.line, link.col);
			else toast.error(`${link.target} is outside the project`);
		} catch (err) {
			toast.error(err instanceof Error ? err.message : String(err));
		}
	}

	onMount(() => {
		const unregister = register([
			{
				id: "atlas::ShowActionOutput",
				label: "Show action output",
				run: () => actionOutput.show(project.id),
				when: () => store.current?.id === project.id,
			},
		]);
		return () => {
			unregister();
			actionOutput.open = false;
		};
	});

	const dot = { running: "run", ok: "ok", failed: "fail" } as const;
</script>

<Drawer bind:open={actionOutput.open} side="right" title="Action output" width="560px">
	<div class="wrap">
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
					<button
						class="run"
						class:on={r.id === run?.id}
						role="option"
						aria-selected={r.id === run?.id}
						onclick={() => (actionOutput.selected[project.id] = r.id)}
					>
						<span class="dot t-{dot[r.status]}" class:pulse={r.status === "running"}></span>
						<span class="label">{r.label}</span>
						<span class="time mono" title="Started {hms(r.start)}">{hms(r.start)}</span>
						<span class="dur mono">{duration(r, now)}</span>
					</button>
				{/each}
			</div>

			{#if run}
				<div class="head">
					<span class="dot t-{dot[run.status]}" class:pulse={run.status === "running"}></span>
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

				<div class="out mono" bind:this={viewport} {onscroll} tabindex="-1">
					{#if run.truncated > 0}
						<div class="note">
							{run.truncated.toLocaleString()} earlier lines were not kept (attached mid-run); full output is in
							.logs/.
						</div>
					{/if}
					{#if rows.hidden > 0}
						<button class="note more" onclick={() => (all = true)}>
							Show {rows.hidden.toLocaleString()} earlier lines
						</button>
					{/if}
					{#each rows.lines as l, i (rows.first + i)}
						<div class="ln">
							{#each segments(l.text) as s}{#if s.link}<a
										href={s.link.target}
										class="lnk"
										style={s.css}
										title={s.link.kind === "file" ? "Open file" : "Open link (⇧ for the browser)"}
										onclick={(e) => openLink(e, s)}>{s.text}</a
									>{:else}<span style={s.css}>{s.text}</span>{/if}{/each}
						</div>
					{:else}
						<div class="note">{run.status === "running" ? "Waiting for output…" : "No output."}</div>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
</Drawer>

<style>
	.wrap {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-height: 0;
		height: 100%;
	}
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
	.run {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		height: 28px;
		padding: 0 6px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 12.5px;
		text-align: left;
	}
	.run:hover {
		background: var(--ca-action-hover);
	}
	.run.on {
		background: var(--ca-surface-raised);
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
		color: var(--ca-text-disabled);
	}
	.dur {
		min-width: 52px;
		text-align: right;
		color: var(--ca-text-secondary);
	}
	.dot {
		flex: none;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-text-disabled);
	}
	.t-run {
		background: var(--space-accent, var(--ca-brand-main));
	}
	.t-ok {
		background: var(--ca-semantic-success);
	}
	.t-fail {
		background: var(--ca-semantic-error);
	}
	.pulse {
		animation: pulse 1.4s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			opacity: 0.35;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.pulse {
			animation: none;
		}
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
	.out {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--ca-surface-sunken, var(--ca-surface-raised));
		border: 1px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		line-height: 1.5;
		user-select: text;
		-webkit-user-select: text;
	}
	.ln {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		min-height: 1.5em;
	}
	.lnk {
		color: var(--ca-semantic-info);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}
	.note {
		padding: 2px 0 6px;
		font-family: inherit;
		font-size: 11px;
		color: var(--ca-text-disabled);
	}
	.more {
		display: block;
		border: 0;
		background: none;
		text-align: left;
		cursor: pointer;
	}
	.more:hover {
		color: var(--ca-text-primary);
	}
</style>
