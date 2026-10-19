<script lang="ts">
	import { onMount, tick } from "svelte";
	import { kbd, primaryKey } from "#lib/keys.ts";
	import { EditorView } from "@codemirror/view";
	import { toast } from "@codeatlasdev/ui";
	import {
		WrapText,
		FileDiff as DiffIcon,
		Eye,
		Code,
		Image as ImageIcon,
		FolderSearch,
		ExternalLink,
		Globe,
		Undo2,
	} from "@lucide/svelte";
	import { host } from "#lib/host.ts";
	import { errText } from "#lib/api.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import DiffView from "#lib/components/DiffView.svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { previewHtml } from "#lib/stores/preview.svelte.ts";
	import { isHtml } from "#lib/stores/preview.ts";
	import "#lib/outline.ts";
	import { registerEditor, unregisterEditor } from "#lib/symbols.ts";
	import { LspDoc } from "#lib/lsp/client.svelte.ts";
	import { lspGated } from "#lib/lsp/uiState.svelte.ts";
	import { applyDiagnostics, countDiagnostics } from "#lib/lsp/diagnostics.ts";
	import { formatBeforeSave, lspCompletion, lspExtension } from "#lib/lsp/extension.ts";
	import LspStatus from "#lib/lsp/LspStatus.svelte";
	import { openLocation, readOnlyPaths, showLocations } from "#lib/lsp/workspace.ts";
	import { editorKey, fileOfTab, readOnlyExtension } from "#lib/lsp/navigation.ts";
	import { navigator as nav } from "#lib/lsp/history.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { EditorSession } from "#lib/editor/EditorSession.svelte.ts";
	import { registerSession } from "#lib/editor/registry.ts";
	import { editorPrefs } from "#lib/editor/settings.ts";
	import { runStatusAction } from "#lib/editor/statusActions.ts";
	import { convertIndentation } from "#lib/editor/commands.ts";
	import StatusBar from "#lib/editor/StatusBar.svelte";
	import Breadcrumbs from "#lib/editor/Breadcrumbs.svelte";
	import { gotoLine } from "@codemirror/search";
	import { MAX_BYTES, MIME, extOf, viewerKind } from "#lib/viewers/kind.ts";
	import { readBlob } from "#lib/viewers/load.ts";
	import { loadLanguage, lspLanguageId } from "#lib/viewers/lang.ts";
	import { fileType } from "#lib/viewers/fileType.ts";
	import { rememberRich, wantsRich } from "#lib/viewers/richPref.ts";
	import RichView from "#lib/viewers/rich/RichView.svelte";
	import ImageViewer from "#lib/viewers/ImageViewer.svelte";
	import SvgPreview from "#lib/viewers/SvgPreview.svelte";
	import PdfViewer from "#lib/viewers/PdfViewer.svelte";
	import MediaViewer from "#lib/viewers/MediaViewer.svelte";
	import MarkdownViewer from "#lib/viewers/MarkdownViewer.svelte";
	import FallbackViewer from "#lib/viewers/FallbackViewer.svelte";

	let {
		worktreeId,
		path,
		focused = false,
	}: { worktreeId: string; path: string; focused?: boolean } = $props();

	let el: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined = $state.raw();
	/** Non-text kinds and load failures; text uses the session's phase. */
	let own = $state<"media" | "error" | null>(null);
	let error = $state("");
	let progress = $state(0);
	let media = $state<{ url: string; blob: Blob } | null>(null);
	let rich = $state(false);
	/** SVG: rendered image (true) or source in the editor (false). */
	let rendered = $state(true);
	let changes = $state(false);
	let lspDoc = $state<LspDoc | undefined>();
	let problems = $state({ error: 0, warning: 0 });
	const abort = new AbortController();

	let root = $derived(ws.worktree(worktreeId)?.path ?? "");
	let file = $derived(fileOfTab(root, path));
	let abs = $derived(file.abs);
	/** Dependency and external sources (node_modules, Rust std, cargo registry): view, never write. */
	let readOnly = $derived(file.external || readOnlyPaths.has(editorKey(worktreeId, path)));
	let kind = $derived(viewerKind(path));
	let ft = $derived(fileType(path));
	let isMd = $derived(kind === "markdown");
	let isSvg = $derived(kind === "svg");
	/** Rich view of a non-Markdown file type (Markdown, SVG keep their own viewers). */
	let richKind = $derived(kind === "text" ? ft.view : undefined);
	let hasRich = $derived(isMd || !!richKind);

	// svelte-ignore state_referenced_locally
	const session = new EditorSession(abs, path, root, {
		// Dependency and external sources: browsable, never writable (see `navigation.ts`).
		extensions: () => (readOnly ? readOnlyExtension() : []),
		onChange: (u) => lspDoc?.change(u.startState.doc, u.changes),
		onConflict: () => {},
	});
	let phase = $derived(own ?? session.phase);
	let text = $derived(session.text);
	let fileSize = $derived(session.fileSize);
	let head = $derived(session.head);
	let wrap = $derived(session.wrap);
	/** Text-backed view shown right now (CodeMirror is mounted but hidden otherwise). */
	let showCm = $derived(
		phase === "text" && !changes && !(isMd && rich) && !(!!richKind && rich) && !(isSvg && rendered),
	);
	let languageId = $derived(lspLanguageId(path) ?? ft.lang ?? "plaintext");

	/** Format on save (language server), then write. Returns once the write finished. */
	const save = () =>
		session.save(async () => {
			if (view && lspDoc?.open) await formatBeforeSave(view);
		});

	/** Start (or stop) the language server session for this document per the settings. */
	function syncLsp() {
		const id = lspLanguageId(path);
		const want = !!view && !!id && kind === "text";
		if (!want) {
			lspDoc?.close();
			lspDoc = undefined;
			problems = { error: 0, warning: 0 };
			view?.dispatch({ effects: session.lspC.reconfigure([]) });
			return;
		}
		if (lspDoc) return;
		const v = view!;
		const doc = new LspDoc(
			abs,
			id,
			() => v.state.doc.toString(),
			root,
			() => lspGated(id),
		);
		doc.listenDiagnostics((version, diags) => {
			applyDiagnostics(v, diags, version, doc.version);
			const c = countDiagnostics(v);
			problems = { error: c.error, warning: c.warning };
		});
		lspDoc = doc;
		v.dispatch({
			effects: session.lspC.reconfigure([
				lspCompletion,
				lspExtension({
					env: {
						doc,
						openAt: (p, pos, end) => {
							const v2 = view;
							if (v2) nav.leave(abs, v2);
							if (!openLocation(p, pos, worktreeId, end)) toast.info(`Could not open ${p}`);
						},
						showLocations,
						inlayHints: () => prefs.value.inlay_hints.enabled,
						formatOnSave: () => prefs.value.format_on_save,
					},
				}),
			]),
		});
	}

	async function load() {
		const st = await host.fsStat(abs);
		const max = MAX_BYTES[kind];
		if (st.size > max) {
			session.fileSize = st.size;
			session.phase = "large";
			return;
		}
		if (kind === "image" || kind === "pdf" || kind === "video" || kind === "audio") {
			session.fileSize = st.size;
			const blob = await readBlob(abs, st.size, MIME[extOf(path)], (p) => (progress = p), abort.signal);
			media = { url: URL.createObjectURL(blob), blob };
			own = "media";
			return;
		}
		const plan = await session.read(max, abort.signal);
		if (!plan) return;
		rich = wantsRich(ft);
		session.phase = "text";
		await Promise.resolve();
		const v = session.mount(el!, plan);
		view = v;
		registerEditor(`${worktreeId}:${path}`, v, () => save(), { path, root });
		registerSession(v, session);
		syncLsp();
	}

	onMount(() => {
		load().catch((e) => {
			if (e?.name === "AbortError") return;
			error = errText(e);
			own = "error";
		});
		return () => {
			abort.abort();
			const v = view;
			void session
				.dispose(() => {
					if (v) unregisterEditor(`${worktreeId}:${path}`, v);
				})
				.finally(() => lspDoc?.close());
			if (media?.url) URL.revokeObjectURL(media.url);
		};
	});
	$effect(() => {
		if (focused && showCm) view?.focus();
	});
	// Grammar for the file type: bundled ones are instant, the rest load as separate chunks.
	$effect(() => {
		const id = ft.lang;
		const v = view;
		if (!v) return;
		let live = true;
		// Huge files stay plain text: a grammar would parse the whole document in the background.
		const grammar = session.plan?.large.disable.syntax === "none" ? Promise.resolve([]) : loadLanguage(id);
		void grammar.then((ext) => {
			if (live && view === v) v.dispatch({ effects: session.langC.reconfigure(ext) });
		});
		return () => (live = false);
	});
	$effect(() => {
		void prefs.value.enable_language_servers;
		syncLsp();
	});
	// Editor preferences (tab size, guides, rulers, wrap, …) apply live to the open view.
	$effect(() => {
		void editorPrefs();
		session.applyPrefs();
	});

	const reveal = () => host.exec(root, ["open", "-R", abs]).catch((e) => toast.error(errText(e)));
	const openExternal = () => host.exec(root, ["open", abs]).catch((e) => toast.error(errText(e)));

	function setRich(on: boolean) {
		rich = on;
		if (richKind) rememberRich(ft, on);
	}

	/** Leave the rendered view for the source, optionally landing on a line. */
	function toSource(line?: number) {
		setRich(false);
		if (!line) return;
		void tick().then(() => {
			const v = view;
			if (!v) return;
			const l = v.state.doc.line(Math.min(Math.max(line, 1), v.state.doc.lines));
			v.dispatch({
				selection: { anchor: l.from },
				effects: EditorView.scrollIntoView(l.from, { y: "center" }),
			});
			v.focus();
		});
	}

	function onkeydown(e: KeyboardEvent) {
		if (!focused) return;
		if (primaryKey(e) && e.shiftKey && e.key.toLowerCase() === "m" && hasRich && phase === "text") {
			setRich(!rich);
			e.preventDefault();
		}
		if (e.altKey && !e.metaKey && !e.ctrlKey && e.code === "KeyZ" && showCm) {
			session.setWrap(!session.wrap);
			e.preventDefault();
		}
	}

	let status = $derived(
		session.status({ id: languageId }, { errors: problems.error, warnings: problems.warning }),
	);

	function statusAction(id: string) {
		const v = view;
		if (!v) return;
		runStatusAction(id, {
			indent: session.plan?.indent ?? { style: "space", size: 4 },
			eol: status.eol,
			setIndent: (style, size) => session.setIndent(style, size),
			convertIndent: (style) => convertIndentation(style)(v),
			setEol: (eol) => session.setEol(eol),
			gotoLine: () => {
				v.focus();
				gotoLine(v);
			},
			showProblems: () => v.focus(),
		});
	}
