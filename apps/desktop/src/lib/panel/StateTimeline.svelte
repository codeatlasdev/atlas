<script lang="ts">
	import { relative } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";
	import { SLOTS, STEP_MS, type Segment } from "./metrics.ts";

	/** One lane per service: contiguous state bands across the history window. */
	let {
		lanes,
		end,
		onpick,
	}: {
		lanes: { name: string; segments: Segment[] }[];
		/** Right edge of the window, ms. */
		end: number;
		onpick?: (name: string) => void;
	} = $props();

	const SPAN = SLOTS * STEP_MS;
	let start = $derived(end - SPAN);

	/** CSS left/width (%) of a segment clipped to the window. */
	function box(s: Segment) {
		const a = Math.max(s.start, start);
		const b = Math.min(s.end ?? end, end);
		return { left: ((a - start) / SPAN) * 100, width: Math.max(0.4, ((b - a) / SPAN) * 100) };
	}
</script>

<div class="tl" role="img" aria-label={m.panel_state_timeline_label({ minutes: Math.round(SPAN / 60000) })}>
	{#each lanes as lane (lane.name)}
		<button
			class="lane"
			onclick={() => onpick?.(lane.name)}
			title={m.panel_state_timeline_open_logs({ name: lane.name })}
		>
			<span class="label">{lane.name}</span>
			<span class="track">
				{#each lane.segments as s, i (i)}
					{@const b = box(s)}
					{#if s.band !== "stopped" && b.left < 100}
						<span
							class="band b-{s.band}"
							class:open={s.end === null}
							style:--x={b.left / 100}
							style:--w={Math.min(b.width, 100 - b.left) / 100}
						></span>
					{/if}
				{/each}
			</span>
		</button>
	{/each}
	<div class="axis">
		<span>{relative(-Math.round(SPAN / 60000), "minute")}</span><span>{relative(0, "second")}</span>
	</div>
</div>

<style>
	.tl {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.lane {
		display: grid;
		grid-template-columns: minmax(64px, 120px) 1fr;
		align-items: center;
		gap: 10px;
		border: 0;
		padding: 1px 0;
		background: none;
		border-radius: 4px;
		font: inherit;
		color: inherit;
		text-align: left;
	}
	.lane:hover .label {
		color: var(--ca-text-primary);
	}
	.label {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.track {
		position: relative;
		height: 10px;
		border-radius: 5px;
		background: var(--ca-surface-sunken);
		overflow: hidden;
	}
	/* A full-width bar placed and sized with transform (compositor only): `--x` is the start and `--w` the length as
	   fractions of the track. It moves in steps with no transition: the right edge advances every sample, and a
	   transition on every band made each one a running animation for ~40 % of the time, which kept WebKitGTK compositing
	   at 60 fps (4 points of CPU on an idle dashboard, docs/perf/linux-webkit-study.md). */
	.band {
		position: absolute;
		inset: 0;
		transform-origin: left;
		transform: translateX(calc(var(--x) * 100%)) scaleX(var(--w));
	}
	.b-running {
		background: var(--atlas-state-success, var(--ca-semantic-success));
	}
	.b-starting {
		background: var(--atlas-state-warning, var(--ca-semantic-warning));
	}
	.b-failed {
		background: var(--atlas-state-error, var(--ca-semantic-error));
	}
	.axis {
		display: flex;
		justify-content: space-between;
		margin-left: calc(min(max(64px, 15%), 120px) + 10px);
		font-size: 10.5px;
		color: var(--ca-text-secondary);
	}
</style>
