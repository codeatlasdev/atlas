<script lang="ts">
	import { BarChart, LineChart } from "@codeatlasdev/ui/charts";
	import { fmtMem } from "#lib/format.ts";
	import { metrics } from "./metrics.svelte.ts";
	import { slotLabels } from "./metrics.ts";

	let { projectId, names }: { projectId: string; names: string[] } = $props();

	/** Points per chart: 5 min at 3 s. */
	const N = 100;
	let tab = $state<"mem" | "cpu" | "errors">("mem");

	// One derived per tick (`rev` is throttled to 1/s): the arrays are N numbers per series.
	let model = $derived.by(() => {
		void metrics.rev;
		const end = metrics.end(projectId);
		const categories = slotLabels(N, end);
		const top = names
			.map((name) => ({ name, h: metrics.service(projectId, name) }))
			.map((s) => ({ ...s, peak: Math.max(...s.h.mem.last(N, end), ...s.h.cpu.last(N, end)) }))
			.sort((a, b) => b.peak - a.peak)
			.slice(0, 6);
		const pick = (k: "mem" | "cpu" | "errors") =>
			top.map((s) => ({ name: s.name, data: s.h[k].last(N, end) }));
		return { categories, mem: pick("mem"), cpu: pick("cpu"), errors: pick("errors") };
	});

	const TABS = [
		{ id: "mem", label: "Memory" },
		{ id: "cpu", label: "CPU" },
		{ id: "errors", label: "Errors" },
	] as const;
</script>

<div class="charts">
	<div class="seg" role="tablist" aria-label="Metric">
		{#each TABS as t (t.id)}
			<button role="tab" aria-selected={tab === t.id} class:on={tab === t.id} onclick={() => (tab = t.id)}
				>{t.label}</button
			>
		{/each}
	</div>
	{#if tab === "errors"}
		<BarChart
			series={model.errors}
			categories={model.categories}
			stacked
			valueFormatter={(v) => `${v} error${v === 1 ? "" : "s"}`}
			height="190px"
		/>
	{:else if tab === "mem"}
		<LineChart
			series={model.mem}
			categories={model.categories}
			area
			valueFormatter={(v) => fmtMem(Math.round(v))}
			height="190px"
		/>
	{:else}
		<LineChart
			series={model.cpu}
			categories={model.categories}
			area
			valueFormatter={(v) => `${Math.round(v)}%`}
			height="190px"
		/>
	{/if}
</div>

<style>
	.charts {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.seg {
		align-self: flex-start;
		display: inline-flex;
		gap: 1px;
		padding: 2px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
	}
	.seg button {
		border: 0;
		padding: 3px 10px;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.seg button:hover {
		color: var(--ca-text-primary);
	}
	.seg button.on {
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
</style>
