<script lang="ts">
	import PaneTree from "./PaneTree.svelte";
	import TabGroupView from "./TabGroupView.svelte";
	import { ws, type Layout } from "#lib/stores/workspace.svelte.ts";

	let { layout, worktreeId, path = [] }: { layout: Layout; worktreeId: string; path?: number[] } = $props();
	let box: HTMLDivElement | undefined = $state();

	function drag(e: PointerEvent) {
		if (layout.type !== "split" || !box) return;
		const r = box.getBoundingClientRect();
		const dir = layout.dir;
		const target = e.currentTarget as HTMLElement;
		target.setPointerCapture(e.pointerId);
		const move = (ev: PointerEvent) => {
			const ratio = dir === "h" ? (ev.clientX - r.left) / r.width : (ev.clientY - r.top) / r.height;
			ws.setRatio(path, worktreeId, ratio);
		};
		const up = () => {
			target.removeEventListener("pointermove", move);
			target.removeEventListener("pointerup", up);
		};
		target.addEventListener("pointermove", move);
		target.addEventListener("pointerup", up);
	}
</script>

{#if layout.type === "leaf"}
	<TabGroupView groupId={layout.group} />
{:else}
	<div class="split" class:v={layout.dir === "v"} bind:this={box}>
		<div class="part" style:flex-basis="{layout.ratio * 100}%">
			<PaneTree layout={layout.first} {worktreeId} path={[...path, 0]} />
		</div>
		<div class="divider" role="separator" onpointerdown={drag}></div>
		<div class="part" style:flex-basis="{(1 - layout.ratio) * 100}%">
			<PaneTree layout={layout.second} {worktreeId} path={[...path, 1]} />
		</div>
	</div>
{/if}

<style>
	.split {
		display: flex;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.split.v {
		flex-direction: column;
	}
	.part {
		display: flex;
		min-width: 0;
		min-height: 0;
		flex-grow: 0;
		flex-shrink: 1;
	}
	.divider {
		flex: 0 0 5px;
		cursor: col-resize;
		background: linear-gradient(var(--ca-outline-subtle), var(--ca-outline-subtle)) center / 1px 100%
			no-repeat;
	}
	.split.v > .divider {
		cursor: row-resize;
		background: linear-gradient(var(--ca-outline-subtle), var(--ca-outline-subtle)) center / 100% 1px
			no-repeat;
	}
	.divider:hover {
		background-color: color-mix(in oklab, var(--ca-brand-main) 30%, transparent);
	}
</style>
