/**
 * Symbol outline of an open editor (⌘⇧O), read from the CodeMirror (Lezer) syntax
 * tree: declarations/definitions/items and Markdown headings, with their names.
 * Importing this module installs the CodeMirror-backed `ops` of `symbols.ts`.
 */
import { EditorView } from "codemirror";
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import { redo, selectAll, undo } from "@codemirror/commands";
import { envOf } from "#lib/lsp/context.ts";
import { ops, type OutlineSymbol } from "#lib/symbols.ts";

/** Grammar node names that carry a symbol (JS/TS, Rust, Python, Markdown). */
const DECL = /^(.+?)(Declaration|Definition|Item)$|^ATXHeading(\d)$/;
/** Declarations that are not symbols (`Field`: struct members; `Ambient`: wrapper of the real declaration). */
const SKIP = /^(Import|Export|Use|ExternCrate|Variable|Lexical|Macro|Attribute|Field|Ambient)/;
/** Child nodes holding the declared name. */
const NAME =
	/^(VariableDefinition|PropertyDefinition|TypeDefinition|BoundIdentifier|TypeIdentifier|VariableName|Identifier|PropertyName)$/;

/**
 * Symbols of the whole document. The parse is finished first (up to `budgetMs` of main-thread time): a state's own
 * tree (`syntaxTree(state)`) is the partial one it was created with, so a large file listed only the symbols of its
 * first kilobytes. The work persists in the state's parse context, so a later call is cheap; past the budget the
 * outline covers what is parsed.
 */
export function outline(state: EditorState, budgetMs = 200): OutlineSymbol[] {
	const out: OutlineSymbol[] = [];
	const doc = state.doc;
	(ensureSyntaxTree(state, doc.length, budgetMs) ?? syntaxTree(state)).iterate({
		enter(n) {
			const m = DECL.exec(n.name);
			if (!m || (m[1] && SKIP.test(m[1]))) return;
			let name: string | undefined;
			if (m[3])
				name = doc
					.sliceString(n.from, n.to)
					.replace(/^#+\s*/, "")
					.trim();
			else {
				for (let c = n.node.firstChild; c; c = c.nextSibling)
					if (NAME.test(c.name)) {
						name = doc.sliceString(c.from, c.to);
						break;
					}
			}
			if (name)
				out.push({ name, kind: m[3] ? `H${m[3]}` : m[1], from: n.from, line: doc.lineAt(n.from).number });
		},
	});
	return out;
}

ops.outline = outline;
ops.reveal = (view, pos, to = pos) =>
	view.dispatch({
		selection: { anchor: pos, head: to },
		effects: EditorView.scrollIntoView(pos, { y: "center" }),
	});
ops.viewOf = (host) => EditorView.findFromDOM(host) ?? undefined;
ops.lspEnv = envOf;
ops.edit = (view, action) => ({ undo, redo, selectAll })[action](view);
