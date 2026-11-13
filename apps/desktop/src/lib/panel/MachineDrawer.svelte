<script lang="ts">
	import { usePoll } from "#lib/clock.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { EmptyState } from "@codeatlasdev/ui";
	import { Gauge } from "@lucide/svelte";
	import { onMount } from "svelte";
	import DrawerRow from "./DrawerRow.svelte";
	import DrawerShell from "./DrawerShell.svelte";
	import type { DotTone } from "./StatusDot.svelte";
	import { describe, human, summary } from "./heavy.ts";
	import { agentRows, bytes, gauge, hygieneView, runTrend, serviceNote, type Gauge as G } from "./machine.ts";
	import { machine } from "./machine.svelte.ts";

	// Polled only while the drawer is open and the page visible; the report is a `ps` and a few small files.
	usePoll(
		3000,
		() => machine.open,
		() => void machine.refresh(),
	);

	let r = $derived(machine.report);
	let jobs = $derived(r ? [...r.heavy.running, ...r.heavy.queued] : []);
	let rows = $derived(r ? agentRows(r.terminals, ws.doc.tabs, agents.status) : []);
	let hygiene = $derived(hygieneView(machine.hygiene));

	const AGENT_TONE: Record<string, DotTone> = {
		working: "run",
		blocked: "fail",
		waiting: "warn",
		done: "ok",
	};
	const BUDGET_TONE: Record<string, DotTone> = { over: "fail", warn: "warn" };

	onMount(() => () => (machine.open = false));
</script>

{#snippet bar(title: string, g: G)}
	<div class="gauge" title={g.over ? "Past the heavy gate: new heavy jobs wait" : undefined}>
		<span class="gname">{title}</span>
		<div class="track" class:over={g.over}>
			{#if g.value != null}<span class="fill" style:transform="scaleX({g.value})"></span>{/if}
			{#if g.limit != null}<span class="limit" style:left="{g.limit * 100}%"></span>{/if}
		</div>
		<span class="glabel">{g.label}</span>
	</div>
{/snippet}

<DrawerShell bind:open={machine.open} title="Machine" width="440px">
	{#if !r}
		<div class="empty">
			<EmptyState
				icon={Gauge}
				title={machine.failed ? "Machine report unavailable" : "Reading the machine…"}
				description={machine.failed
					? "Atlas could not read the load, queue or terminals. It retries every few seconds."
					: ""}
			/>
		</div>
	{:else}
		<div class="body">
			<section aria-label="Load and memory">
				<div class="gl">
					Load and memory{r.gates.holding ? ` · heavy jobs wait on ${r.gates.holding}` : ""}
				</div>
				{@render bar("Load", gauge.load(r))}
				{@render bar("Memory", gauge.memory(r))}
			</section>

			<section aria-label="Heavy jobs">
				<div class="gl">Heavy jobs · {summary(r.heavy)}</div>
				{#each jobs as j (j.id)}
					{@const d = describe(j)}
					<DrawerRow
						tone={j.state === "running" ? "run" : "off"}
						pulse={j.state === "running"}
						disabled
						title={j.command}
					>
						<span class="title">{d.title}</span>
						<span class="detail">{d.detail}</span>
					</DrawerRow>
				{:else}
					<div class="none">Nothing running or waiting.</div>
				{/each}
			</section>

			<section aria-label="Agents and terminals">
				<div class="gl">Agents and terminals</div>
				{#each rows as a (a.pty)}
					<DrawerRow
						tone={AGENT_TONE[a.state ?? ""] ?? "off"}
						pulse={a.state === "working"}
						disabled
						title={a.detail}
					>
						<span class="title">{a.title}</span>
						<span class="detail">{a.state ?? a.detail}</span>
						<span class="num mono"
							>{a.cpu != null ? `${Math.round(a.cpu)}%` : "–"} · {a.mem != null ? bytes(a.mem) : "–"}</span
						>
					</DrawerRow>
				{:else}
					<div class="none">No terminals running.</div>
				{/each}
			</section>

			<section aria-label="Services by memory">
				<div class="gl">Services by memory</div>
				{#each r.services as s (s.project + "/" + s.name)}
					{@const note = serviceNote(s)}
					<DrawerRow tone={BUDGET_TONE[s.budget_state] ?? "ok"} disabled title={note?.text}>
						<span class="title">{s.name}</span>
						<span class="detail">{note?.text ?? s.project}</span>
						<span class="num mono">{s.memory_rss != null ? bytes(s.memory_rss) : "–"}</span>
					</DrawerRow>
				{:else}
					<div class="none">No service is running.</div>
				{/each}
			</section>

			<section aria-label="Heavy run history">
				<div class="gl">Heavy run history</div>
				{#each r.history as h (h.label)}
					{@const t = runTrend(h.secs)}
					<DrawerRow
						tone={t?.kind === "slower" ? "warn" : "off"}
						disabled
						title={h.secs.map(human).join(", ")}
					>
						<span class="title">{h.label}</span>
						<span class="detail">
							{h.secs.length} run{h.secs.length === 1 ? "" : "s"} · avg {human(h.avg_s)} · max {human(
								h.max_s,
							)}{t && t.kind !== "steady" ? ` · ${t.pct}% ${t.kind}` : ""}
						</span>
						<span class="num mono">{human(h.last_s)}</span>
					</DrawerRow>
				{:else}
					<div class="none">No heavy run recorded yet.</div>
				{/each}
			</section>

			<section aria-label="Orphans and disk">
				<div class="gl">Orphans and disk</div>
				{#if hygiene.available}
					<DrawerRow tone={hygiene.orphans ? "warn" : "ok"} disabled>
						<span class="title">{hygiene.orphans} orphaned process{hygiene.orphans === 1 ? "" : "es"}</span>
						<span class="detail">{hygiene.live} live · {bytes(hygiene.freeable)} to free</span>
					</DrawerRow>
					{#each hygiene.disk as d (d.name)}
						<DrawerRow tone="off" disabled>
							<span class="title">{d.name}</span>
							<span class="num mono">{bytes(d.bytes)}</span>
						</DrawerRow>
					{/each}
				{:else}
					<div class="none">Not available in this version.</div>
				{/if}
			</section>
		</div>
	{/if}
</DrawerShell>

<style>
	.body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		margin: 0 -8px;
		padding: 0 8px 8px;
	}
	.gl {
		position: sticky;
		top: 0;
		z-index: 1;
		padding: 8px 6px 3px;
		font-size: 10.5px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
		background: var(--ca-surface-overlay);
	}
	.gauge {
		display: grid;
		grid-template-columns: 52px 1fr auto;
		align-items: center;
		gap: 8px;
		height: 28px;
		padding: 0 6px;
		font-size: 12.5px;
	}
	.track {
		position: relative;
		height: 6px;
		border-radius: 3px;
		background: var(--ca-surface-raised);
		overflow: hidden;
	}
	/* scaleX, not width: a width transition would relayout on every sample. */
	.fill {
		position: absolute;
		inset: 0;
		border-radius: 3px;
		background: var(--space-accent, var(--ca-brand-main));
		transform-origin: left;
		transition: transform 300ms ease-out;
	}
	.over .fill {
		background: var(--ca-semantic-error);
	}
	.limit {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 2px;
		background: var(--ca-text-secondary);
	}
	.gname,
	.glabel,
	.detail,
	.num,
	.none {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.title {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.detail {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.num {
		margin-left: auto;
		flex: none;
	}
	.none {
		padding: 4px 6px;
	}
	.empty {
		flex: 1;
		display: grid;
		place-items: center;
	}
</style>
