<script lang="ts">
	import { ChevronRight } from "@lucide/svelte";
	import type { EditorView } from "@codemirror/view";
	import { revealLabel } from "#lib/keys.ts";
	import {
		breadcrumbPath,
		nestFlat,
		pathSegments,
		lspTree,
		type LspDocSymbol,
		type Sym,
	} from "#lib/editor/breadcrumbs.ts";
	import { pathToUri, posToOffset } from "#lib/lsp/convert.ts";
	import { ops } from "#lib/symbols.ts";
	import type { LspDoc } from "#lib/lsp/client.svelte.ts";

	let {
		view,
		path,
		line,
		doc,
		onreveal,
	}: {
		view: EditorView | undefined;
		/** Worktree-relative path. */
		path: string;
		/** Caret line (1-based): the trail follows it. */
		line: number;
		/** The LSP document, when a server serves the file (its symbols are exact). */
		doc?: LspDoc;
		onreveal: () => void;
	} = $props();

	const REFRESH_MS = 600;
	let symbols = $state<Sym[]>([]);
	let timer: number | undefined;

	async function refresh() {
		const v = view;
		if (!v) return;
		if (doc?.open && doc.caps?.documentSymbolProvider) {
			const res = await doc
				.request<LspDocSymbol[]>("textDocument/documentSymbol", {
					textDocument: { uri: pathToUri(doc.path) },
				})
				.catch(() => null);
			// A flat `SymbolInformation[]` reply has no ranges nesting: fall through to the syntax outline.
			if (res?.length && "range" in res[0]) {
				symbols = lspTree(res, (p) => posToOffset(v.state.doc, p));
				return;
			}
		}
		symbols = nestFlat(ops.outline(v.state), v.state.doc.length);
	}

	// Recompute shortly after the document or the server changes; never on every keystroke.
	$effect(() => {
		void view;
		void doc?.open;
		void doc?.caps;
		void line;
		clearTimeout(timer);
		timer = window.setTimeout(() => void refresh(), REFRESH_MS);
		return () => clearTimeout(timer);
	});

	let trail = $derived(view ? breadcrumbPath(symbols, view.state.selection.main.head) : []);
	let files = $derived(pathSegments(path));

	function jump(from: number | undefined) {
		if (from !== undefined && view) ops.reveal(view, from);
	}
</script>

<nav class="crumbs" aria-label="Breadcrumbs">
	{#each files as seg, i (seg.path)}
		{#if i > 0}<ChevronRight size={11} class="sep" />{/if}
		{#if seg.kind === "file"}
			<button class="seg file" title={revealLabel()} onclick={onreveal}>{seg.name}</button>
		{:else}
			<span class="seg dir">{seg.name}</span>
		{/if}
	{/each}
	{#each trail as seg, i (`${i}:${seg.from}`)}
		<ChevronRight size={11} class="sep" />
		<button class="seg sym" title={seg.symbolKind} onclick={() => jump(seg.from)}>{seg.name}</button>
	{/each}
</nav>

<style>
	.crumbs {
		display: flex;
		align-items: center;
		gap: 2px;
		min-height: 22px;
		padding: 0 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
	}
	.crumbs :global(.sep) {
		flex-shrink: 0;
		color: var(--ca-text-disabled);
	}
	.seg {
		border: 0;
		background: none;
		padding: 1px 3px;
		border-radius: 4px;
		font: inherit;
		color: inherit;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	button.seg:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.file,
	.sym:last-child {
		color: var(--ca-text-primary);
	}
</style>
