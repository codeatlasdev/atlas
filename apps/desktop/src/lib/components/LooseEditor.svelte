<script lang="ts">
	/**
	 * One loose file in the light editor: the same CodeMirror session as the project editor (autosave, external-change
	 * handling, editorconfig, large-file tiers) and nothing around it. No language server, no git, no worktree, no
	 * breadcrumbs, no project: opening it reads the file, builds the view and paints.
	 */
	import { onMount } from "svelte";
	import { EditorView } from "@codemirror/view";
	import { toast } from "@codeatlasdev/ui";
	import { host } from "#lib/host.ts";
	import { looseIo } from "#lib/looseIo.ts";
	import { errText } from "#lib/api.ts";
	import { primaryKey } from "#lib/keys.ts";
	import { baseName, dirOf } from "#lib/openPath.ts";
	import { EditorSession } from "#lib/editor/EditorSession.svelte.ts";
	import { editorPrefs } from "#lib/editor/settings.ts";
	import { loose } from "#lib/stores/loose.svelte.ts";
	import { MAX_BYTES, viewerKind } from "#lib/viewers/kind.ts";
	import { loadLanguage } from "#lib/viewers/lang.ts";
	import { fileType } from "#lib/viewers/fileType.ts";
	import FallbackViewer from "#lib/viewers/FallbackViewer.svelte";
	import MarkdownViewer from "#lib/viewers/MarkdownViewer.svelte";
	import MarkdownBar from "./MarkdownBar.svelte";
	import { m } from "#lib/i18n/m.ts";

	/** Absolute path of the file. */
	let { path }: { path: string } = $props();

	// svelte-ignore state_referenced_locally
	const abs = path;
	const name = baseName(abs);
	const root = dirOf(abs);
	const kind = viewerKind(abs);
	const isMd = kind === "markdown";
	const ft = fileType(abs);

	let el: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined = $state.raw();
	let error = $state("");
	let preview = $state(false);
	const abort = new AbortController();

	const session = new EditorSession(
		abs,
		name,
		root,
		{ extensions: () => [], onChange: () => {}, onConflict: () => {} },
		looseIo,
	);
	let phase = $derived(error ? "error" : session.phase);
	let showCm = $derived(phase === "text" && !preview);
	let cursor = $derived(session.cursor);

	/** Time to first paint of the file, readable with `performance.getEntriesByName("loose:first-paint")`. */
	const opened = performance.now();

	async function load() {
		const st = await looseIo.fsStat(abs);
		const max = MAX_BYTES[kind];
		if (st.size > max) {
			session.fileSize = st.size;
			session.phase = "large";
			return;
		}
		const plan = await session.read(max, abort.signal);
		if (!plan) return;
		session.phase = "text";
		await Promise.resolve();
		const v = session.mount(el!, plan);
		view = v;
		const line = loose.takeLine(abs);
		if (line) {
			const l = v.state.doc.line(Math.min(Math.max(line, 1), v.state.doc.lines));
			v.dispatch({
				selection: { anchor: l.from },
				effects: EditorView.scrollIntoView(l.from, { y: "center" }),
			});
		}
		v.focus();
		requestAnimationFrame(() =>
			requestAnimationFrame(() =>
				performance.measure("loose:first-paint", { start: opened, detail: { path: abs, size: st.size } }),
			),
		);
	}

	onMount(() => {
		load().catch((e) => {
			if (e?.name !== "AbortError") error = errText(e);
		});
		return () => {
			abort.abort();
			void session.dispose();
		};
	});

	// Grammar for the file type: bundled ones are instant, the rest load as separate chunks. Huge files stay plain.
	$effect(() => {
		const v = view;
		if (!v) return;
		let live = true;
		const grammar =
			session.plan?.large.disable.syntax === "none" ? Promise.resolve([]) : loadLanguage(ft.lang);
		void grammar.then((ext) => {
			if (live && view === v) v.dispatch({ effects: session.langC.reconfigure(ext) });
		});
		return () => (live = false);
	});
	// Editor preferences (tab size, guides, wrap, …) apply live to the open view.
	$effect(() => {
		void editorPrefs();
		session.applyPrefs();
	});

	const reveal = () => host.exec(root, ["open", "-R", abs]).catch((e) => toast.error(errText(e)));
	const openExternal = () => host.exec(root, ["open", abs]).catch((e) => toast.error(errText(e)));

	/** ⌘S saves now instead of waiting for the idle autosave. */
	function onkeydown(e: KeyboardEvent) {
		if (primaryKey(e) && !e.shiftKey && e.key.toLowerCase() === "s" && phase === "text") {
			e.preventDefault();
			void session.save();
		}
	}
</script>

<svelte:window {onkeydown} />

<div class="loose">
	{#if isMd && phase === "text"}
		<MarkdownBar {view} {preview} onpreview={(on) => (preview = on)} />
	{/if}
	{#if session.watchError}
		<p class="notice" role="status">{m.workbench_editor_watch_failed({ error: session.watchError })}</p>
	{/if}
	{#if session.conflict}
		<div class="banner" role="alert">
			<span>{m.workbench_loose_editor_this_file_changed_on_disk()}</span>
			<button onclick={() => void session.resolve("reload")}
				>{m.workbench_loose_editor_load_from_disk()}</button
			>
			<button onclick={() => void session.resolve("keep")}
				>{m.workbench_loose_editor_keep_my_version()}</button
			>
		</div>
	{:else if session.deleted}
		<div class="banner" role="alert">
			<span>{m.workbench_loose_editor_this_file_was_deleted_on()}</span>
			<button onclick={() => void session.save()}>{m.common_action_save()}</button>
		</div>
	{/if}
	{#if phase === "loading"}
		<div class="msg" aria-busy="true">{m.common_state_loading()}</div>
	{:else if phase === "large" || phase === "binary"}
		<FallbackViewer
			reason={phase}
			size={session.fileSize}
			head={session.head}
			onopen={openExternal}
			onreveal={reveal}
		/>
	{:else if phase === "error"}
		<p class="msg">{error}</p>
	{:else if preview && isMd}
		<MarkdownViewer text={session.text} path={name} {root} />
	{/if}
	<div class="cm" class:hidden={!showCm} bind:this={el}></div>
	{#if showCm}
		<footer>
			<span class="mono path" title={abs}>{abs}</span>
			<span class="grow"></span>
			<span
				>{session.dirty
					? m.workbench_loose_editor_state_edited()
					: m.workbench_loose_editor_state_saved()}</span
			>
			<span class="mono"
				>{m.workbench_loose_editor_ln_line_col_col({ line: cursor.line, col: cursor.col })}</span
			>
		</footer>
	{/if}
</div>

<style>
	.loose {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	.cm {
		flex: 1;
		min-height: 0;
		min-width: 0;
		display: block;
		overflow: hidden;
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
	footer {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 3px 10px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.path {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.grow {
		margin-right: auto;
	}
	.mono {
		font-family: var(--mono);
	}
</style>
