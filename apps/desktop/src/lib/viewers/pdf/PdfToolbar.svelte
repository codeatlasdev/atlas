<script lang="ts">
	import { kbd } from "#lib/keys.ts";
	import { tick } from "svelte";
	import {
		ZoomIn,
		ZoomOut,
		Maximize2,
		PanelLeft,
		Search,
		ChevronUp,
		ChevronDown,
		X,
		ExternalLink,
		FolderSearch,
	} from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { parsePage, TOOLBAR_IDLE_MS } from "#lib/viewers/pdf.ts";
	import type { PdfSearch } from "#lib/viewers/pdf/search.svelte.ts";

	/** Floating glass pill: sidebar toggle, page box, zoom, find, open/reveal. Fades with idle. */
	let {
		search,
		sidebar = $bindable(),
		page,
		total,
		pct,
		zoom,
		goto,
		zoomStep,
		zoomTo,
		toggleFit,
		onclose,
		onopen,
		onreveal,
	}: {
		search: PdfSearch;
		sidebar: boolean;
		page: number;
		total: number;
		pct: number;
		zoom: number | "width" | "page";
		goto: (i: number) => void;
		zoomStep: (dir: 1 | -1) => void;
		zoomTo: (z: "width") => void;
		toggleFit: () => void;
		/** Find closed: the viewer takes focus back. */
		onclose: () => void;
		onopen: () => void;
		onreveal: () => void;
	} = $props();

	let visible = $state(true);
	let hover = false;
	let timer = 0;
	let editingPage = $state(false);
	let pageDraft = $state("");
	let findInput: HTMLInputElement | undefined = $state();

	/** Shows the bar and (re)starts the idle countdown. */
	export function show() {
		visible = true;
		clearTimeout(timer);
		timer = window.setTimeout(() => {
			if (!hover && !search.open && !editingPage) visible = false;
		}, TOOLBAR_IDLE_MS);
	}

	export async function openFind() {
		search.open = true;
		visible = true;
		await tick();
		findInput?.focus();
		findInput?.select();
	}

	export function closeFind() {
		search.close();
		onclose();
		show();
	}

	export const dispose = () => clearTimeout(timer);

	function commitPage() {
		editingPage = false;
		const n = parsePage(pageDraft, total);
		if (n !== null) goto(n);
	}
</script>

<div
	class="bar"
	class:show={visible || search.open}
	onpointerenter={() => ((hover = true), (visible = true))}
	onpointerleave={() => ((hover = false), show())}
	role="toolbar"
	aria-label="PDF controls"
	tabindex="-1"
