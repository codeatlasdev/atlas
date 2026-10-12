<script lang="ts">
	import { clampSize, DOCK_DEFAULTS, type DockSide } from "#lib/docksize.ts";
	import { docks } from "#lib/stores/docks.svelte.ts";

	/**
	 * `side`: which dock's size this handle edits. `edge`: which window edge the dock is
	 * anchored to (the project panel is the "left" dock but sits on the right when
	 * `project_panel.dock` is "right").
	 */
	let { side, edge = side }: { side: DockSide; edge?: DockSide } = $props();

	const horizontal = $derived(side === "bottom");
	const key = $derived(`${side}Size` as const);
	let dragging = $state(false);

	/** Pointer position → dock size, measured against the dock's container. */
	function drag(e: PointerEvent) {
		const el = e.currentTarget as HTMLElement;
		const box = el.parentElement!.getBoundingClientRect();
		el.setPointerCapture(e.pointerId);
		dragging = true;
		const move = (ev: PointerEvent) => {
			const raw =
				edge === "left"
					? ev.clientX - box.left
					: edge === "right"
						? box.right - ev.clientX
						: box.bottom - ev.clientY;
			const room = horizontal ? box.height - 120 : box.width - 360;
			docks[key] = clampSize(side, raw, room);
		};
		const up = () => {
			dragging = false;
			el.removeEventListener("pointermove", move);
			el.removeEventListener("pointerup", up);
			el.removeEventListener("pointercancel", up);
		};
		el.addEventListener("pointermove", move);
		el.addEventListener("pointerup", up);
		el.addEventListener("pointercancel", up);
	}

	/** Keyboard: arrows nudge, so the handle is usable without a pointer. */
	function nudge(e: KeyboardEvent) {
		const grow = edge === "left" || edge === "bottom" ? "ArrowRight" : "ArrowLeft";
		const shrink = edge === "left" || edge === "bottom" ? "ArrowLeft" : "ArrowRight";
		const up = edge === "bottom" ? "ArrowUp" : grow;
		const down = edge === "bottom" ? "ArrowDown" : shrink;
		const step = e.shiftKey ? 40 : 10;
		if (e.key === up) docks[key] = clampSize(side, docks[key] + step);
		else if (e.key === down) docks[key] = clampSize(side, docks[key] - step);
		else return;
		e.preventDefault();
	}
</script>

<!-- A focusable separator is the WAI-ARIA "window splitter" widget: keyboard + pointer are intended. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
	class="resizer {edge}"
	class:dragging
	role="separator"
	aria-orientation={horizontal ? "horizontal" : "vertical"}
	aria-label="Resize {side === 'bottom' ? 'bottom' : 'side'} dock"
	aria-valuenow={docks[key]}
	tabindex="0"
	onpointerdown={drag}
	onkeydown={nudge}
	ondblclick={() => (docks[key] = DOCK_DEFAULTS[side])}
></div>

<style>
	.resizer {
		position: relative;
		flex: 0 0 5px;
		z-index: 2;
		touch-action: none;
	}
	.resizer.left,
	.resizer.right {
		cursor: col-resize;
	}
	.resizer.bottom {
		cursor: row-resize;
	}
	/* The visible hairline; widens to a brand tint on hover/drag. */
	.resizer::after {
		content: "";
		position: absolute;
		background: var(--ca-outline-subtle);
		transition:
			background 120ms ease-out,
			transform 120ms ease-out;
	}
	.resizer.left::after,
	.resizer.right::after {
		top: 0;
		bottom: 0;
		left: 2px;
		width: 1px;
	}
	.resizer.bottom::after {
		left: 0;
		right: 0;
		top: 2px;
		height: 1px;
	}
	.resizer:hover::after,
	.resizer.dragging::after {
		background: var(--ca-brand-main);
	}
	.resizer.left:hover::after,
	.resizer.right:hover::after,
	.resizer.left.dragging::after,
	.resizer.right.dragging::after {
		transform: scaleX(3);
	}
	.resizer.bottom:hover::after,
	.resizer.bottom.dragging::after {
		transform: scaleY(3);
	}
</style>
