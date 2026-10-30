<script lang="ts">
	// Annotation editor (`/shots/editor?path=...`, window `shots-editor`). A separate webview: no app shell, no workspace.
	//
	// Esc: cancels the running gesture, then the text edit, then deselects, then asks to close (a confirmation only
	// when there are unsaved edits). Cmd/Ctrl+Z undoes (mid-drag it cancels the drag), Shift+Cmd+Z redoes, Delete /
	// Backspace removes the selection, Cmd/Ctrl+S saves a copy, Cmd/Ctrl+C copies the image (unless a text field has
	// focus), Cmd/Ctrl+Enter is Save & Copy. Tool letters: V A R T H B C N S M (B again flips Blur / Pixelate).
	//
	// Beautify (background, padding, rounded corners, shadow) is an export setting, not an annotation: it is not in the
	// undo history. The same `paintBeautified` draws the live preview and the export, so what you see is what you save.
	//
	// Two canvases at IMAGE resolution, placed by one CSS transform (the `View`): the base image and an overlay with
	// the items and the editor chrome. Pointer positions are converted with `viewToImage`, so the document only ever
	// holds image pixels. The export repaints from the source pixels on an offscreen canvas and never draws the chrome.
	import { invoke } from "@tauri-apps/api/core";
	import {
		Blend,
		Bold,
		Brush,
		Check,
		Circle,
		Copy,
		Crop,
		Grid3x3,
		Hash,
		Highlighter,
		Maximize,
		MousePointer2,
		MoveUpRight,
		Redo2,
		Save,
		Spotlight,
		Sparkles,
		Square,
		TriangleAlert,
		Type,
		Undo2,
		X,
	} from "@lucide/svelte";
	import { onMount, tick } from "svelte";
	import { errText } from "#lib/api.ts";
	import {
		COLORS,
		DEFAULT_STYLE,
		TEXT_SIZES,
		WIDTHS,
		begin,
		boundsOf,
		canRedo,
		canUndo,
		commitText,
		counterNumbers,
		deleteSelected,
		drag,
		end,
		escape,
		isDirty,
		markSaved,
		newDoc,
		normalizeHex,
		redo,
		restyle,
		setCrop,
		setText,
		styleOf,
		undo,
		type Doc,
		type Item,
		type Point,
		type SpotlightShape,
		type TextBg,
		type TextWeight,
		type Tool,
	} from "#lib/shots/annotate.ts";
	import {
		BEAUTIFY_DEFAULT,
		BEAUTIFY_GRADIENTS,
		BEAUTIFY_PADDINGS,
		BEAUTIFY_RADII,
		beautifyLayout,
		centerView,
		exportSize,
		fitZoom,
		imageToView,
		paintBackdrop,
		paintBeautified,
		paintItems,
		viewToImage,
		zoomAt,
		type Beautify,
		type BlurMode,
		type Patch,
		type Pixels,
		type View,
	} from "#lib/shots/render.ts";
	import type { EditorDoc, Entry } from "#lib/shots/types.ts";

	const PADDING = 32;
	const TOOLS: { tool: Tool; label: string; key: string; icon: typeof Square }[] = [
		{ tool: "select", label: "Select", key: "V", icon: MousePointer2 },
		{ tool: "arrow", label: "Arrow", key: "A", icon: MoveUpRight },
		{ tool: "rect", label: "Rectangle", key: "R", icon: Square },
		{ tool: "text", label: "Text", key: "T", icon: Type },
		{ tool: "highlight", label: "Highlight", key: "H", icon: Highlighter },
		{ tool: "marker", label: "Highlighter", key: "M", icon: Brush },
		{ tool: "counter", label: "Counter", key: "N", icon: Hash },
		{ tool: "spotlight", label: "Spotlight", key: "S", icon: Spotlight },
		{ tool: "blur", label: "Blur", key: "B", icon: Blend },
		{ tool: "crop", label: "Crop", key: "C", icon: Crop },
	];
	/** Tool letters (lower case) to tools. */
	const KEYS: Record<string, Tool> = {
		v: "select",
		a: "arrow",
		r: "rect",
		t: "text",
		h: "highlight",
		m: "marker",
		n: "counter",
		s: "spotlight",
		b: "blur",
		c: "crop",
	};
	/** Tools whose items have no colour or width of their own (the colour/width controls are hidden for them). */
	const NO_COLOR: Tool[] = ["blur", "crop", "spotlight"];

	let path = $state("");
	const empty = newDoc(1, 1);
	let doc = $state<Doc>(empty);
	let saved = $state(markSaved(empty));
	let loaded = $state(false);
	let error = $state("");
	let busy = $state(false);
	let confirmClose = $state(false);

	let tool = $state<Tool>("arrow");
	let color = $state(DEFAULT_STYLE.color);
	let width = $state(DEFAULT_STYLE.width);
	let blurMode = $state<BlurMode>("pixelate");
	let textSize = $state(DEFAULT_STYLE.size);
	let weight = $state<TextWeight>(DEFAULT_STYLE.weight);
	let bg = $state<TextBg>(DEFAULT_STYLE.bg);
	let shape = $state<SpotlightShape>(DEFAULT_STYLE.shape);
	let hex = $state("");
	let hexBad = $state(false);
	let beautify = $state<Beautify>({ ...BEAUTIFY_DEFAULT });
	let beautifyOpen = $state(false);
	/** Plain backgrounds next to the gradients. */
	const BACKDROP_SOLIDS = ["#ffffff", "#111111", "#e2e8f0"];

	let stage = $state<HTMLDivElement>();
	let baseCanvas = $state<HTMLCanvasElement>();
	let overlay = $state<HTMLCanvasElement>();
	let textInput = $state<HTMLInputElement>();
	let viewport = $state({ w: 0, h: 0 });
	let view = $state<View>({ zoom: 1, x: 0, y: 0 });
	/** True until the user zooms or pans by hand: a resize then keeps the image fitted. */
	let fitted = true;

	let image: HTMLImageElement | undefined;
	let source: Pixels | undefined;
	const patches = new Map<string, Patch | null>();

	let dirty = $derived(isDirty(doc, saved));
	let out = $derived(exportSize(doc));
	let style = $derived(styleOf({ color, width, mode: blurMode, size: textSize, weight, bg, shape }));
	let numbers = $derived(counterNumbers(doc.items));
	/** The selected item, so the controls show (and change) what it has. */
	let selectedItem = $derived(doc.items.find((i) => i.id === doc.selected));
	let layout = $derived(beautifyLayout(out, beautify));
	/** Text controls show for the text tool and for a selected text. */
	let textControls = $derived(tool === "text" || selectedItem?.type === "text");
	let editingItem = $derived(
		doc.editing
			? (doc.items.find((i) => i.id === doc.editing?.id && i.type === "text") as
					Extract<Item, { type: "text" }> | undefined)
			: undefined,
	);

	// Selecting an item shows its style in the toolbar, so the next change (or the next new item) starts from it.
	let syncedFor: number | null = null;
	$effect(() => {
		const it = selectedItem;
		const id = it?.id ?? null;
		if (id === syncedFor) return;
		syncedFor = id;
		if (!it) return;
		if ("color" in it) color = it.color;
		if ("width" in it) width = it.width;
		if (it.type === "text") {
			textSize = it.size;
			weight = it.weight ?? "regular";
			bg = it.bg ?? "none";
		}
		if (it.type === "spotlight") shape = it.shape;
		if (it.type === "blur") blurMode = it.mode;
	});

	onMount(() => {
		path = new URLSearchParams(location.search).get("path") ?? "";
		void load();
	});

	async function load() {
		try {
			const meta = await invoke<EditorDoc>("shots_editor_load", { path });
			const img = new Image();
			// An asset URL is cross-origin: without this the canvas is tainted and `getImageData` (needed for the blur) throws.
			if (!meta.src.startsWith("data:")) img.crossOrigin = "anonymous";
			img.src = meta.src;
			await img.decode();
			image = img;
			doc = newDoc(img.naturalWidth, img.naturalHeight);
			saved = markSaved(doc);
			await tick();
			drawBase();
			loaded = true;
			fit();
		} catch (e) {
			error = errText(e);
		}
	}

	function drawBase() {
		if (!image || !baseCanvas) return;
		baseCanvas.width = image.naturalWidth;
		baseCanvas.height = image.naturalHeight;
		const ctx = baseCanvas.getContext("2d", { willReadFrequently: true });
		ctx?.drawImage(image, 0, 0);
		try {
			source = ctx?.getImageData(0, 0, image.naturalWidth, image.naturalHeight);
		} catch (e) {
			error = `The image cannot be read for blurring: ${errText(e)}`;
		}
	}

	/**
	 * The box the editor keeps in view: the whole image, plus the beautify backdrop when it is on (it can reach past
	 * the image by the padding). `x`/`y` is its top-left in image space, so the view is shifted by it.
	 */
	function shown(): { x: number; y: number; w: number; h: number } {
		const w = image?.naturalWidth ?? doc.width;
		const h = image?.naturalHeight ?? doc.height;
		if (!beautify.on) return { x: 0, y: 0, w, h };
		const x1 = Math.min(0, kept.x - layout.image.x);
		const y1 = Math.min(0, kept.y - layout.image.y);
		const x2 = Math.max(w, kept.x - layout.image.x + layout.out.w);
		const y2 = Math.max(h, kept.y - layout.image.y + layout.out.h);
		return { x: x1, y: y1, w: x2 - x1, h: y2 - y1 };
	}

	function fitView(): View {
		const b = shown();
		const v = centerView(b, viewport, fitZoom(b, viewport, PADDING));
		return { ...v, x: v.x - b.x * v.zoom, y: v.y - b.y * v.zoom };
	}

	function fit() {
		if (!image) return;
		view = fitView();
		fitted = true;
	}

	function actualSize() {
		if (!image) return;
		view = centerView({ w: image.naturalWidth, h: image.naturalHeight }, viewport, 1);
		fitted = false;
	}

	$effect(() => {
		// Keep the image fitted while the window is resized and the user has not zoomed by hand.
		if (loaded && fitted && viewport.w > 0) untrackedFit();
	});
	function untrackedFit() {
		if (!image) return;
		const next = fitView();
		if (next.zoom !== view.zoom || next.x !== view.x || next.y !== view.y) view = next;
	}

	// Repaint the overlay whenever the document, the selection or the view changes.
	$effect(() => {
		if (!overlay || !loaded || !source) return;
		overlay.width = doc.width;
		overlay.height = doc.height;
		const ctx = overlay.getContext("2d");
		if (!ctx) return;
		ctx.clearRect(0, 0, doc.width, doc.height);
		paintItems(ctx, doc.items, {
			base: source,
			cache: patches,
			numbers,
			size: { w: doc.width, h: doc.height },
		});
		drawChrome(ctx);
	});

	/** The part of the image the export keeps: the crop, else the whole image. The backdrop wraps exactly this. */
	let kept = $derived(doc.crop ?? { x: 0, y: 0, w: doc.width, h: doc.height });
	let frame = $state<HTMLCanvasElement>();
	/** Rounded corners of the live preview: the kept region of the picture, clipped with the export's radius. */
	let picClip = $derived(
		beautify.on
			? `inset(${kept.y}px ${doc.width - kept.x - kept.w}px ${doc.height - kept.y - kept.h}px ${kept.x}px round ${layout.radius}px)`
			: undefined,
	);

	// The live beautify preview: the same backdrop the export paints, behind the editing layer. It is positioned in
	// image space (offset by the padding), so the layer keeps its coordinates and pointer mapping is unchanged.
	$effect(() => {
		if (!frame || !loaded) return;
		frame.width = layout.out.w;
		frame.height = layout.out.h;
		const ctx = frame.getContext("2d");
		if (!ctx) return;
		ctx.clearRect(0, 0, frame.width, frame.height);
		paintBackdrop(ctx, out, beautify);
	});

	/** Editor-only decoration (selection outline, crop dim), sized in screen pixels. Never part of the export. */
	function drawChrome(ctx: CanvasRenderingContext2D) {
		const px = 1 / view.zoom;
		const sel = doc.items.find((i) => i.id === doc.selected);
		if (sel) {
			const b = boundsOf(sel);
			ctx.save();
			ctx.strokeStyle = "#3b82f6";
			ctx.lineWidth = 1.5 * px;
			ctx.setLineDash([5 * px, 3 * px]);
			ctx.strokeRect(b.x - 3 * px, b.y - 3 * px, b.w + 6 * px, b.h + 6 * px);
			ctx.restore();
		}
		if (doc.crop) {
			const c = doc.crop;
			ctx.save();
			ctx.fillStyle = "rgba(0,0,0,0.55)";
			ctx.beginPath();
			ctx.rect(0, 0, doc.width, doc.height);
			ctx.rect(c.x, c.y, c.w, c.h);
			ctx.fill("evenodd");
			ctx.strokeStyle = "#ffffff";
			ctx.lineWidth = 1.5 * px;
			ctx.strokeRect(c.x, c.y, c.w, c.h);
			ctx.restore();
		}
	}

	// A blur region is re-blurred only when its parameters change; drop cache entries nothing refers to any more.
	$effect(() => {
		if (patches.size > 64) patches.clear();
	});

	function point(e: PointerEvent): Point {
		const r = stage!.getBoundingClientRect();
		return viewToImage({ x: e.clientX - r.left, y: e.clientY - r.top }, view);
	}

	let panning: { x: number; y: number } | null = null;
	let spaceDown = false;

	function onPointerDown(e: PointerEvent) {
		if (!loaded || isTyping(e.target)) return;
		stage?.setPointerCapture(e.pointerId);
		if (e.button === 1 || spaceDown) {
			panning = { x: e.clientX, y: e.clientY };
			return;
		}
		if (e.button !== 0) return;
		doc = begin(doc, tool, point(e), style);
		if (doc.editing) void focusInput();
	}

	function onPointerMove(e: PointerEvent) {
		if (panning) {
			view = { ...view, x: view.x + e.clientX - panning.x, y: view.y + e.clientY - panning.y };
			panning = { x: e.clientX, y: e.clientY };
			fitted = false;
			return;
		}
		if (doc.gesture) doc = drag(doc, point(e), e.shiftKey);
	}

	function onPointerUp(e: PointerEvent) {
		stage?.releasePointerCapture(e.pointerId);
		if (panning) {
			panning = null;
			return;
		}
		doc = end(doc);
	}

	function onWheel(e: WheelEvent) {
		e.preventDefault();
		const r = stage!.getBoundingClientRect();
		fitted = false;
		if (e.ctrlKey || e.metaKey) {
			view = zoomAt(view, view.zoom * Math.exp(-e.deltaY * 0.01), {
				x: e.clientX - r.left,
				y: e.clientY - r.top,
			});
		} else {
			view = { ...view, x: view.x - e.deltaX, y: view.y - e.deltaY };
		}
	}

	async function focusInput() {
		await tick();
		textInput?.focus();
	}

	function onTextKey(e: KeyboardEvent) {
		e.stopPropagation();
		if (e.key === "Enter") {
			e.preventDefault();
			doc = commitText(doc);
		} else if (e.key === "Escape") {
			e.preventDefault();
			doc = escape(doc).doc;
		}
	}

	function pickTool(t: Tool) {
		doc = commitText(doc);
		if (t === "blur" && tool === "blur") blurMode = blurMode === "blur" ? "pixelate" : "blur";
		tool = t;
	}

	function pickColor(c: string) {
		color = c;
		hexBad = false;
		if (doc.selected !== null) doc = restyle(doc, doc.selected, { color: c });
	}

	/** Apply the typed hex colour on Enter/blur; an invalid one is flagged and changes nothing. */
	function applyHex() {
		if (hex.trim() === "") {
			hexBad = false;
			return;
		}
		const c = normalizeHex(hex);
		hexBad = c === null;
		if (c) {
			hex = c;
			pickColor(c);
		}
	}

	function pickWidth(w: number) {
		width = w;
		if (doc.selected !== null) doc = restyle(doc, doc.selected, { width: w });
	}

	function pickTextSize(s: number) {
		textSize = s;
		if (selectedItem?.type === "text") doc = restyle(doc, selectedItem.id, { size: s });
	}

	function pickWeight(w: TextWeight) {
		weight = w;
		if (selectedItem?.type === "text") doc = restyle(doc, selectedItem.id, { weight: w });
	}

	function pickBg(b: TextBg) {
		bg = b;
		if (selectedItem?.type === "text") doc = restyle(doc, selectedItem.id, { bg: b });
	}

	function pickShape(sh: SpotlightShape) {
		shape = sh;
		if (selectedItem?.type === "spotlight") doc = restyle(doc, selectedItem.id, { shape: sh });
	}

	function setMode(m: BlurMode) {
		blurMode = m;
		if (doc.selected !== null) doc = restyle(doc, doc.selected, { mode: m });
	}

	/** Opening the panel turns Beautify on; closing it turns it off, so the export never carries a hidden background. */
	function toggleBeautify() {
		beautifyOpen = !beautifyOpen;
		beautify = { ...beautify, on: beautifyOpen };
	}

	function setBeautify(patch: Partial<Beautify>) {
		beautify = { ...beautify, ...patch, on: true };
	}

	function requestClose() {
		if (dirty) confirmClose = true;
		else void closeWindow();
	}

	async function closeWindow() {
		try {
			await invoke("shots_editor_close");
		} catch (e) {
			error = errText(e);
		}
	}

	/** The edited image as base64 PNG at image resolution: crop applied, blur/pixelate applied to the pixels. */
	function exportPng(): string {
		if (!image || !source) throw new Error("The image is not loaded");
		const full = document.createElement("canvas");
		full.width = doc.width;
		full.height = doc.height;
		const ctx = full.getContext("2d");
		if (!ctx) throw new Error("No 2D canvas available");
		ctx.drawImage(image, 0, 0);
		paintItems(ctx, doc.items, {
			base: source,
			numbers: counterNumbers(doc.items),
			size: { w: doc.width, h: doc.height },
		});
		// Crop first, then beautify: the background wraps the cropped picture.
		const size = exportSize(doc);
		const crop = doc.crop ?? { x: 0, y: 0, w: doc.width, h: doc.height };
		const cropped = document.createElement("canvas");
		cropped.width = size.w;
		cropped.height = size.h;
		cropped.getContext("2d")?.drawImage(full, crop.x, crop.y, crop.w, crop.h, 0, 0, size.w, size.h);
		const result = document.createElement("canvas");
		const l = beautifyLayout(size, beautify);
		result.width = l.out.w;
		result.height = l.out.h;
		const rctx = result.getContext("2d");
		if (!rctx) throw new Error("No 2D canvas available");
		paintBeautified(rctx, cropped, size, beautify);
		return result.toDataURL("image/png").replace(/^data:image\/png;base64,/, "");
	}

	async function done(mode: "file" | "copy" | "both") {
		if (busy) return;
		doc = commitText(doc);
		busy = true;
		error = "";
		try {
			await invoke<Entry>("shots_editor_save", { path, png: exportPng(), mode });
			saved = markSaved(doc);
			await closeWindow();
		} catch (e) {
			error = errText(e);
		} finally {
			busy = false;
		}
	}

	function onKeyDown(e: KeyboardEvent) {
		if (!loaded) return;
		if (e.key === " " && !isTyping(e.target)) spaceDown = true;
		const mod = e.metaKey || e.ctrlKey;
		if (isTyping(e.target)) return;
		if (mod && e.key === "Enter") {
			e.preventDefault();
			void done("both");
		} else if (mod && e.key.toLowerCase() === "s") {
			e.preventDefault();
			void done("file");
		} else if (mod && e.key.toLowerCase() === "c") {
			// Copy the image, but never steal Cmd+C from a text selection the person made on the page.
			if (hasTextSelection()) return;
			e.preventDefault();
			void done("copy");
		} else if (mod && e.key.toLowerCase() === "z") {
			e.preventDefault();
			doc = e.shiftKey ? redo(doc) : undo(doc);
		} else if (mod && e.key.toLowerCase() === "y") {
			e.preventDefault();
			doc = redo(doc);
		} else if (e.key === "Delete" || e.key === "Backspace") {
			e.preventDefault();
			doc = deleteSelected(doc);
		} else if (e.key === "Escape") {
			e.preventDefault();
			if (confirmClose) {
				confirmClose = false;
				return;
			}
			const r = escape(doc);
			doc = r.doc;
			if (r.action === "close") requestClose();
		} else if (!mod && !e.altKey && KEYS[e.key.toLowerCase()]) {
			pickTool(KEYS[e.key.toLowerCase()]);
		}
	}

	/** A text field has the focus: letters and Cmd+C/Z belong to it. */
	function isTyping(t: EventTarget | null): boolean {
		return (
			t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement
		);
	}

	function hasTextSelection(): boolean {
		return (window.getSelection()?.toString() ?? "") !== "";
	}

	function onKeyUp(e: KeyboardEvent) {
		if (e.key === " ") spaceDown = false;
	}

	let box = $derived(editingItem ? imageToView(editingItem.at, view) : null);
	let cursor = $derived(panning || spaceDown ? "grabbing" : tool === "select" ? "default" : "crosshair");
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />

