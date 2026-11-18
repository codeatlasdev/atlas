import {
	autocompletion,
	snippet,
	type Completion,
	type CompletionContext,
	type CompletionResult,
} from "@codemirror/autocomplete";
import { ChangeSet } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { renderMarkdown } from "#lib/viewers/markdown.ts";
import { envOf, posParams, type LspEnv } from "#lib/lsp/context.ts";
import { editsToChanges } from "#lib/lsp/changes.ts";
import { posToOffset, type Range, type TextEdit } from "#lib/lsp/convert.ts";
import { lspSnippetToCm } from "#lib/lsp/snippet.ts";

interface LspCompletionItem {
	label: string;
	labelDetails?: { detail?: string; description?: string };
	kind?: number;
	detail?: string;
	documentation?: string | { kind: string; value: string };
	sortText?: string;
	filterText?: string;
	insertText?: string;
	insertTextFormat?: 1 | 2;
	textEdit?: TextEdit | { newText: string; insert: Range; replace: Range };
	additionalTextEdits?: TextEdit[];
	preselect?: boolean;
	deprecated?: boolean;
	tags?: number[];
	commitCharacters?: string[];
	data?: unknown;
}

interface LspCompletionList {
	isIncomplete?: boolean;
	items: LspCompletionItem[];
	itemDefaults?: {
		editRange?: Range | { insert: Range; replace: Range };
		insertTextFormat?: 1 | 2;
	};
}

/** LSP `CompletionItemKind` -> CodeMirror's icon type (see `cm-completionIcon-*`). */
const KIND: Record<number, string> = {
	1: "text",
	2: "method",
	3: "function",
	4: "function",
	5: "property",
	6: "variable",
	7: "class",
	8: "interface",
	9: "namespace",
	10: "property",
	11: "constant",
	12: "constant",
	13: "enum",
	14: "keyword",
	15: "text",
	16: "constant",
	17: "text",
	18: "variable",
	19: "namespace",
	20: "enum",
	21: "constant",
	22: "type",
	23: "keyword",
	24: "keyword",
	25: "type",
};

const docHtml = (d: LspCompletionItem["documentation"], detail?: string) => {
	const md = [detail && `\`\`\`\n${detail}\n\`\`\``, typeof d === "string" ? d : d?.value]
		.filter(Boolean)
		.join("\n\n");
	return md ? renderMarkdown(md, "", true) : "";
};

/** Render markdown into a tooltip node. */
export function mdNode(md: string, cls = "cm-lsp-doc"): HTMLElement {
	const el = document.createElement("div");
	el.className = `${cls} md`;
	el.innerHTML = renderMarkdown(md, "", true);
	return el;
}

function editRangeOf(item: LspCompletionItem, list: LspCompletionList): Range | undefined {
	const te = item.textEdit;
	if (te) return "range" in te ? te.range : te.replace;
	const d = list.itemDefaults?.editRange;
	return d && "insert" in d ? d.replace : d;
}

