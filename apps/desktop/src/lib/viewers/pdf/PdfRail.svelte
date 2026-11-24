<script lang="ts">
	import { ChevronRight } from "@lucide/svelte";
	import type { PageSize } from "#lib/viewers/pdf.ts";
	import type { Outline, PdfDoc } from "#lib/viewers/pdf/doc.ts";
	import { m } from "#lib/i18n/m.ts";

	/** Sidebar: page thumbnails (lazy, painted when scrolled into view) or the document outline. */
	let {
		doc,
		pages,
		page,
		outline,
		goto,
	}: {
		doc: PdfDoc;
		pages: PageSize[];
		page: number;
		outline: Outline[];
		goto: (i: number, smooth?: boolean) => void;
	} = $props();

	const THUMB_W = 96;

	let rail: HTMLElement | undefined = $state();
	let sideTab = $state<"thumbs" | "outline">("thumbs");
	const thumbs = new Map<number, HTMLCanvasElement>();

	/** Repaints the thumbnail of page `i` if it is mounted (called when the main page bitmap lands). */
	export async function paintThumb(i: number) {
		const c = thumbs.get(i);
		if (!c || !pages[i]) return;
		const pg = await doc.page(i);
		const dpr = window.devicePixelRatio || 1;
		const vp = pg.getViewport({ scale: (THUMB_W / pages[i].w) * dpr });
		c.width = Math.floor(vp.width);
		c.height = Math.floor(vp.height);
		await pg.render({ canvasContext: c.getContext("2d")!, canvas: c, viewport: vp }).promise.catch(() => {});
	}

	/** Svelte action: queue a thumbnail render when its rail item scrolls into view. */
	function thumb(node: HTMLCanvasElement, i: number) {
		thumbs.set(i, node);
		const io = new IntersectionObserver(
			([e]) => {
				if (e.isIntersecting) {
					void paintThumb(i);
					io.disconnect();
				}
			},
			{ root: rail, rootMargin: "200px" },
		);
		io.observe(node);
		return {
			destroy() {
				io.disconnect();
				thumbs.delete(i);
			},
		};
	}

	// Keep the current page's thumbnail in view.
	$effect(() => {
		rail?.querySelector<HTMLElement>(`[data-thumb="${page}"]`)?.scrollIntoView({ block: "nearest" });
	});
</script>

<aside class="rail" bind:this={rail} aria-label={m.viewers_files_pdf_rail_pages()}>
	{#if outline.length}
		<div class="tabs">
			<button class:on={sideTab === "thumbs"} onclick={() => (sideTab = "thumbs")}>
				{m.viewers_files_pdf_rail_pages()}
			</button>
			<button class:on={sideTab === "outline"} onclick={() => (sideTab = "outline")}>
				{m.viewers_files_pdf_rail_outline()}
			</button>
		</div>
	{/if}
	{#if sideTab === "outline" && outline.length}
		<div class="outline">
			{#snippet node(items: Outline[], depth: number)}
				{#each items as it}
					<button
						class="ol"
						style:padding-left="{8 + depth * 12}px"
						disabled={it.page === null}
						onclick={() => it.page !== null && goto(it.page, true)}
					>
						{#if it.items.length}<ChevronRight size={10} />{:else}<span class="dot"></span>{/if}
						<span class="t">{it.title}</span>
					</button>
					{#if it.items.length}{@render node(it.items, depth + 1)}{/if}
				{/each}
			{/snippet}
			{@render node(outline, 0)}
		</div>
	{:else}
		{#each pages as p, i (i)}
			<button
				class="thumb"
				class:cur={i === page}
				data-thumb={i}
				onclick={() => goto(i)}
				aria-label={m.viewers_files_pdf_rail_page_n({ page: i + 1 })}
			>
				<canvas use:thumb={i} style:width="{THUMB_W}px" style:aspect-ratio="{p.w} / {p.h}"></canvas>
				<span>{i + 1}</span>
			</button>
		{/each}
	{/if}
</aside>

<style>
	.rail {
		width: 132px;
		flex-shrink: 0;
		overflow-y: auto;
		padding: 10px 0;
		border-right: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 3%, transparent);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		scrollbar-width: thin;
	}
	.rail:has(.outline) {
		align-items: stretch;
	}
	.thumb {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 5px;
		border: 0;
		border-radius: 8px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 10.5px;
	}
	.thumb canvas {
		background: white;
		border-radius: 2px;
		box-shadow: 0 1px 4px color-mix(in oklab, black 30%, transparent);
	}
	.thumb:hover {
		background: var(--ca-action-hover);
	}
	.thumb.cur {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 18%, transparent);
		color: var(--ca-text-primary);
	}
	.thumb.cur canvas {
		outline: 1.5px solid var(--space-accent, var(--ca-brand-main));
	}
	.tabs {
		display: flex;
		gap: 2px;
		padding: 0 8px;
		align-self: stretch;
	}
	.tabs button {
		flex: 1;
		height: 22px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
	.tabs button.on {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.outline {
		display: flex;
		flex-direction: column;
		align-self: stretch;
		padding: 0 4px;
	}
	.ol {
		display: flex;
		align-items: center;
		gap: 4px;
		min-height: 22px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 11px;
		text-align: left;
	}
	.ol:hover:not(:disabled) {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.ol .t {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ol .dot {
		width: 10px;
		flex-shrink: 0;
	}
</style>