</script>

<svelte:window {onkeydown} />

<div class="editor">
	<div class="bar">
		<span class="grow"></span>
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
			{#if session.dirty}
				<Tip label="Revert file" side="bottom">
					{#snippet child(props)}
						<button {...props} onclick={() => void session.revert()}><Undo2 size={13} /></button>
					{/snippet}
				</Tip>
			{/if}
			{#if showCm || changes}
				<button class:on={wrap} title="Word wrap (⌥Z)" onclick={() => session.setWrap(!session.wrap)}
					><WrapText size={13} /></button
				>
				<button class:on={changes} title="Changes vs HEAD" onclick={() => (changes = !changes)}>
					<DiffIcon size={13} />
				</button>
			{/if}
			{#if isHtml(path)}
				<Tip label={`Open preview (${kbd("shift-cmd-v")})`} side="bottom">
					{#snippet child(props)}
						<button {...props} onclick={() => void previewHtml(worktreeId, path)}><Globe size={13} /></button>
					{/snippet}
				</Tip>
			{/if}
			{#if hasRich}
				<Tip
					label={rich
						? `Show source (${kbd("shift-cmd-m")})`
						: `${isMd ? "Rich" : ft.label} view (${kbd("shift-cmd-m")})`}
					side="bottom"
				>
					{#snippet child(props)}
						<button {...props} class:on={rich} aria-pressed={rich} onclick={() => setRich(!rich)}>
							{#if rich}<Code size={13} />{:else}<Eye size={13} />{/if}
						</button>
					{/snippet}
				</Tip>
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
	{#if showCm}
		<Breadcrumbs {view} {path} line={session.cursor.line} doc={lspDoc} onreveal={reveal} />
	{:else}
		<div class="pathbar">
			<button class="path mono" title="Reveal in Finder" onclick={reveal}>{path}</button>
		</div>
	{/if}
	{#if session.plan?.large.notice && showCm}
		<p class="notice" role="note">{session.plan.large.notice}</p>
	{/if}
	{#if session.conflict}
		<div class="banner" role="alert">
			<span>This file changed on disk and you have unsaved edits.</span>
			<button onclick={() => void session.resolve("reload")}>Load from disk</button>
			<button onclick={() => void session.resolve("keep")}>Keep my version</button>
		</div>
	{:else if session.deleted}
		<div class="banner" role="alert">
			<span>This file was deleted on disk. Saving writes it again.</span>
			<button onclick={() => void save()}>Save</button>
		</div>
	{/if}
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
	{:else if phase === "text" && isSvg && rendered && !changes}
		<SvgPreview {text} {path} size={fileSize} {focused} />
	{/if}
	{#if changes}
		<div class="fill"><DiffView {worktreeId} only={path} mode="working" /></div>
	{:else if rich && isMd && phase === "text"}
		<MarkdownViewer {text} {path} {root} />
	{:else if rich && richKind && phase === "text"}
		<RichView view={richKind} ctx={{ text, path, worktreeId, root, size: fileSize, toSource }} />
	{/if}
	<div class="cm" class:hidden={!showCm} bind:this={el}></div>
	{#if showCm}
		<StatusBar input={status} onaction={statusAction}>
			{#snippet lsp()}
				{#if lspDoc}<LspStatus doc={lspDoc} errors={0} warnings={0} />{/if}
			{/snippet}
		</StatusBar>
	{/if}
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
		padding: 2px 10px;
	}
	.grow {
		margin-right: auto;
	}
	.pathbar {
		padding: 0 10px 4px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.path {
		border: 0;
		background: none;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.bar button {
		display: inline-flex;
		padding: 4px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-secondary);
	}
	.bar button.on {
		color: var(--space-accent-text, var(--ca-brand-main));
		background: var(--ca-action-hover);
	}
	.mono {
		font-family: var(--mono);
	}
	.cm,
	.fill {
		flex: 1;
		min-height: 0;
		min-width: 0;
		overflow: hidden;
	}
	.fill {
		display: flex;
	}
	/* A block host: the CodeMirror view fills it (width/height 100%, see cm.ts), never the longest line. */
	.cm {
		display: block;
		contain: strict;
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
	.notice,
	.banner {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 0;
		padding: 4px 10px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-semantic-warning) 12%, transparent);
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.banner button {
		border: 0.5px solid var(--ca-outline-default);
		border-radius: 5px;
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
		padding: 1px 8px;
		font: inherit;
	}
</style>
