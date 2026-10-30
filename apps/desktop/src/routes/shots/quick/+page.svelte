<script lang="ts">
	// Quick-access stack (`shots-quick`): Rust sizes and places ONE window for all the cards (`shots://quick-stack`,
	// `shots_quick_stack`); this page draws them, newest nearest the corner. State machine and timers live in
	// `#lib/shots/quick.ts` and are per card; every file/clipboard/dialog action is a Rust command (contract:
	// `shots_quick_*`, `shots_pin_open`, `shots_gif_*`). Keyboard: Esc closes a GIF sheet, then a pending Discard
	// confirmation, then the newest card (the reducer decides). The stack is pointer-driven, but buttons keep
	// `:focus-visible` rings for the keyboard. A card is a capture or a text read from an image (OCR).
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import {
		Check,
		Copy,
		Download,
		Ellipsis,
		Film,
		FolderOpen,
		Image,
		Loader,
		Pencil,
		Pin,
		Send,
		TextCursorInput,
		TriangleAlert,
		Video,
		X,
	} from "@lucide/svelte";
	import { onMount, untrack } from "svelte";
	import { errText } from "#lib/api.ts";
	import {
		humanBytes,
		initialState,
		kindIcon,
		layout,
		GIF_CANCELLED,
		reduce,
		textTitle,
		title,
		type CaptureAction,
		type CaptureView,
		type QuickEffect,
		type QuickEvent,
		type QuickView,
		type StackIn,
		type TextAction,
	} from "#lib/shots/quick.ts";
	import type { Entry, GifEstimate, GifPreset, GifProgress } from "#lib/shots/types.ts";

	let st = $state<QuickView>(initialState());
	/** The card whose overflow menu is open. */
	let menuFor = $state<string | null>(null);

	// GIF sheet (one at a time: the reducer only opens it for a recording, and Esc closes it first).
	let presets = $state<GifPreset[]>([]);
	let preset = $state("");
	let estimate = $state<GifEstimate | null>(null);
	let sheetError = $state<string | null>(null);
	let exporting = $state(false);
	let exported = $state<Entry | null>(null);

	const icons = { image: Image, video: Video, film: Film } as const;
	const cards = $derived(layout(st));

	function dispatch(ev: QuickEvent) {
		const r = reduce(st, ev);
		st = r.state;
		for (const e of r.effects) void run(e);
	}

	async function run(e: QuickEffect) {
		switch (e.type) {
			case "close":
				if (menuFor === e.id) menuFor = null;
				// Rust removes the card (and hides the window with the last one) and re-sends the list.
				if (e.card === "capture")
					await invoke("shots_quick_action", { action: "close", path: e.id }).catch(() => {});
				else await invoke("shots_quick_text", { action: "close", id: e.id }).catch(() => {});
				break;
			case "hover":
				await invoke("shots_quick_hover", { hovering: e.hovering }).catch(() => {});
				break;
			case "invoke":
				try {
					await invoke("shots_quick_action", { action: e.action, path: e.path });
					dispatch({ type: "actionOk", id: e.id });
				} catch (err) {
					dispatch({ type: "actionErr", id: e.id, message: errText(err) });
				}
				break;
			case "invokeText":
				try {
					await invoke("shots_quick_text", { action: e.action, id: e.id });
					dispatch({ type: "actionOk", id: e.id });
				} catch (err) {
					dispatch({ type: "actionErr", id: e.id, message: errText(err) });
				}
				break;
			case "cancelExport":
				// The encode stops within one frame; its export call then rejects with GIF_CANCELLED.
				await invoke("shots_gif_cancel").catch(() => {});
				break;
		}
	}

	function act(id: string, name: CaptureAction) {
		menuFor = null;
		dispatch({ type: "action", id, name });
	}

	function actText(id: string, name: TextAction) {
		dispatch({ type: "textAction", id, name });
	}

	/** Native file drag (NSDraggingSession in Rust); the web drag would only move the data-URL image. */
	function onDragStart(ev: DragEvent, c: CaptureView) {
		ev.preventDefault();
		void invoke("shots_quick_drag", { path: c.entry.path }).catch((err) =>
			dispatch({ type: "actionErr", id: c.id, message: errText(err) }),
		);
	}

	function onKey(ev: KeyboardEvent) {
		if (ev.key !== "Escape") return;
		if (menuFor !== null) {
			menuFor = null;
			return;
		}
		dispatch({ type: "escape" });
	}

	// --- GIF sheet ---------------------------------------------------------------------------------------------

	async function openGif(id: string) {
		dispatch({ type: "openGifSheet", id });
		sheetError = null;
		exported = null;
		if (presets.length > 0) return;
		try {
			presets = await invoke<GifPreset[]>("shots_gif_presets");
			// The middle preset is the sensible default when Rust lists several; the first otherwise.
			preset = presets[Math.floor((presets.length - 1) / 2)]?.id ?? "";
		} catch (err) {
			sheetError = errText(err);
		}
	}

	const sheetCard = $derived(
		st.cards.find((c): c is CaptureView => c.kind === "capture" && c.sheet === "gif") ?? null,
	);

	let estimateSeq = 0;
	// Refetched whenever a sheet is open and the preset (or that card's file) changes; a slow answer for a preset the
	// user already left must not overwrite the current one.
	$effect(() => {
		const path = sheetCard?.entry.path ?? "";
		const id = preset;
		if (!path || !id) return;
		const seq = ++estimateSeq;
		untrack(() => {
			estimate = null;
			sheetError = null;
		});
		invoke<GifEstimate>("shots_gif_estimate", { path, preset: id }).then(
			(r) => {
				if (seq === estimateSeq) estimate = r;
			},
			(err) => {
				if (seq === estimateSeq) sheetError = errText(err);
			},
		);
	});

	async function exportGif(c: CaptureView) {
		if (!preset || exporting) return;
		exporting = true;
		sheetError = null;
		dispatch({ type: "exportStart", id: c.id });
		try {
			exported = await invoke<Entry>("shots_gif_export", { path: c.entry.path, preset });
			dispatch({ type: "exportEnd", id: c.id });
		} catch (err) {
			if (errText(err) === GIF_CANCELLED) {
				// Esc asked for it: the sheet closes quietly, nothing to read.
				dispatch({ type: "exportCancelled", id: c.id });
			} else {
				sheetError = errText(err);
				dispatch({ type: "exportEnd", id: c.id });
			}
		} finally {
			exporting = false;
		}
	}

	const estimateText = $derived(
		estimate
			? `≈ ${humanBytes(estimate.bytes)} · ${estimate.width} × ${estimate.height} · ${estimate.fps} fps`
			: "",
	);

	let column: HTMLElement | undefined = $state();

	onMount(() => {
		const offs: Array<() => void> = [];
		const keep = (p: Promise<() => void>) => void p.then((f) => offs.push(f));
		keep(
			listen<StackIn>("shots://quick-stack", (e) => {
				exported = null;
				dispatch({ type: "stack", stack: e.payload });
			}),
		);
		keep(listen<GifProgress>("shots://gif-progress", (e) => dispatch({ type: "gifProgress", ...e.payload })));
		void invoke<StackIn>("shots_quick_stack").then(
			(s) => dispatch({ type: "stack", stack: s }),
			(err) => console.error("shots_quick_stack", errText(err)),
		);
		window.addEventListener("keydown", onKey);
		let last = performance.now();
		const clock = window.setInterval(() => {
			const now = performance.now();
			dispatch({ type: "tick", dtMs: now - last });
			last = now;
		}, 100);
		return () => {
			offs.forEach((f) => f());
			window.removeEventListener("keydown", onKey);
			window.clearInterval(clock);
		};
	});

	// A stack taller than the display scrolls inside the window: keep the newest card (nearest the corner) in view.
	let seen = 0;
	$effect(() => {
		const n = st.cards.length;
		const bottom = st.position === "bottomLeft" || st.position === "bottomRight";
		if (n > seen && column) column.scrollTo({ top: bottom ? column.scrollHeight : 0 });
		seen = n;
	});
