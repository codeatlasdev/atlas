<script lang="ts">
	import { Sparkline } from "@codeatlasdev/ui/charts";
	import { fmtMem } from "#lib/format.ts";
	import type { Service } from "#lib/stores/projects.svelte.ts";
	import { metrics } from "./metrics.svelte.ts";
	import { memoryNote } from "./memoryFacts.ts";

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

	let note = $derived(memoryNote(service));

	const SPARK_POINTS = 40;
	const WINDOW_MIN = Math.round((SPARK_POINTS * 3) / 60);

	// Re-read the rings once per published revision; the arrays are small (40 numbers).
	let cpu = $derived.by(() => {
		void metrics.rev;
		return metrics.service(projectId, service.name).cpu.last(SPARK_POINTS, metrics.end(projectId));
	});
	let mem = $derived.by(() => {
		void metrics.rev;
		return metrics.service(projectId, service.name).mem.last(SPARK_POINTS, metrics.end(projectId));
	});
</script>

<div class="spark-wrap">
	<div class="spark">
		<div class="sp" title="CPU of the process tree, last {WINDOW_MIN} min">
			<Sparkline data={cpu} height="30px" color={colors.cpu} />
			<span class="val mono">{service.cpu ?? 0}%</span>
		</div>
		<div class="sp" title="Memory of the process tree, last {WINDOW_MIN} min">
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
