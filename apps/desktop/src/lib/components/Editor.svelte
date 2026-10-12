<script lang="ts">
	import { onMount } from "svelte";
	import { EditorView, basicSetup } from "codemirror";
	import { Compartment, EditorState } from "@codemirror/state";
	import { keymap } from "@codemirror/view";
	import { toast } from "@codeatlasdev/ui";
	import {
		WrapText,
		FileDiff as DiffIcon,
		Eye,
		Code,
		Image as ImageIcon,
		FolderSearch,
		ExternalLink,
	} from "@lucide/svelte";
	import { atlasTheme } from "#lib/cm.ts";
	import { host, b64bytes } from "#lib/host.ts";
	import { api, errText } from "#lib/api.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import DiffView from "#lib/components/DiffView.svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { registerEditor, unregisterEditor } from "#lib/symbols.ts";
	import { MAX_BYTES, MIME, extOf, viewerKind } from "#lib/viewers/kind.ts";
	import { readBlob } from "#lib/viewers/load.ts";
	import { langForPath } from "#lib/viewers/lang.ts";
	import ImageViewer from "#lib/viewers/ImageViewer.svelte";
	import PdfViewer from "#lib/viewers/PdfViewer.svelte";
	import MediaViewer from "#lib/viewers/MediaViewer.svelte";
	import MarkdownViewer from "#lib/viewers/MarkdownViewer.svelte";
	import FallbackViewer from "#lib/viewers/FallbackViewer.svelte";

	let {
		worktreeId,
		path,
		focused = false,
	}: { worktreeId: string; path: string; focused?: boolean } = $props();

	const SAVE_IDLE_MS = 500;
	const HEX_BYTES = 4096;

	let el: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined;
	let phase = $state<"loading" | "text" | "media" | "large" | "binary" | "error">("loading");
	let error = $state("");
	let progress = $state(0);
	let fileSize = $state(0);
	let head = $state<Uint8Array | undefined>();
	let media = $state<{ url: string; blob: Blob } | null>(null);
	let wrap = $state(false);
	let rich = $state(false);
	/** SVG: rendered image (true) or source in the editor (false). */
	let rendered = $state(true);
	let changes = $state(false);
	let text = $state("");
	const wrapC = new Compartment();
	let saveTimer: number | undefined;
	let lastSaved = "";
	const abort = new AbortController();

	let root = $derived(ws.worktree(worktreeId)?.path ?? "");
	let abs = $derived(`${root}/${path}`);
	let kind = $derived(viewerKind(path));
	let isMd = $derived(kind === "markdown");
	let isSvg = $derived(kind === "svg");
	/** Text-backed view shown right now (CodeMirror is mounted but hidden otherwise). */
	let showCm = $derived(phase === "text" && !changes && !(isMd && rich) && !(isSvg && rendered));

	async function save() {
		const cur = view?.state.doc.toString();
		if (cur === undefined || cur === lastSaved) return;
		try {
			await host.fsWrite(abs, cur);
			lastSaved = cur;
		} catch (e) {
			toast.error(`Save failed: ${errText(e)}`);
		}
	}

	function mountEditor() {
		view = new EditorView({
			parent: el!,
			state: EditorState.create({
				doc: text,
				extensions: [
					basicSetup,
					atlasTheme,
					langForPath(path),
					wrapC.of([]),
					keymap.of([
						{
							key: "Alt-z",
							run: () => {
								toggleWrap();
								return true;
							},
						},
						{
							key: "Mod-s",
							run: () => {
								void save();
								return true;
							},
						},
					]),
					EditorView.updateListener.of((u) => {
						if (!u.docChanged) return;
						text = u.state.doc.toString();
						clearTimeout(saveTimer);
						saveTimer = window.setTimeout(save, SAVE_IDLE_MS);
					}),
				],
			}),
		});
		registerEditor(`${worktreeId}:${path}`, view, save);
	}

	async function load() {
		const st = await host.fsStat(abs);
		fileSize = st.size;
		const max = MAX_BYTES[kind];
		if (st.size > max) {
			phase = "large";
			return;
		}
		if (kind === "image" || kind === "pdf" || kind === "video" || kind === "audio") {
			const blob = await readBlob(abs, st.size, MIME[extOf(path)], (p) => (progress = p), abort.signal);
			media = { url: URL.createObjectURL(blob), blob };
			phase = "media";
			return;
		}
		const r = await host.fsRead(abs, max);
		const bytes = b64bytes(r.data);
		try {
			text = lastSaved = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
		} catch {
			head = bytes.subarray(0, HEX_BYTES);
			phase = "binary";
			return;
		}
		if (isSvg) {
			media = {
				blob: new Blob([bytes as Uint8Array<ArrayBuffer>], { type: MIME.svg }),
				url: "",
			};
			media.url = URL.createObjectURL(media.blob);
		}
		phase = "text";
		await Promise.resolve();
		mountEditor();
	}

	onMount(() => {
		load().catch((e) => {
			if (e?.name === "AbortError") return;
			error = errText(e);
			phase = "error";
		});
		return () => {
			abort.abort();
			clearTimeout(saveTimer);
			void save();
			if (view) unregisterEditor(`${worktreeId}:${path}`, view);
			view?.destroy();
			if (media?.url) URL.revokeObjectURL(media.url);
		};
	});

	// Keep the SVG preview in sync with edits.
	$effect(() => {
		if (!isSvg || phase !== "text" || !rendered) return;
		const blob = new Blob([text], { type: MIME.svg });
		const url = URL.createObjectURL(blob);
		const old = media?.url;
		media = { blob, url };
		if (old) setTimeout(() => URL.revokeObjectURL(old), 1000);
	});

	$effect(() => {
		if (focused && showCm) view?.focus();
	});

	function toggleWrap() {
		wrap = !wrap;
		view?.dispatch({ effects: wrapC.reconfigure(wrap ? EditorView.lineWrapping : []) });
	}

	const reveal = () => host.exec(root, ["open", "-R", abs]).catch((e) => toast.error(errText(e)));
	const openExternal = () => host.exec(root, ["open", abs]).catch((e) => toast.error(errText(e)));

	function onkeydown(e: KeyboardEvent) {
		if (!focused) return;
		if (e.metaKey && e.shiftKey && e.key.toLowerCase() === "m" && isMd) {
			rich = !rich;
			e.preventDefault();
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="editor">
	<div class="bar">
		<button class="path mono" title="Reveal in Finder" onclick={reveal}>
			{path}
		</button>
		{#if phase === "text"}
			{#if isSvg}
				<Tip label={rendered ? "Edit source" : "Show image"} side="bottom">
					{#snippet child(props)}
						<button {...props} class:on={!rendered} onclick={() => (rendered = !rendered)}>
							{#if rendered}<Code size={13} />{:else}<ImageIcon size={13} />{/if}
						</button>
					{/snippet}
				</Tip>
			{/if}
			{#if showCm || changes}
				<button class:on={wrap} title="Word wrap (⌥Z)" onclick={toggleWrap}><WrapText size={13} /></button>
				<button class:on={changes} title="Changes vs HEAD" onclick={() => (changes = !changes)}>
					<DiffIcon size={13} />
				</button>
			{/if}
			{#if isMd}
				<button class:on={rich} title="Rich / Raw (⌘⇧M)" onclick={() => (rich = !rich)}>
					{#if rich}<Code size={13} />{:else}<Eye size={13} />{/if}
				</button>
			{/if}
		{/if}
		{#if phase === "media" || isSvg}
			<Tip label="Reveal in Finder" side="bottom">
				{#snippet child(props)}
					<button {...props} onclick={reveal}><FolderSearch size={13} /></button>
				{/snippet}
			</Tip>
			<Tip label="Open externally" side="bottom">
				{#snippet child(props)}
					<button {...props} onclick={openExternal}><ExternalLink size={13} /></button>
				{/snippet}
			</Tip>
		{/if}
	</div>
	{#if phase === "loading"}
		<div class="msg" aria-busy="true">
			Loading…
			{#if progress > 0 && progress < 1}<span class="mono">{Math.round(progress * 100)}%</span>{/if}
		</div>
	{:else if phase === "large" || phase === "binary"}
		<FallbackViewer reason={phase} size={fileSize} {head} onopen={openExternal} onreveal={reveal} />
	{:else if phase === "error"}
		<p class="msg">{error}</p>
	{:else if phase === "media" && media}
		{#if kind === "image"}
			<ImageViewer {...media} {path} size={fileSize} {focused} />
		{:else if kind === "pdf"}
			<PdfViewer blob={media.blob} {path} {focused} onopen={openExternal} onreveal={reveal} />
		{:else}
			<MediaViewer url={media.url} {path} size={fileSize} video={kind === "video"} />
		{/if}
	{:else if phase === "text" && isSvg && rendered && !changes && media}
		<ImageViewer {...media} {path} size={fileSize} {focused} />
	{/if}
	{#if changes}
		<div class="fill"><DiffView {worktreeId} only={path} mode="working" /></div>
	{:else if rich && isMd && phase === "text"}
		<MarkdownViewer {text} {path} {root} />
	{/if}
	<div class="cm" class:hidden={!showCm} bind:this={el}></div>
</div>

<style>
	.editor {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.path {
		margin-right: auto;
		border: 0;
		background: none;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.bar button:not(.path) {
		display: inline-flex;
		padding: 4px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-secondary);
	}
	.bar button.on {
		color: var(--space-accent, var(--ca-brand-main));
		background: var(--ca-action-hover);
	}
	.mono {
		font-family: var(--mono);
	}
	.cm,
	.fill {
		flex: 1;
		min-height: 0;
		overflow: hidden;
		display: flex;
	}
	.cm.hidden {
		display: none;
	}
	.msg {
		display: flex;
		gap: 8px;
		padding: 20px;
		color: var(--ca-text-secondary);
		font-size: 13px;
	}
</style>