export function toCompletion(
	view: EditorView,
	env: LspEnv,
	item: LspCompletionItem,
	list: LspCompletionList,
) {
	const isSnippet = (item.insertTextFormat ?? list.itemDefaults?.insertTextFormat) === 2;
	const te = item.textEdit;
	const newText = te?.newText ?? item.insertText ?? item.label;
	const range = editRangeOf(item, list);
	const deprecated = item.deprecated || item.tags?.includes(1);
	const c: Completion = {
		label: item.filterText ?? item.label,
		displayLabel: item.label,
		detail: item.labelDetails?.detail ?? item.detail,
		type: `${KIND[item.kind ?? 1] ?? "text"}${deprecated ? " deprecated" : ""}`,
		sortText: item.sortText,
		boost: item.preselect ? 99 : 0,
		apply: (v, completion, from, to) => {
			// The server's textEdit range wins over the word CodeMirror matched (it can start earlier,
			// e.g. `self.` or an import path). Extra edits (auto-import) are applied first and the main
			// range is mapped through them, so every offset stays valid.
			const doc = v.state.doc;
			const start = range ? posToOffset(doc, range.start) : from;
			const end = range ? Math.max(start, posToOffset(doc, range.end)) : to;
			const extra = ChangeSet.of(editsToChanges(doc, item.additionalTextEdits ?? []), doc.length);
			const s = extra.mapPos(start, -1);
			const e = Math.max(s, extra.mapPos(end, 1));
			const parsed = isSnippet ? lspSnippetToCm(newText) : null;
			if (parsed && !parsed.plain) {
				if (!extra.empty) v.dispatch({ changes: extra, userEvent: "input.complete" });
				snippet(parsed.template)(v, completion, s, e);
				return;
			}
			// Plain text (or a snippet without stops): unescape what `lspSnippetToCm` escaped for CodeMirror.
			const text = parsed ? parsed.template.replace(/\\([{}])/g, "$1").replace(/\$\{0\}/g, "") : newText;
			// One transaction: all specs are in original-document coordinates; the caret goes after the
			// inserted text, shifted by whatever the extra edits added before it.
			v.dispatch({
				changes: [extra, { from: start, to: end, insert: text }],
				selection: { anchor: s + text.length },
				userEvent: "input.complete",
				scrollIntoView: true,
			});
		},
		info: () => resolveInfo(view, env, item),
	};
	return c;
}

/**
 * Resolve when the item is highlighted (CodeMirror calls `info` on selection): docs for the panel and,
 * crucially, the `additionalTextEdits` (auto-imports) many servers only fill in on resolve. They are
 * stored on the item, so the synchronous `apply` sees them by the time the user accepts.
 */
async function resolveInfo(view: EditorView, env: LspEnv, item: LspCompletionItem) {
	let full = item;
	if (!resolved.has(item) && env.doc.caps?.completionProvider?.resolveProvider) {
		resolved.add(item);
		full =
			(await env.doc.request<LspCompletionItem>("completionItem/resolve", item).catch(() => null)) ?? item;
		item.additionalTextEdits ??= full.additionalTextEdits;
		item.documentation ??= full.documentation;
		item.detail ??= full.detail;
	}
	const html = docHtml(full.documentation ?? item.documentation, full.detail ?? item.detail);
	if (!html) return null;
	const el = document.createElement("div");
	el.className = "cm-lsp-doc md";
	el.innerHTML = html;
	return el;
}

const resolved = new WeakSet<LspCompletionItem>();

async function source(ctx: CompletionContext): Promise<CompletionResult | null> {
	const env = envOf(ctx.view!);
	if (!env?.doc.open) return null;
	const view = ctx.view!;
	const triggers = env.doc.caps?.completionProvider?.triggerCharacters ?? [];
	const before = ctx.state.sliceDoc(Math.max(0, ctx.pos - 1), ctx.pos);
	const word = ctx.matchBefore(/[\w$#@-]*/);
	const byTrigger = triggers.includes(before);
	if (!ctx.explicit && !byTrigger && (!word || word.from === word.to)) return null;

	const id = `c${Date.now()}${Math.random()}`;
	ctx.addEventListener("abort", () => env.doc.cancel(id));
	const res = await env.doc.request<LspCompletionList | LspCompletionItem[]>(
		"textDocument/completion",
		{
			...posParams(view, env, ctx.pos),
			context: byTrigger
				? { triggerKind: 2, triggerCharacter: before }
				: { triggerKind: ctx.explicit ? 1 : 1 },
		},
		id,
	);
	if (ctx.aborted || !res) return null;
	const list: LspCompletionList = Array.isArray(res) ? { items: res } : res;
	if (list.items.length === 0) return null;
	const from = byTrigger ? ctx.pos : (word?.from ?? ctx.pos);
	return {
		from,
		options: list.items.map((i) => toCompletion(view, env, i, list)),
		// Server-side filtering/sorting is authoritative when the list is incomplete.
		filter: !list.isIncomplete,
		update: undefined,
		validFor: list.isIncomplete ? undefined : /^[\w$#@-]*$/,
	};
}

/** Replaces CodeMirror's own sources while the server is serving the document (the caller toggles it). */
export const lspCompletion = autocompletion({
	override: [source],
	activateOnTyping: true,
	icons: true,
});