>
	{#if search.open}
		<div class="find">
			<Search size={13} />
			<input
				bind:this={findInput}
				bind:value={search.query}
				oninput={() => search.schedule()}
				placeholder="Find in PDF"
				spellcheck="false"
				onkeydown={(e) => {
					if (e.key === "Enter") (e.preventDefault(), search.next(e.shiftKey ? -1 : 1));
					else if (e.key === "Escape") (e.preventDefault(), closeFind());
					e.stopPropagation();
				}}
			/>
			<span class="count mono">
				{#if search.searching}…{:else if search.query.trim()}{search.matches.length
						? `${search.cur + 1} / ${search.matches.length}`
						: "0"}{/if}
			</span>
			<button aria-label="Previous match" disabled={!search.matches.length} onclick={() => search.next(-1)}
				><ChevronUp size={14} /></button
			>
			<button aria-label="Next match" disabled={!search.matches.length} onclick={() => search.next(1)}
				><ChevronDown size={14} /></button
			>
			<button aria-label="Close find" onclick={closeFind}><X size={14} /></button>
		</div>
	{:else}
		<Tip label="Pages & outline" side="top">
			{#snippet child(props)}
				<button {...props} class:on={sidebar} aria-label="Toggle sidebar" onclick={() => (sidebar = !sidebar)}
					><PanelLeft size={14} /></button
				>
			{/snippet}
		</Tip>
		<span class="sep"></span>
		<div class="pageno mono">
			{#if editingPage}
				<!-- svelte-ignore a11y_autofocus -->
				<input
					autofocus
					size="3"
					bind:value={pageDraft}
					onblur={commitPage}
					onkeydown={(e) => {
						if (e.key === "Enter") e.currentTarget.blur();
						else if (e.key === "Escape") ((editingPage = false), e.stopPropagation());
						e.stopPropagation();
					}}
					onfocus={(e) => e.currentTarget.select()}
				/>
			{:else}
				<button
					class="num"
					title="Go to page"
					onclick={() => ((pageDraft = String(page + 1)), (editingPage = true))}>{page + 1}</button
				>
			{/if}
			<span class="of">/ {total}</span>
		</div>
		<span class="sep"></span>
		<Tip label="Zoom out" command="" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Zoom out" onclick={() => zoomStep(-1)}><ZoomOut size={14} /></button>
			{/snippet}
		</Tip>
		<button class="pct mono" title="Fit width" onclick={() => zoomTo("width")}>{pct}%</button>
		<Tip label="Zoom in" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Zoom in" onclick={() => zoomStep(1)}><ZoomIn size={14} /></button>
			{/snippet}
		</Tip>
		<Tip label={zoom === "page" ? "Fit width" : "Fit page"} side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Toggle fit" class:on={typeof zoom === "string"} onclick={toggleFit}
					><Maximize2 size={14} /></button
				>
			{/snippet}
		</Tip>
		<span class="sep"></span>
		<Tip label={`Find  ${kbd("cmd-f")}`} side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Find" onclick={openFind}><Search size={14} /></button>
			{/snippet}
		</Tip>
		<Tip label="Open externally" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Open externally" onclick={onopen}><ExternalLink size={14} /></button>
			{/snippet}
		</Tip>
		<Tip label="Reveal in Finder" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Reveal in Finder" onclick={onreveal}><FolderSearch size={14} /></button
				>
			{/snippet}
		</Tip>
	{/if}
</div>

<style>
	/* Floating glass pill, bottom-center; fades with idle. */
	.bar {
		position: absolute;
		left: 50%;
		bottom: 16px;
		transform: translate(-50%, 8px);
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 4px 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 999px;
		background: color-mix(in oklab, var(--ca-surface-raised) 78%, transparent);
		backdrop-filter: blur(18px) saturate(1.6);
		-webkit-backdrop-filter: blur(18px) saturate(1.6);
		box-shadow: 0 8px 28px color-mix(in oklab, black 28%, transparent);
		color: var(--ca-text-secondary);
		font-size: 11.5px;
		opacity: 0;
		pointer-events: none;
		transition:
			opacity 200ms ease-out,
			transform 200ms ease-out;
		z-index: 5;
	}
	.bar.show {
		opacity: 1;
		pointer-events: auto;
		transform: translate(-50%, 0);
	}
	.bar button {
		display: inline-grid;
		place-items: center;
		min-width: 26px;
		height: 26px;
		padding: 0 4px;
		border: 0;
		border-radius: 999px;
		background: none;
		color: inherit;
		transition: background 120ms ease-out;
	}
	.bar button:hover:not(:disabled) {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.bar button:disabled {
		color: var(--ca-text-disabled);
	}
	.bar button.on {
		color: var(--space-accent-text, var(--ca-brand-main));
	}
	.sep {
		width: 0.5px;
		height: 14px;
		margin: 0 4px;
		background: var(--ca-outline-subtle);
	}
	.mono {
		font-family: var(--mono);
	}
	.pageno {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
		color: var(--ca-text-primary);
	}
	.pageno .num {
		min-width: 22px;
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.pageno .of {
		color: var(--ca-text-secondary);
	}
	.pageno input,
	.find input {
		border: 0;
		outline: none;
		background: color-mix(in oklab, var(--ca-text-primary) 10%, transparent);
		color: var(--ca-text-primary);
		border-radius: 6px;
		font: inherit;
		font-family: var(--mono);
		text-align: center;
		height: 22px;
		width: 36px;
	}
	.pct {
		min-width: 46px !important;
		font-size: 11.5px;
	}
	.find {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
	}
	.find input {
		width: 200px;
		padding: 0 8px;
		text-align: left;
		font-family: inherit;
	}
	.count {
		min-width: 48px;
		text-align: center;
		font-size: 11px;
	}
	@media (prefers-reduced-motion: reduce) {
		.bar {
			transition: none;
		}
	}
</style>