<main class="editor">
	<header class="bar" aria-label="Editor tools">
		<div class="group" role="toolbar" aria-label="Tools">
			{#each TOOLS as t (t.tool)}
				<button
					class="tool"
					class:on={tool === t.tool}
					aria-label={t.label}
					aria-pressed={tool === t.tool}
					title="{t.label} ({t.key})"
					onclick={() => pickTool(t.tool)}
				>
					<t.icon size={16} />
				</button>
			{/each}
		</div>

		{#if tool === "blur"}
			<div class="group" role="radiogroup" aria-label="Blur style">
				<button
					class="chip"
					class:on={blurMode === "pixelate"}
					role="radio"
					aria-checked={blurMode === "pixelate"}
					onclick={() => setMode("pixelate")}
				>
					<Grid3x3 size={14} /> Pixelate
				</button>
				<button
					class="chip"
					class:on={blurMode === "blur"}
					role="radio"
					aria-checked={blurMode === "blur"}
					onclick={() => setMode("blur")}
				>
					<Blend size={14} /> Blur
				</button>
			</div>
		{:else if tool === "spotlight" || selectedItem?.type === "spotlight"}
			<div class="group" role="radiogroup" aria-label="Spotlight shape">
				<button
					class="chip"
					class:on={shape === "rect"}
					role="radio"
					aria-checked={shape === "rect"}
					onclick={() => pickShape("rect")}
				>
					<Square size={14} /> Rectangle
				</button>
				<button
					class="chip"
					class:on={shape === "ellipse"}
					role="radio"
					aria-checked={shape === "ellipse"}
					onclick={() => pickShape("ellipse")}
				>
					<Circle size={14} /> Ellipse
				</button>
			</div>
		{/if}

		{#if !NO_COLOR.includes(tool) && selectedItem?.type !== "blur" && selectedItem?.type !== "spotlight"}
			<div class="group palette" role="radiogroup" aria-label="Colour">
				{#each COLORS as c (c)}
					<button
						class="swatch"
						class:on={color === c}
						role="radio"
						aria-checked={color === c}
						aria-label="Colour {c}"
						style:--c={c}
						onclick={() => pickColor(c)}
					></button>
				{/each}
				<input
					class="hex"
					class:bad={hexBad}
					aria-label="Custom colour, hex"
					aria-invalid={hexBad}
					placeholder="#hex"
					maxlength="7"
					spellcheck="false"
					autocomplete="off"
					bind:value={hex}
					onkeydown={(e) => {
						e.stopPropagation();
						if (e.key === "Enter") applyHex();
					}}
					onblur={applyHex}
				/>
			</div>
			{#if tool !== "text" && selectedItem?.type !== "text"}
				<div class="group" role="radiogroup" aria-label="Stroke width">
					{#each WIDTHS as w (w)}
						<button
							class="chip"
							class:on={width === w}
							role="radio"
							aria-checked={width === w}
							aria-label="Stroke {w} pixels"
							onclick={() => pickWidth(w)}
						>
							<span class="stroke" style:height="{w}px"></span>
						</button>
					{/each}
				</div>
			{/if}
		{/if}

		{#if textControls}
			<div class="group" role="radiogroup" aria-label="Text size">
				{#each TEXT_SIZES as t (t.label)}
					<button
						class="chip"
						class:on={textSize === t.size}
						role="radio"
						aria-checked={textSize === t.size}
						aria-label="Text size {t.label}"
						onclick={() => pickTextSize(t.size)}>{t.label}</button
					>
				{/each}
			</div>
			<div class="group">
				<button
					class="chip"
					class:on={weight === "bold"}
					aria-label="Bold"
					aria-pressed={weight === "bold"}
					onclick={() => pickWeight(weight === "bold" ? "regular" : "bold")}><Bold size={14} /></button
				>
			</div>
			<div class="group" role="radiogroup" aria-label="Text background">
				{#each [["none", "None"], ["solid", "Solid"], ["pill", "Pill"]] as const as [b, label] (b)}
					<button
						class="chip"
						class:on={bg === b}
						role="radio"
						aria-checked={bg === b}
						aria-label="Text background {label}"
						onclick={() => pickBg(b)}>{label}</button
					>
				{/each}
			</div>
		{/if}

		<div class="group">
			<button
				class="tool"
				aria-label="Undo"
				title="Undo (Cmd+Z)"
				disabled={!canUndo(doc)}
				onclick={() => (doc = undo(doc))}><Undo2 size={16} /></button
			>
			<button
				class="tool"
				aria-label="Redo"
				title="Redo (Shift+Cmd+Z)"
				disabled={!canRedo(doc)}
				onclick={() => (doc = redo(doc))}><Redo2 size={16} /></button
			>
			{#if doc.crop}
				<button class="chip" aria-label="Remove crop" onclick={() => (doc = setCrop(doc, null))}
					><Crop size={14} /> Reset crop</button
				>
			{/if}
		</div>

		<div class="group zoom">
			<button
				class="chip"
				class:on={beautify.on}
				aria-label="Beautify"
				aria-pressed={beautify.on}
				aria-expanded={beautifyOpen}
				title="Beautify: background, padding, rounded corners, shadow"
				onclick={toggleBeautify}><Sparkles size={14} /> Beautify</button
			>
			<button class="chip" aria-label="Fit to window" onclick={fit}><Maximize size={14} /> Fit</button>
			<button class="chip" aria-label="Actual size, 100%" onclick={actualSize}>100%</button>
			<span class="dim" aria-live="polite">{Math.round(view.zoom * 100)}%</span>
		</div>
	</header>

	{#if beautifyOpen}
		<section class="bar beautify" aria-label="Beautify">
			<div class="group" role="radiogroup" aria-label="Background">
				{#each BEAUTIFY_GRADIENTS as g, i (g.name)}
					<button
						class="swatch"
						class:on={beautify.background.kind === "gradient" && beautify.background.index === i}
						role="radio"
						aria-checked={beautify.background.kind === "gradient" && beautify.background.index === i}
						aria-label="Background {g.name}"
						title={g.name}
						style:--c="linear-gradient(135deg, {g.stops.join(', ')})"
						onclick={() => setBeautify({ background: { kind: "gradient", index: i } })}
					></button>
				{/each}
				{#each BACKDROP_SOLIDS as c (c)}
					<button
						class="swatch"
						class:on={beautify.background.kind === "solid" && beautify.background.color === c}
						role="radio"
						aria-checked={beautify.background.kind === "solid" && beautify.background.color === c}
						aria-label="Background colour {c}"
						style:--c={c}
						onclick={() => setBeautify({ background: { kind: "solid", color: c } })}
					></button>
				{/each}
			</div>
			<div class="group" role="radiogroup" aria-label="Padding">
				<span class="dim">Padding</span>
				{#each BEAUTIFY_PADDINGS as v (v)}
					<button
						class="chip"
						class:on={beautify.padding === v}
						role="radio"
						aria-checked={beautify.padding === v}
						aria-label="Padding {v} pixels"
						onclick={() => setBeautify({ padding: v })}>{v}</button
					>
				{/each}
			</div>
			<div class="group" role="radiogroup" aria-label="Corner radius">
				<span class="dim">Corners</span>
				{#each BEAUTIFY_RADII as v (v)}
					<button
						class="chip"
						class:on={beautify.radius === v}
						role="radio"
						aria-checked={beautify.radius === v}
						aria-label="Corner radius {v} pixels"
						onclick={() => setBeautify({ radius: v })}>{v}</button
					>
				{/each}
			</div>
			<div class="group">
				<button
					class="chip"
					class:on={beautify.shadow}
					aria-label="Shadow"
					aria-pressed={beautify.shadow}
					onclick={() => setBeautify({ shadow: !beautify.shadow })}>Shadow</button
				>
			</div>
		</section>
	{/if}

	<div
		class="stage"
		role="application"
		aria-label="Image canvas"
		style:cursor
		bind:this={stage}
		bind:clientWidth={viewport.w}
		bind:clientHeight={viewport.h}
		onpointerdown={onPointerDown}
		onpointermove={onPointerMove}
		onpointerup={onPointerUp}
		onpointercancel={onPointerUp}
		onwheel={onWheel}
	>
		<div
			class="layer"
			class:plain={!beautify.on}
			style:width="{doc.width}px"
			style:height="{doc.height}px"
			style:transform="translate({view.x}px, {view.y}px) scale({view.zoom})"
		>
			{#if beautify.on}
				<canvas
					class="frame"
					bind:this={frame}
					aria-hidden="true"
					style:left="{kept.x - layout.image.x}px"
					style:top="{kept.y - layout.image.y}px"
					style:width="{layout.out.w}px"
					style:height="{layout.out.h}px"
				></canvas>
			{/if}
			<div class="pic" style:clip-path={picClip}>
				<canvas class="base" bind:this={baseCanvas} aria-label="Screenshot"></canvas>
				<canvas class="over" bind:this={overlay} aria-hidden="true"></canvas>
			</div>
		</div>

		{#if editingItem && box}
			<input
				class="text-edit"
				bind:this={textInput}
				aria-label="Annotation text"
				value={editingItem.text}
				oninput={(e) => (doc = setText(doc, e.currentTarget.value))}
				onkeydown={onTextKey}
				onblur={() => (doc = commitText(doc))}
				style:left="{box.x}px"
				style:top="{box.y}px"
				style:font-size="{editingItem.size * view.zoom}px"
				style:font-weight={editingItem.weight === "bold" ? 700 : 400}
				style:color={editingItem.bg === "none" ? editingItem.color : "inherit"}
				style:background={editingItem.bg === "none" ? undefined : editingItem.color}
				style:border-radius={editingItem.bg === "pill"
					? "999px"
					: editingItem.bg === "solid"
						? "4px"
						: undefined}
			/>
		{/if}

		{#if !loaded && !error}<p class="dim center">Loading…</p>{/if}
	</div>

	{#if error}
		<p class="error" role="alert"><TriangleAlert size={14} /> {error}</p>
	{/if}

	<footer class="foot">
		<span class="dim">{layout.out.w} × {layout.out.h} px{dirty ? " · edited" : ""}</span>
		{#if confirmClose}
			<div class="group" role="alertdialog" aria-label="Discard edits">
				<span>Discard your edits?</span>
				<button class="chip danger" onclick={closeWindow}>Discard</button>
				<button class="chip" onclick={() => (confirmClose = false)}>Keep editing</button>
			</div>
		{:else}
			<div class="group">
				<button class="chip" aria-label="Cancel" onclick={requestClose}><X size={14} /> Cancel</button>
				<button
					class="chip"
					aria-label="Copy to clipboard"
					title="Copy the image (Cmd+C)"
					disabled={!loaded || busy}
					onclick={() => done("copy")}><Copy size={14} /> Copy</button
				>
				<button
					class="chip"
					aria-label="Save a copy"
					title="Save a copy (Cmd+S)"
					disabled={!loaded || busy}
					onclick={() => done("file")}><Save size={14} /> Save</button
				>
				<button
					class="chip primary"
					aria-label="Save and copy"
					title="Save and copy (Cmd+Enter)"
					disabled={!loaded || busy}
					onclick={() => done("both")}><Check size={14} /> Save & Copy</button
				>
			</div>
		{/if}
	</footer>
</main>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
	}
	.editor {
		display: flex;
		flex-direction: column;
		width: 100vw;
		height: 100vh;
		overflow: hidden;
		color: var(--ca-text-primary);
		font-size: 12.5px;
		background: var(--ca-surface-sunken);
		--line: color-mix(in oklab, var(--ca-text-primary) 14%, transparent);
	}
	.bar,
	.foot {
		display: flex;
		flex: none;
		flex-wrap: wrap;
		gap: 12px;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		background: var(--ca-surface-raised);
	}
	.bar {
		justify-content: flex-start;
		box-shadow: inset 0 -1px 0 var(--line);
	}
	.foot {
		box-shadow: inset 0 1px 0 var(--line);
	}
	.group {
		display: flex;
		gap: 4px;
		align-items: center;
	}
	.zoom {
		margin-left: auto;
	}
	button {
		color: inherit;
		font: inherit;
		cursor: pointer;
		background: transparent;
		border: 0;
	}
	button:disabled {
		cursor: default;
		opacity: 0.4;
	}
	button:focus-visible,
	.text-edit:focus-visible {
		outline: 2px solid var(--ca-brand-main);
		outline-offset: 1px;
	}
	.tool,
	.chip {
		display: inline-flex;
		gap: 5px;
		align-items: center;
		justify-content: center;
		min-width: 28px;
		height: 28px;
		padding: 0 8px;
		border-radius: 7px;
		box-shadow: inset 0 0 0 1px var(--line);
	}
	.tool:hover:not(:disabled),
	.chip:hover:not(:disabled) {
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.tool.on,
	.chip.on {
		background: color-mix(in oklab, var(--ca-brand-main) 22%, transparent);
		box-shadow: inset 0 0 0 1px var(--ca-brand-main);
	}
	.chip.primary {
		color: var(--ca-surface-sunken);
		background: var(--ca-brand-main);
	}
	.chip.danger {
		color: var(--ca-semantic-error);
	}
	.beautify {
		box-shadow: inset 0 -1px 0 var(--line);
	}
	.hex {
		width: 64px;
		height: 24px;
		padding: 0 6px;
		margin-left: 4px;
		color: inherit;
		font: inherit;
		font-family: ui-monospace, monospace;
		background: transparent;
		border: 0;
		border-radius: 6px;
		box-shadow: inset 0 0 0 1px var(--line);
	}
	.hex:focus-visible {
		outline: 2px solid var(--ca-brand-main);
		outline-offset: 1px;
	}
	.hex.bad {
		box-shadow: inset 0 0 0 1px var(--ca-semantic-error);
	}
	.swatch {
		width: 20px;
		height: 20px;
		background: var(--c);
		border-radius: 50%;
		box-shadow: inset 0 0 0 1px var(--line);
	}
	.swatch.on {
		box-shadow:
			inset 0 0 0 1px var(--line),
			0 0 0 2px var(--ca-surface-raised),
			0 0 0 4px var(--ca-brand-main);
	}
	.stroke {
		display: block;
		width: 16px;
		background: currentColor;
		border-radius: 2px;
	}
	.stage {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		touch-action: none;
		background: conic-gradient(
				#0000 25%,
				color-mix(in oklab, var(--ca-text-primary) 6%, transparent) 0 50%,
				#0000 0 75%,
				color-mix(in oklab, var(--ca-text-primary) 6%, transparent) 0
			)
			0 0 / 16px 16px;
	}
	.layer {
		position: absolute;
		top: 0;
		left: 0;
		transform-origin: 0 0;
	}
	.layer.plain {
		box-shadow: 0 4px 24px color-mix(in oklab, black 35%, transparent);
	}
	.frame {
		position: absolute;
		pointer-events: none;
	}
	.pic {
		position: absolute;
		inset: 0;
	}
	.base,
	.over {
		position: absolute;
		top: 0;
		left: 0;
		width: 100%;
		height: 100%;
	}
	.over {
		pointer-events: none;
	}
	.text-edit {
		position: absolute;
		min-width: 4ch;
		padding: 0;
		margin: 0;
		font-family:
			system-ui,
			-apple-system,
			"Segoe UI",
			sans-serif;
		font-weight: 600;
		line-height: 1.25;
		background: color-mix(in oklab, black 25%, transparent);
		border: 0;
		outline: 1px dashed #3b82f6;
	}
	.dim {
		color: var(--ca-text-secondary);
	}
	.center {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		margin: 0;
	}
	.error {
		display: flex;
		gap: 6px;
		align-items: center;
		margin: 0;
		padding: 6px 12px;
		color: var(--ca-semantic-error);
		background: var(--ca-surface-raised);
	}
</style>
