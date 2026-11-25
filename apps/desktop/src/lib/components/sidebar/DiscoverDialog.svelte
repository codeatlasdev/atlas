<script lang="ts">
	/**
	 * "Find projects": results stream in from the Rust scan. Grouped by scan root, filterable,
	 * multi-select, with moved-copy hints for missing projects and an optional "Could use Atlas"
	 * section. Driven by the `discovery` store.
	 */
	import { Button, Checkbox, Dialog, Spinner, Switch } from "@codeatlasdev/ui";
	import { api } from "#lib/api.ts";
	import { FolderPlus, Search, TriangleAlert, X } from "@lucide/svelte";
	import { tilde } from "#lib/format.ts";
	import { formatNumber } from "#lib/i18n/fmt.ts";
	import Rich from "#lib/i18n/Rich.svelte";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { discovery } from "#lib/stores/discovery.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import {
		ago,
		groupFound,
		matchesQuery,
		movedCopy,
		noProjectsLine,
		pickable,
		scannedLine,
		scanningLine,
		splitFound,
		stackLabel,
		tickedByDefault,
		type Found,
	} from "./discovery.ts";
	import { addPaths, relinkProject } from "./projects.ts";
	import { m } from "#lib/i18n/m.ts";

	let query = $state("");
	/** Explicit user choices; everything else follows the default (valid configs ticked). */
	let override = $state<Record<string, boolean>>({});

	const split = $derived(splitFound(discovery.found));
	const groups = $derived(groupFound(split.projects, query));
	const candidateGroups = $derived(groupFound(split.candidates, query));
	const isPicked = (f: Found) => override[f.path] ?? tickedByDefault(f);
	/** Ticked folders that are on screen: a filter never adds rows the user cannot see. */
	const picked = $derived(
		pickable(discovery.found, discovery.registered)
			.filter((f) => isPicked(f) && matchesQuery(f, query))
			.map((f) => f.path),
	);
	/** Missing project → the discovered folder that is probably its new home. */
	const moves = $derived(
		store.missing.flatMap((p) => {
			const hit = movedCopy({ name: p.name, remote: p.last_remote }, discovery.found, discovery.registered);
			return hit ? [{ project: p, hit }] : [];
		}),
	);
	const moveByPath = $derived(Object.fromEntries(moves.map((m) => [m.hit.path, m.project])));

	// A closed dialog starts from the defaults again.
	$effect(() => {
		if (!discovery.open) {
			override = {};
			query = "";
		}
	});

	async function addSelected() {
		const paths = picked;
		await discovery.close();
		await addPaths(paths);
	}

	async function addRoot() {
		const dir = await api.pickPath({ directory: true, title: m.sidebar_discover_dialog_pick_root() });
		if (typeof dir !== "string") return;
		discovery.addRoot(dir);
		await discovery.start();
	}

	async function toggleCandidates(on: boolean) {
		discovery.setCandidates(on);
		await discovery.start();
	}

	async function relink(f: Found) {
		const p = moveByPath[f.path];
		if (p) await relinkProject(p.id, f.path);
	}

	/** Give a just-added worktree its layout before anything renders it, then open it. */
	function openNew(id: string) {
		ws.select(id);
		shell.select(id);
	}

	async function init(f: Found) {
		const id = await discovery.init(f.path);
		if (id) openNew(id);
	}

	/** A folder without atlas.yaml is added as is: its stack is detected, nothing is written. */
	async function addCandidate(f: Found) {
		await discovery.close();
		await addPaths([f.path]);
	}

	const added = (f: Found) => discovery.registered.has(f.path);
</script>

<Dialog
	open={discovery.open}
	onOpenChange={(o) => !o && discovery.close()}
	title={m.sidebar_discover_dialog_find_projects()}
	description={m.sidebar_discover_dialog_lead()}
	size="xl"
