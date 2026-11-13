<script lang="ts">
	/**
	 * Settings > Workspaces > Hygiene: what Atlas and agents left behind. The report is the same one `atlas doctor
	 * --hygiene` prints; Fix ends only processes carrying an Atlas session tag whose owner is gone and frees only
	 * what Atlas registered. `target/`, `node_modules` and worktrees are listed, never removed.
	 */
	import { Button, Spinner, toast } from "@codeatlasdev/ui";
	import { api, errText } from "#lib/api.ts";
	import Group from "#lib/settings/Group.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";
	import Row from "#lib/settings/Row.svelte";
	import {
		byDisk,
		bytes,
		fixSummary,
		fixedText,
		projectTotal,
		type HygieneReport,
	} from "#lib/settings/hygiene.ts";

	let report = $state<HygieneReport | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);

	async function scan() {
		busy = true;
		error = null;
		try {
			report = await api.hygieneReport();
		} catch (e) {
			error = errText(e);
		} finally {
			busy = false;
		}
	}

	async function fix() {
		busy = true;
		try {
			const r = await api.hygieneFix();
			report = r.report;
			toast.success(fixedText(r.fixed));
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = false;
		}
	}

	let todo = $derived(report ? fixSummary(report) : null);
</script>

{#if error}
	<LoadError what="the hygiene report" {error} />
{/if}

<Group
	title="Hygiene"
	description="Agents leave browsers, dev servers and caches behind. Fix ends only processes Atlas tagged whose owner is gone, and frees only folders Atlas registered. target/, node_modules and worktrees are never removed."
>
	<Row
		id="ws-hygiene"
		label="Orphans and disk"
		description={report
			? todo
				? `Fix would ${todo}.`
				: "Nothing to clean: no orphans, no registered folder over its cap."
			: "Scan to list orphaned processes and the disk each project holds."}
	>
		{#if busy}<Spinner size={12} />{/if}
		<Button size="sm" variant="glass" disabled={busy} onclick={scan}>Scan</Button>
		<Button size="sm" variant="destructive" disabled={busy || !todo} onclick={fix}>Fix</Button>
	</Row>
	{#if report}
		{#each report.orphans as o (o.pid + ":" + o.start)}
			<div class="line">
				<span class="mono">{o.pid}</span>
				<span class="name">{o.name}</span>
				<span class="dim">{o.label ?? o.kind ?? ""}</span>
				<span class="dim path">{o.project ?? ""}</span>
			</div>
		{/each}
		{#each byDisk(report.projects) as p (p.root)}
			<div class="line">
				<span class="name path" title={p.root}>{p.name}</span>
				<span class="dim">target {bytes(p.target)}</span>
				<span class="dim">node_modules {bytes(p.node_modules)}</span>
				<span class="dim">total {bytes(projectTotal(p))}</span>
			</div>
		{/each}
		{#each report.dirs as d (d.path)}
			<div class="line">
				<span class="name path" title={d.path}>{d.path}</span>
				<span class="dim">{bytes(d.bytes)} / {bytes(d.cap_bytes)}</span>
				<span class="dim">{d.orphaned ? "owner gone" : d.kind}</span>
			</div>
		{/each}
	{/if}
</Group>

<style>
	.line {
		display: grid;
		grid-template-columns: minmax(0, 1.6fr) repeat(3, minmax(0, 1fr));
		gap: 8px;
		align-items: center;
		padding: 8px 14px;
		font-size: 12px;
	}
	.name {
		font-weight: 600;
	}
	.dim {
		color: var(--ca-text-secondary);
	}
	.mono {
		font-family: ui-monospace, monospace;
	}
	.path {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
