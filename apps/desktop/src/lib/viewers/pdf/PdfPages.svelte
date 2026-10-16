<script lang="ts">
	import { PAGE_PAD, type Layout } from "#lib/viewers/pdf.ts";

	/** The scrolling column of virtualized page sheets (canvas + selectable text layer) and pinch transform. */
	let {
		scroller = $bindable(),
		layout,
		indices,
		scale,
		pinch,
		label,
		slot,
		onscroll,
		onwheel,
		ondblclick,
		onpointermove,
	}: {
		scroller: HTMLDivElement | undefined;
		layout: Layout;
		/** Page indices currently mounted (the prefetch window). */
		indices: number[];
		scale: number;
		pinch: { k: number; ox: number; oy: number };
		label: string;
		/** Svelte action binding a page element to the renderer. */
		slot: (node: HTMLElement, i: number) => { destroy: () => void };
		onscroll: () => void;
		onwheel: (e: WheelEvent) => void;
		ondblclick: (e: MouseEvent) => void;
		onpointermove: () => void;
	} = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
	class="scroller ca-selectable"
	bind:this={scroller}
	{onscroll}
	{onwheel}
	{ondblclick}
	{onpointermove}
	tabindex="0"
	role="document"
	aria-label={label}
>
	<div
		class="content"
		style:width="{layout.width + PAGE_PAD * 2}px"
		style:height="{layout.height}px"
		style:transform={pinch.k !== 1 ? `scale(${pinch.k})` : undefined}
		style:transform-origin="{pinch.ox}px {pinch.oy}px"
	>
		{#each indices as i (i)}
			{@const sz = layout.sizes[i]}
			<div
				class="page"
				use:slot={i}
				style:top="{layout.tops[i]}px"
				style:width="{sz.w}px"
				style:height="{sz.h}px"
				style:left="max({PAGE_PAD}px, calc((100% - {sz.w}px) / 2))"
				style:--total-scale-factor={scale}
			>
				<canvas></canvas>
				<div class="tl textLayer"></div>
			</div>
		{/each}
	</div>
</div>

<style>
	.scroller {
		flex: 1;
		min-width: 0;
		overflow: auto;
		position: relative;
		outline: none;
		overscroll-behavior: contain;
		scrollbar-width: thin;
	}
	.content {
		position: relative;
		min-width: 100%;
		/* Pinch scales this layer live; the crisp re-render lands after the gesture settles. */
		will-change: transform;
	}
	.page {
		position: absolute;
		background: white;
		border-radius: 2px;
		box-shadow: var(--pdf-shadow);
		overflow: hidden;
		contain: layout paint;
	}
	.page canvas {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
	}
	/* pdf.js text layer (mirrors pdfjs-dist/web/pdf_viewer.css): transparent glyphs over the canvas for
	   selection/copy. `--min-font-size` is set by TextLayer; sizes derive from `--total-scale-factor`. */
	.tl {
		position: absolute;
		inset: 0;
		overflow: clip;
		line-height: 1;
		letter-spacing: normal;
		word-spacing: normal;
		text-size-adjust: none;
		forced-color-adjust: none;
		transform-origin: 0 0;
		caret-color: CanvasText;
		z-index: 1;
		--min-font-size: 1;
		--text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
		--min-font-size-inv: calc(1 / var(--min-font-size));
		--scale-round-x: 1px;
		--scale-round-y: 1px;
	}
	.tl :global(:is(span, br)) {
		color: transparent;
		position: absolute;
		white-space: pre;
		cursor: text;
		transform-origin: 0% 0%;
		user-select: text;
	}
	.tl > :global(:not(.markedContent)),
	.tl :global(.markedContent span:not(.markedContent)) {
		z-index: 1;
		--font-height: 0;
		font-size: calc(var(--text-scale-factor) * var(--font-height));
		--scale-x: 1;
		--rotate: 0deg;
		transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
	}
	.tl :global(.markedContent) {
		display: contents;
	}
	.tl :global(span)::selection {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 40%, transparent);
	}
	.tl :global(mark.hit) {
		background: var(--cm-search-match, color-mix(in oklab, var(--ca-semantic-warning) 35%, transparent));
		color: transparent;
		border-radius: 2px;
	}
	.tl :global(mark.hit.cur) {
		background: var(
			--cm-search-match-active,
			color-mix(in oklab, var(--ca-semantic-warning) 65%, transparent)
		);
		outline: 1.5px solid var(--ca-semantic-warning);
	}
</style>
