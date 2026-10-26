<script lang="ts">
	import { onMount } from "svelte";
	import { toast } from "@codeatlasdev/ui";
	import { ZoomIn, ZoomOut, Maximize, Copy } from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { formatBytes, extOf } from "#lib/viewers/kind.ts";
	import { clampZoom, fitScale, wheelZoomFactor, zoomAt } from "#lib/viewers/zoom.ts";

	let {
		url,
		blob,
		path,
		size,
		focused = false,
	}: { url: string; blob: Blob; path: string; size: number; focused?: boolean } = $props();

	let stage: HTMLDivElement | undefined = $state();
	let nat = $state({ w: 0, h: 0 });
	/** null = fit to view; number = explicit scale (1 = 100%). */
	let manual = $state<number | null>(null);
	let pan = $state({ x: 0, y: 0 });
	let box = $state({ w: 0, h: 0 });
	let dragging = $state(false);

	let fit = $derived(nat.w ? fitScale(nat, box) : 1);
	let scale = $derived(manual ?? fit);
	let pixelated = $derived(scale >= 2);

	onMount(() => {
		const ro = new ResizeObserver(([e]) => (box = { w: e.contentRect.width, h: e.contentRect.height }));
		ro.observe(stage!);
		return () => ro.disconnect();
	});

	function setScale(next: number, at?: { x: number; y: number }) {
		const cur = scale;
		next = clampZoom(next);
		if (Math.abs(next - fit) < 0.001) {
			manual = null;
			pan = { x: 0, y: 0 };
			return;
		}
		const c = at ?? { x: 0, y: 0 };
		pan = zoomAt(pan, c, cur, next);
		manual = next;
	}

	const reset = () => {
		manual = null;
		pan = { x: 0, y: 0 };
	};

	function center(e: { clientX: number; clientY: number }) {
		const r = stage!.getBoundingClientRect();
		return { x: e.clientX - r.left - r.width / 2, y: e.clientY - r.top - r.height / 2 };
	}

	function onwheel(e: WheelEvent) {
		// Trackpad pinch arrives as ctrlKey+wheel; ⌘+wheel zooms like Preview.
		if (e.ctrlKey || e.metaKey) {
			e.preventDefault();
			setScale(scale * wheelZoomFactor(e.deltaY, e.ctrlKey), center(e));
		} else if (manual !== null) {
			e.preventDefault();
			pan = { x: pan.x - e.deltaX, y: pan.y - e.deltaY };
		}
	}

	let drag: { x: number; y: number; px: number; py: number } | null = null;
	function onpointerdown(e: PointerEvent) {
		if (e.button !== 0 || manual === null) return;
		drag = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
		dragging = true;
		stage!.setPointerCapture(e.pointerId);
	}
	function onpointermove(e: PointerEvent) {
		if (drag) pan = { x: drag.px + e.clientX - drag.x, y: drag.py + e.clientY - drag.y };
	}
	function onpointerup() {
		drag = null;
		dragging = false;
	}

	// WebKit gesture events (macOS pinch) fire alongside wheel on some builds; the wheel path covers them.
	function onkeydown(e: KeyboardEvent) {
		if (!focused || !(e.metaKey || e.ctrlKey)) return;
		if (e.key === "=" || e.key === "+") (setScale(scale * 1.25), e.preventDefault());
		else if (e.key === "-") (setScale(scale / 1.25), e.preventDefault());
		else if (e.key === "0") (reset(), e.preventDefault());
		else if (e.key === "1") (setScale(1), e.preventDefault());
	}

	async function copyImage() {
		try {
			// The clipboard only takes PNG; re-encode through a canvas for other formats.
			let png = blob;
			if (blob.type !== "image/png") {
				const bmp = await createImageBitmap(blob);
				const c = new OffscreenCanvas(bmp.width, bmp.height);
				c.getContext("2d")!.drawImage(bmp, 0, 0);
				png = await c.convertToBlob({ type: "image/png" });
			}
			await navigator.clipboard.write([new ClipboardItem({ "image/png": png })]);
			toast.success("Image copied");
		} catch {
			toast.error("Could not copy this image");
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="viewer">
	<div
		class="stage"
		class:pan={manual !== null}
		class:dragging
		bind:this={stage}
		{onwheel}
		{onpointerdown}
		{onpointermove}
		{onpointerup}
		onpointercancel={onpointerup}
		ondblclick={() => (manual === null ? setScale(1) : reset())}
		role="img"
		aria-label={path}
	>
		<img
			src={url}
			alt={path}
			draggable="false"
			class:pixelated
			style:width="{nat.w * scale}px"
			style:height="{nat.h * scale}px"
			style:transform="translate({pan.x}px, {pan.y}px)"
			onload={(e) => {
				const img = e.currentTarget as HTMLImageElement;
				nat = { w: img.naturalWidth, h: img.naturalHeight };
			}}
		/>
	</div>
	<div class="info">
		<span class="mono">{nat.w ? `${nat.w} × ${nat.h}` : "…"}</span>
		<span>{formatBytes(size)}</span>
		<span>{extOf(path).toUpperCase()}</span>
		<span class="grow"></span>
		<Tip label="Zoom out" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Zoom out" onclick={() => setScale(scale / 1.25)}
					><ZoomOut size={13} /></button
				>
			{/snippet}
		</Tip>
		<button class="pct mono" onclick={() => setScale(1)} title="Actual size"
			>{Math.round(scale * 100)}%</button
		>
		<Tip label="Zoom in" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Zoom in" onclick={() => setScale(scale * 1.25)}
					><ZoomIn size={13} /></button
				>
			{/snippet}
		</Tip>
		<Tip label="Fit to view" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Fit to view" class:on={manual === null} onclick={reset}
					><Maximize size={13} /></button
				>
			{/snippet}
		</Tip>
		<Tip label="Copy image" side="top">
			{#snippet child(props)}
				<button {...props} aria-label="Copy image" onclick={copyImage}><Copy size={13} /></button>
			{/snippet}
		</Tip>
	</div>
</div>

<style>
	.viewer {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	.stage {
		flex: 1;
		min-height: 0;
		display: grid;
		place-items: center;
		overflow: hidden;
		touch-action: none;
		/* Checkerboard so transparency reads as transparency. */
		--sq: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		background-color: transparent;
		background-image:
			linear-gradient(45deg, var(--sq) 25%, transparent 25%, transparent 75%, var(--sq) 75%),
			linear-gradient(45deg, var(--sq) 25%, transparent 25%, transparent 75%, var(--sq) 75%);
		background-size: 16px 16px;
		background-position:
			0 0,
			8px 8px;
	}
	.stage.pan {
		cursor: grab;
	}
	.stage.dragging {
		cursor: grabbing;
	}
	img {
		display: block;
		max-width: none;
		border-radius: 2px;
		box-shadow: 0 8px 32px color-mix(in oklab, black 22%, transparent);
		user-select: none;
		-webkit-user-drag: none;
	}
	/* Promoted only while the picture is being dragged (a permanent layer holds a texture per open image). */
	.stage.dragging img {
		will-change: transform;
	}
	.stage:not(.dragging) img {
		transition:
			width 160ms cubic-bezier(0.2, 0.8, 0.2, 1),
			height 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	img.pixelated {
		image-rendering: pixelated;
	}
	.info {
		display: flex;
		align-items: center;
		gap: 12px;
		height: 28px;
		flex-shrink: 0;
		padding: 0 10px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.grow {
		flex: 1;
	}
	.mono {
		font-family: var(--mono);
	}
	.info button {
		display: inline-grid;
		place-items: center;
		min-width: 22px;
		height: 22px;
		padding: 0 4px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: inherit;
		transition: background 120ms ease-out;
	}
	.info button:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.info button.on {
		color: var(--space-accent-text, var(--ca-brand-main));
	}
	.pct {
		font-size: 11px;
		min-width: 44px !important;
	}
	@media (prefers-reduced-motion: reduce) {
		.stage:not(.dragging) img {
			transition: none;
		}
	}
</style>
