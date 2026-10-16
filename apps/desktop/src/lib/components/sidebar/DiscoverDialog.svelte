<script lang="ts">
	/**
	 * "Find projects": results stream in from the Rust scan. Grouped by scan root, filterable,
	 * multi-select, with moved-copy hints for missing projects and an optional "Could use Atlas"
	 * section. Driven by the `discovery` store.
	 */
	import { Button, Checkbox, Dialog, Spinner, Switch } from "@codeatlasdev/ui";
	import { open as pickFolder } from "@tauri-apps/plugin-dialog";
	import { FolderPlus, Search, TriangleAlert, X } from "@lucide/svelte";
	import { tilde } from "#lib/format.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { discovery } from "#lib/stores/discovery.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ago, groupFound, matchesQuery, movedCopy, splitFound, type Found } from "./discovery.ts";
	import { addPaths, relinkProject } from "./projects.ts";

	let query = $state("");
	/** Explicit user choices; everything else follows the default (valid configs ticked). */
	let override = $state<Record<string, boolean>>({});

	const split = $derived(splitFound(discovery.found));
	const groups = $derived(groupFound(split.projects, query));
	const candidateGroups = $derived(groupFound(split.candidates, query));
	const isPicked = (f: Found) => override[f.path] ?? !f.error;
	/** Ticked folders that are on screen: a filter never adds rows the user cannot see. */
	const picked = $derived(
		discovery.addable.filter((f) => isPicked(f) && matchesQuery(f, query)).map((f) => f.path),
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
		const dir = await pickFolder({ directory: true, title: "Folder to scan for projects" });
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

	const added = (f: Found) => discovery.registered.has(f.path);
</script>

<Dialog
	open={discovery.open}
	onOpenChange={(o) => !o && discovery.close()}
	title="Find projects"
	description="Looks for atlas.yaml in your usual project folders and with Spotlight."
	size="xl"
>
	<div class="bar">
		<label class="search">
			<Search size={13} />
			<input
				placeholder="Filter by name, path, branch or remote"
				aria-label="Filter results"
				bind:value={query}
			/>
			{#if query}
				<button aria-label="Clear filter" onclick={() => (query = "")}><X size={12} /></button>
			{/if}
		</label>
		<Switch
			label="Also folders without atlas.yaml"
			checked={discovery.candidates}
			onchange={toggleCandidates}
		/>
	</div>

	<div class="status" role="status" aria-live="polite">
		{#if discovery.running}
			<Spinner size={12} />
			<span>
				Scanning… {discovery.dirs.toLocaleString()} folders · {(discovery.elapsedMs / 1000).toFixed(1)}s ·
				{split.projects.length} found
			</span>
			<span class="cur mono" dir="rtl" title={discovery.current}
				><bdi dir="ltr">{tilde(discovery.current)}</bdi></span
			>
			<Button size="sm" variant="ghost" onclick={() => discovery.cancel()}>Cancel</Button>
		{:else if discovery.error}
			<TriangleAlert size={13} /> <span>{discovery.error}</span>
			<Button size="sm" variant="ghost" onclick={() => discovery.start()}>Retry</Button>
		{:else if discovery.done}
			<span>
				{discovery.done.cancelled ? "Cancelled after" : "Scanned"}
				{discovery.done.dirs.toLocaleString()} folders in {(discovery.done.elapsed_ms / 1000).toFixed(1)}s
				{#if discovery.done.unreadable}· {discovery.done.unreadable} not readable{/if}
			</span>
			<Button size="sm" variant="ghost" onclick={() => discovery.start()}>Scan again</Button>
		{/if}
	</div>

	<div class="results">
		{#each groups as g (g.root)}
			{@render group(g.root, g.items)}
		{:else}
			{#if !discovery.running}
				<p class="empty">
					{query ? `No project matches “${query}”.` : "No atlas.yaml found in the scanned folders."}
				</p>
			{/if}
		{/each}

		{#if discovery.candidates && candidateGroups.length}
			<h4 class="section">Could use Atlas</h4>
			<p class="note">Git repositories with a project file but no atlas.yaml.</p>
			{#each candidateGroups as g (g.root)}
				{@render group(g.root, g.items)}
			{/each}
		{/if}
	</div>

	{#snippet actions()}
		<Button variant="ghost" onclick={addRoot}><FolderPlus size={13} /> Add folder to scan…</Button>
		<span class="spacer"></span>
		<Button variant="ghost" onclick={() => discovery.close()}>Close</Button>
		<Button disabled={!picked.length} onclick={addSelected}>
			Add selected{picked.length ? ` (${picked.length})` : ""}
		</Button>
	{/snippet}
</Dialog>

{#snippet group(root: string, items: Found[])}
	<section class="group">
		<h4 class="root mono" title={root || "Found by Spotlight"}>
			{root ? tilde(root) : "Elsewhere (found by Spotlight)"}
		</h4>
		{#each items as f (f.path)}
			{@const move = moveByPath[f.path]}
			<div class="row" class:dim={added(f)}>
				{#if f.has_config && !added(f)}
					<Checkbox
						checked={isPicked(f)}
						onchange={(on) => (override[f.path] = on)}
						aria-label="Add {f.name}"
					/>
				{:else}
					<span class="nocheck"></span>
				{/if}
				<div class="main">
					<div class="line1">
						<strong>{f.name}</strong>
						{#if f.nested}<span class="tag">package</span>{/if}
						{#if added(f)}<span class="tag ok">In Atlas</span>{/if}
						{#if f.stack}<span class="tag">{f.stack}</span>{/if}
						{#if f.has_config}
							<span class="meta">{f.services} service{f.services === 1 ? "" : "s"}</span>
						{/if}
						{#if f.branch}<span class="meta mono">{f.branch}</span>{/if}
						<span class="meta when">{ago(f.modified)}</span>
					</div>
					<div class="path mono ca-selectable" title={f.path}>{tilde(f.path)}</div>
					{#if f.remote}<div class="path mono">{f.remote}</div>{/if}
					{#if f.error}
						<div class="err" title={f.error}><TriangleAlert size={11} /> atlas.yaml has problems</div>
					{/if}
					{#if move}
						<div class="move">
							Probably the moved copy of <strong>{move.name}</strong>
							<Button size="sm" onclick={() => relink(f)}>Relink</Button>
						</div>
					{/if}
				</div>
				{#if !f.has_config}
					<Button size="sm" variant="glass" disabled={!!store.busy[`init:${f.path}`]} onclick={() => init(f)}
						>Init</Button
					>
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