</script>

<svelte:head><title>Quick access</title></svelte:head>

<div class="column" bind:this={column}>
	{#each cards as c (c.id)}
		<div
			class="card"
			role="group"
			aria-label={c.kind === "text" ? "Captured text" : "Capture"}
			onpointerenter={() => dispatch({ type: "hover", id: c.id, on: true })}
			onpointerleave={() => dispatch({ type: "hover", id: c.id, on: false })}
		>
			{#if c.kind === "text"}
				<div class="meta head">
					<TextCursorInput size={13} aria-hidden="true" />
					<span class="title">{textTitle(c)}</span>
					<button
						class="x inline"
						aria-label="Close"
						onclick={() => dispatch({ type: "textAction", id: c.id, name: "close" })}
						><X size={12} aria-hidden="true" /></button
					>
				</div>
				<div class="text">
					{#if c.empty}
						<p class="none">No text found</p>
					{:else}
						<pre class="preview">{c.preview}</pre>
					{/if}
				</div>
				{#if c.copied}
					<span class="badge inline" role="status"><Check size={12} aria-hidden="true" /> Copied</span>
				{/if}
				{#if c.error}
					<p class="err" role="alert"><TriangleAlert size={12} aria-hidden="true" /> {c.error}</p>
				{/if}
				{#if !c.empty}
					<div class="actions">
						<button class="btn" onclick={() => actText(c.id, "copy")} disabled={c.busy}
							><Copy size={12} aria-hidden="true" /> Copy again</button
						>
						<button class="btn primary" onclick={() => actText(c.id, "send")} disabled={c.busy}
							><Send size={12} aria-hidden="true" /> Send to agent</button
						>
					</div>
				{/if}
			{:else}
				{@const Icon = icons[kindIcon(c.entry.kind) as keyof typeof icons]}
				<div class="thumb" class:small={c.sheet === "gif"}>
					<img src={c.thumb} alt={title(c.entry)} draggable="true" ondragstart={(ev) => onDragStart(ev, c)} />
					<button class="x" aria-label="Close" onclick={() => act(c.id, "close")}
						><X size={12} aria-hidden="true" /></button
					>
					{#if c.copied}
						<span class="badge" role="status"><Check size={12} aria-hidden="true" /> Copied</span>
					{/if}
				</div>
				<div class="meta">
					<Icon size={13} aria-hidden="true" />
					<span class="title">{title(c.entry)}</span>
				</div>

				{#if c.error}
					<p class="err" role="alert"><TriangleAlert size={12} aria-hidden="true" /> {c.error}</p>
				{/if}

				{#if c.sheet === "gif"}
					<div class="sheet" role="group" aria-label="Make GIF">
						{#if presets.length > 0}
							<div class="presets" role="radiogroup" aria-label="GIF quality">
								{#each presets as p (p.id)}
									<label class="preset">
										<input
											type="radio"
											name="gif-preset"
											value={p.id}
											bind:group={preset}
											disabled={exporting}
										/>
										<span>{p.label}</span>
									</label>
								{/each}
							</div>
						{/if}
						<p class="est" aria-live="polite">
							{estimateText || (presets.length > 0 && !sheetError ? "Estimating…" : "")}
						</p>
						{#if exporting}
							<progress
								class="bar"
								aria-label="Exporting GIF"
								max={c.progress?.total ?? 1}
								value={c.progress?.done ?? 0}
							></progress>
						{/if}
						{#if exported}
							<p class="est" role="status">
								<Check size={12} aria-hidden="true" /> Saved {title(exported)}
							</p>
						{/if}
						{#if sheetError}
							<p class="err" role="alert"><TriangleAlert size={12} aria-hidden="true" /> {sheetError}</p>
						{/if}
						<div class="row">
							<button
								class="btn"
								onclick={() => dispatch({ type: "closeSheet", id: c.id })}
								disabled={exporting}>Cancel</button
							>
							<button
								class="btn primary"
								onclick={() => exportGif(c)}
								disabled={exporting || !preset || !estimate}
							>
								{#if exporting}<Loader size={12} class="spin" aria-hidden="true" />{:else}<Download
										size={12}
										aria-hidden="true"
									/>{/if}
								Export
							</button>
						</div>
					</div>
				{:else}
					<div class="actions">
						<button class="btn" onclick={() => act(c.id, "copy")} disabled={c.busy}
							><Copy size={12} aria-hidden="true" /> Copy</button
						>
						<button class="btn" onclick={() => act(c.id, "save")} disabled={c.busy}
							><Download size={12} aria-hidden="true" /> Save…</button
						>
						{#if c.entry.kind === "screenshot"}
							<button class="btn" onclick={() => act(c.id, "annotate")} disabled={c.busy}
								><Pencil size={12} aria-hidden="true" /> Annotate</button
							>
							<button class="btn" onclick={() => act(c.id, "pin")} disabled={c.busy}
								><Pin size={12} aria-hidden="true" /> Pin</button
							>
							<button class="btn" onclick={() => act(c.id, "ocr")} disabled={c.busy}
								><TextCursorInput size={12} aria-hidden="true" /> Text</button
							>
						{/if}
						{#if c.entry.kind === "recording"}
							<button class="btn" onclick={() => openGif(c.id)} disabled={c.busy}
								><Film size={12} aria-hidden="true" /> Make GIF</button
							>
						{/if}
						<button class="btn primary" onclick={() => act(c.id, "send")} disabled={c.busy}
							><Send size={12} aria-hidden="true" /> Send to agent</button
						>
						<div class="more">
							<button
								class="btn icon"
								aria-label="More actions"
								aria-haspopup="menu"
								aria-expanded={menuFor === c.id}
								onclick={() => (menuFor = menuFor === c.id ? null : c.id)}
								><Ellipsis size={14} aria-hidden="true" /></button
							>
							{#if menuFor === c.id}
								<div class="menu" role="menu">
									<button role="menuitem" class="item" onclick={() => act(c.id, "reveal")}
										><FolderOpen size={12} aria-hidden="true" /> Reveal in Finder</button
									>
									<button role="menuitem" class="item danger" onclick={() => act(c.id, "discard")}>
										{c.confirmDiscard ? "Click again to delete" : "Discard"}
									</button>
								</div>
							{/if}
						</div>
					</div>
					{#if c.confirmDiscard && menuFor !== c.id}
						<button class="confirm" onclick={() => act(c.id, "discard")}>Click again to delete</button>
					{/if}
				{/if}
			{/if}
		</div>
	{/each}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		margin: 0;
		overflow: hidden;
	}
	/* The window is exactly the cards (252 × 270 each, 8 apart, see `surface::QUICK_CARD`): a taller stack than the
	   display scrolls inside it, without a scrollbar. */
	.column {
		box-sizing: border-box;
		height: 100vh;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		gap: 8px;
		padding: 0 16px 0 16px;
		overflow-y: auto;
		scrollbar-width: none;
	}
	.column::-webkit-scrollbar {
		display: none;
	}
	.card {
		box-sizing: border-box;
		flex: 0 0 270px;
		height: 270px;
		width: 220px;
		margin-left: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px;
		border-radius: 12px;
		overflow: hidden;
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised);
		font-size: 12px;
		--line: color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
		box-shadow:
			inset 0 0 0 1px var(--line),
			0 6px 24px color-mix(in oklab, black 28%, transparent);
		user-select: none;
		-webkit-user-select: none;
	}
	.thumb {
		position: relative;
		flex: 0 0 124px;
		height: 124px;
		border-radius: 8px;
		overflow: hidden;
		background-color: color-mix(in oklab, var(--ca-text-primary) 6%, transparent);
		background-image:
			linear-gradient(45deg, var(--line) 25%, transparent 25%, transparent 75%, var(--line) 75%),
			linear-gradient(45deg, var(--line) 25%, transparent 25%, transparent 75%, var(--line) 75%);
		background-size: 12px 12px;
		background-position:
			0 0,
			6px 6px;
	}
	/* The GIF sheet needs the room the thumbnail leaves when it shrinks. */
	.thumb.small {
		flex-basis: 56px;
		height: 56px;
	}
	.thumb img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: contain;
		cursor: grab;
	}
	.x {
		position: absolute;
		top: 4px;
		left: 4px;
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-surface-raised) 85%, transparent);
		box-shadow: inset 0 0 0 1px var(--line);
		cursor: pointer;
		opacity: 0;
	}
	.x.inline {
		position: static;
		margin-left: auto;
		flex: none;
	}
	.card:hover .x,
	.x:focus-visible {
		opacity: 1;
	}
	.badge {
		position: absolute;
		right: 4px;
		bottom: 4px;
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 2px 7px;
		border-radius: 999px;
		font-size: 11px;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-surface-raised) 88%, transparent);
		box-shadow: inset 0 0 0 1px var(--line);
	}
	.badge.inline {
		position: static;
		align-self: flex-start;
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 5px;
		color: var(--ca-text-secondary);
	}
	.title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.text {
		flex: 1 1 auto;
		min-height: 0;
		overflow: hidden;
		padding: 6px 8px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-text-primary) 6%, transparent);
		user-select: text;
		-webkit-user-select: text;
	}
	.preview {
		margin: 0;
		font:
			11px/1.4 ui-monospace,
			SFMono-Regular,
			Menlo,
			monospace;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.none {
		margin: 0;
		color: var(--ca-text-secondary);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-top: auto;
	}
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		height: 24px;
		padding: 0 8px;
		border: 0;
		border-radius: 6px;
		font: inherit;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
		cursor: pointer;
	}
	.btn:hover:not(:disabled) {
		background: color-mix(in oklab, var(--ca-text-primary) 14%, transparent);
	}
	.btn:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.btn.primary {
		color: var(--ca-brand-contrast);
		background: var(--ca-accent, #3b82f6);
	}
	.btn.icon {
		padding: 0 6px;
	}
	.btn:focus-visible,
	.item:focus-visible,
	.confirm:focus-visible,
	.preset:has(input:focus-visible) {
		outline: 2px solid var(--ca-accent, #3b82f6);
		outline-offset: 1px;
	}
	.more {
		position: relative;
	}
	.menu {
		position: absolute;
		right: 0;
		bottom: calc(100% + 4px);
		z-index: 1;
		display: flex;
		flex-direction: column;
		min-width: 160px;
		padding: 4px;
		border-radius: 8px;
		background: var(--ca-surface-raised);
		box-shadow:
			inset 0 0 0 1px var(--line),
			0 6px 18px color-mix(in oklab, black 28%, transparent);
	}
	.item {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 8px;
		border: 0;
		border-radius: 5px;
		font: inherit;
		text-align: left;
		color: var(--ca-text-primary);
		background: transparent;
		cursor: pointer;
	}
	.item:hover {
		background: color-mix(in oklab, var(--ca-text-primary) 10%, transparent);
	}
	.item.danger,
	.confirm {
		color: var(--ca-danger, #ef4444);
	}
	.confirm {
		padding: 4px 8px;
		border: 0;
		border-radius: 6px;
		font: inherit;
		background: color-mix(in oklab, var(--ca-danger, #ef4444) 14%, transparent);
		cursor: pointer;
	}
	.err {
		display: flex;
		align-items: flex-start;
		gap: 4px;
		margin: 0;
		color: var(--ca-danger, #ef4444);
		line-height: 1.35;
		user-select: text;
		-webkit-user-select: text;
	}
	.sheet {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-height: 0;
	}
	.presets {
		display: flex;
		flex-wrap: wrap;
		gap: 2px 6px;
	}
	.preset {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 2px 4px;
		border-radius: 5px;
		cursor: pointer;
	}
	.est {
		display: flex;
		align-items: center;
		gap: 4px;
		min-height: 16px;
		margin: 0;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	.bar {
		width: 100%;
		height: 6px;
	}
	.row {
		display: flex;
		justify-content: flex-end;
		gap: 4px;
	}
	:global(.spin) {
		animation: spin 0.9s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		:global(.spin) {
			animation: none;
		}
	}
</style>