>
	<div class="bar">
		<label class="search">
			<Search size={13} />
			<input
				placeholder={m.sidebar_discover_dialog_filter_placeholder()}
				aria-label={m.sidebar_discover_dialog_filter_results()}
				bind:value={query}
			/>
			{#if query}
				<button aria-label={m.sidebar_clear_filter()} onclick={() => (query = "")}><X size={12} /></button>
			{/if}
		</label>
		<Switch
			label={m.sidebar_discover_dialog_candidates_switch()}
			checked={discovery.candidates}
			onchange={toggleCandidates}
		/>
	</div>

	<div class="status" role="status" aria-live="polite">
		{#if discovery.running}
			<Spinner size={12} />
			<span>{scanningLine(discovery.dirs, discovery.elapsedMs, split.projects.length)}</span>
			<span class="cur mono" dir="rtl" title={discovery.current}>
				<bdi dir="ltr">{tilde(discovery.current)}</bdi>
			</span>
			<Button size="sm" variant="ghost" onclick={() => discovery.cancel()}>{m.common_action_cancel()}</Button>
		{:else if discovery.error}
			<TriangleAlert size={13} /> <span>{discovery.error}</span>
			<Button size="sm" variant="ghost" onclick={() => discovery.start()}>{m.common_action_retry()}</Button>
		{:else if discovery.done}
			<span>
				{scannedLine(discovery.done)}
				{#if discovery.done.unreadable}
					· {m.sidebar_discover_dialog_unreadable({
						count: discovery.done.unreadable,
						amount: formatNumber(discovery.done.unreadable),
					})}{/if}
			</span>
			<Button size="sm" variant="ghost" onclick={() => discovery.start()}>
				{m.onboarding_phases_scan_again()}
			</Button>
		{/if}
	</div>

	<div class="results">
		{#each groups as g (g.root)}
			{@render group(g.root, g.items)}
		{:else}
			{#if !discovery.running}
				<p class="empty">
					{noProjectsLine(query, split.candidates.length)}
				</p>
			{/if}
		{/each}

		{#if discovery.candidates && candidateGroups.length}
			<h4 class="section">{m.sidebar_discover_dialog_could_use_atlas()}</h4>
			<p class="note">
				{m.sidebar_discover_dialog_candidates_note()}
			</p>
			{#each candidateGroups as g (g.root)}
				{@render group(g.root, g.items)}
			{/each}
		{/if}
	</div>

	{#snippet actions()}
		<Button variant="ghost" onclick={addRoot}>
			<FolderPlus size={13} />
			{m.sidebar_discover_dialog_add_root()}
		</Button>
		<span class="spacer"></span>
		<Button variant="ghost" onclick={() => discovery.close()}>{m.common_action_close()}</Button>
		<Button disabled={!picked.length} onclick={addSelected}>
			{m.sidebar_discover_dialog_add_selected({ count: picked.length })}
		</Button>
	{/snippet}
</Dialog>

{#snippet group(root: string, items: Found[])}
	<section class="group">
		<h4 class="root mono" title={root || m.sidebar_discover_dialog_elsewhere()}>
			{root ? tilde(root) : m.sidebar_discover_dialog_elsewhere()}
		</h4>
		{#each items as f (f.path)}
			{@const move = moveByPath[f.path]}
			<div class="row" class:dim={added(f)}>
				{#if !added(f)}
					<Checkbox
						checked={isPicked(f)}
						onchange={(on) => (override[f.path] = on)}
						aria-label={m.onboarding_phases_add_name({ name: f.name })}
					/>
				{:else}
					<span class="nocheck"></span>
				{/if}
				<div class="main">
					<div class="line1">
						<strong>{f.name}</strong>
						{#if f.nested}<span class="tag">{m.sidebar_discover_dialog_tag_package()}</span>{/if}
						{#if added(f)}<span class="tag ok">{m.sidebar_discover_dialog_tag_in_atlas()}</span>{/if}
						{#if f.stack}<span class="tag">{stackLabel(f.stack)}</span>{/if}
						{#if f.has_config}
							<span class="meta">{m.onboarding_flow_services({ count: f.services })}</span>
						{/if}
						{#if f.branch}<span class="meta mono">{f.branch}</span>{/if}
						<span class="meta when">{ago(f.modified)}</span>
					</div>
					<div class="path mono ca-selectable" title={f.path}>{tilde(f.path)}</div>
					{#if f.remote}<div class="path mono">{f.remote}</div>{/if}
					{#if f.error}
						<div class="err" title={f.error}>
							<TriangleAlert size={11} />
							{m.shell_space_view_config_problems()}
						</div>
					{/if}
					{#if move}
						<div class="move">
							<span>
								<Rich
									parts={m.sidebar_discover_dialog_moved_copy.parts({ name: move.name })}
									tags={{ b: "strong" }}
								/>
							</span>
							<Button size="sm" onclick={() => relink(f)}>{m.shell_home_missing_project_relink()}</Button>
						</div>
					{/if}
				</div>
				{#if !f.has_config}
					<Button size="sm" variant="glass" onclick={() => addCandidate(f)}>{m.common_action_add()}</Button>
					<Button
						size="sm"
						variant="ghost"
						title={m.sidebar_discover_dialog_init_title()}
						disabled={!!store.busy[`init:${f.path}`]}
						onclick={() => init(f)}
					>
						{m.sidebar_discover_dialog_create_atlas_yaml()}
					</Button>
				{/if}
			</div>
		{/each}
	</section>
{/snippet}

<style>
	.bar {
		display: flex;
		align-items: center;
		gap: 16px;
		margin-bottom: 8px;
	}
	.search {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 6px;
		height: 30px;
		padding: 0 8px;
		border: 0.5px solid var(--atlas-control-border, var(--ca-outline-default));
		border-radius: 8px;
		color: var(--ca-text-secondary);
	}
	.search input {
		flex: 1;
		min-width: 0;
		border: 0;
		outline: none;
		background: none;
		font: inherit;
		font-size: 12.5px;
		color: var(--ca-text-primary);
	}
	.search button {
		display: inline-grid;
		place-items: center;
		border: 0;
		background: none;
		color: inherit;
	}
	.status {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 28px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.status .cur {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		text-align: left;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.results {
		max-height: 46vh;
		overflow-y: auto;
		padding-right: 4px;
	}
	.group {
		margin-bottom: 10px;
	}
	.root {
		margin: 8px 0 4px;
		font-size: 11px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.section {
		margin: 16px 0 2px;
		font-size: 12.5px;
	}
	.note,
	.empty {
		margin: 0 0 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 6px 4px;
		border-radius: 8px;
	}
	.row:hover {
		background: var(--ca-vibrancy-fill);
	}
	.row.dim .main {
		color: var(--ca-text-secondary);
	}
	.nocheck {
		flex: none;
		width: 16px;
	}
	.main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.line1 {
		display: flex;
		align-items: baseline;
		gap: 8px;
		font-size: 13px;
	}
	.meta,
	.path {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.when {
		margin-left: auto;
	}
	.path {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tag {
		font-size: 10px;
		padding: 0 5px;
		border-radius: 4px;
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
		color: var(--ca-text-secondary);
	}
	.tag.ok {
		color: var(--ca-semantic-success);
		background: color-mix(in oklab, var(--dot-ok) 14%, transparent);
	}
	.err {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11.5px;
		color: var(--ca-semantic-error);
	}
	.move {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 4px;
		font-size: 12px;
		color: var(--ca-semantic-warning);
	}
	.spacer {
		flex: 1;
	}
</style>
