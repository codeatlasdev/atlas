<script lang="ts">
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

<div class="tl" role="img" aria-label="Service state over the last {Math.round(SPAN / 60000)} minutes">
	{#each lanes as lane (lane.name)}
		<button class="lane" onclick={() => onpick?.(lane.name)} title="{lane.name}: open logs">
			<span class="label">{lane.name}</span>
			<span class="track">
				{#each lane.segments as s, i (i)}
					{@const b = box(s)}
					{#if s.band !== "stopped" && b.left < 100}
						<span
							class="band b-{s.band}"
							class:open={s.end === null}
							style:left="{b.left}%"
							style:width="{Math.min(b.width, 100 - b.left)}%"
						></span>
					{/if}
				{/each}
			</span>
		</button>
	{/each}
	<div class="axis"><span>{Math.round(SPAN / 60000)} min ago</span><span>now</span></div>
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
	.band {
		position: absolute;
		top: 0;
		bottom: 0;
		transition: width 400ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.b-running {
		background: var(--ca-semantic-success);
	}
	.b-starting {
		background: var(--ca-semantic-warning);
	}
	.b-failed {
		background: var(--ca-semantic-error);
	}
	.axis {
		display: flex;
		justify-content: space-between;
		margin-left: calc(min(max(64px, 15%), 120px) + 10px);
		font-size: 10.5px;
		color: var(--ca-text-disabled);
	}
	@media (prefers-reduced-motion: reduce) {
		.band {
			transition: none;
		}
	}
</style>
