<script lang="ts">
	import { Sparkline } from "@codeatlasdev/ui/charts";
	import "./chartMotion.ts";
	import { fmtMem } from "#lib/format.ts";
	import type { Service } from "#lib/stores/projects.svelte.ts";
	import { metrics } from "./metrics.svelte.ts";
	import { liveRev } from "./liveRev.svelte.ts";
	import { sameSeries } from "./metrics.ts";
	import { memoryNote } from "./memoryFacts.ts";
	import { m } from "#lib/i18n/m.ts";

	/** CPU and memory sparklines of one service with their current values. */
	let {
		projectId,
		service,
		colors,
	}: {
		projectId: string;
		service: Service;
		/** Canvas-safe `#rrggbb` colors, resolved once by the grid. */
		colors: { cpu: string; mem: string };
	} = $props();

	const revision = liveRev();
	let note = $derived(memoryNote(service));

	const SPARK_POINTS = 40;
	const WINDOW_MIN = Math.round((SPARK_POINTS * 3) / 60);

	// The name as its own derived: the runner re-sends every row (a new `service` object) between samples, and a
	// derivation that read `service.name` itself re-ran for each one and drew the same sparkline again.
	let name = $derived(service.name);
	// Re-read the rings once per revision (published at most once a second, and only while the panel is on screen).
	// A flat history (an idle service: 0 % and a steady memory) reads the same numbers every time: hand back the array the
	// chart already drew, so the sparkline is not painted again for a picture that has not changed.
	const drawn: Record<"cpu" | "mem", number[]> = { cpu: [], mem: [] };
	const read = (k: "cpu" | "mem") => {
		const next = metrics.service(projectId, name)[k].last(SPARK_POINTS, metrics.end(projectId));
		return sameSeries(drawn[k], next) ? drawn[k] : (drawn[k] = next);
	};
	let cpu = $derived.by(() => (void revision.rev, read("cpu")));
	let mem = $derived.by(() => (void revision.rev, read("mem")));
</script>

<div class="spark-wrap">
	<div class="spark">
		<div class="sp" title={m.panel_card_spark_cpu({ minutes: WINDOW_MIN })}>
			<Sparkline data={cpu} height="30px" color={colors.cpu} />
			<span class="val mono">{service.cpu ?? 0}%</span>
		</div>
		<div class="sp" title={m.panel_card_spark_memory({ minutes: WINDOW_MIN })}>
			<Sparkline data={mem} height="30px" color={colors.mem} />
			<span class="val mono" class:heavy={(service.mem_mb ?? 0) >= 2048}>{fmtMem(service.mem_mb ?? 0)}</span>
		</div>
	</div>
	{#if note}
		<div class="note note-{note.tone}" title={note.title}>{note.text}</div>
	{/if}
</div>

<style>
	.spark-wrap {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}
	.note {
		font-size: 11px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.note-warn {
		color: var(--ca-semantic-warning);
	}
	.note-error {
		color: var(--ca-semantic-error);
	}
	.spark {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px;
	}
	.sp {
		position: relative;
		min-width: 0;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		overflow: hidden;
	}
	.val {
		position: absolute;
		right: 6px;
		top: 3px;
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	.val.heavy {
		color: var(--ca-semantic-warning);
	}
</style>
