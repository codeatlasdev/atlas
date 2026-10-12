/**
 * Symbol outline of an open editor (⌘⇧O), read from the CodeMirror (Lezer) syntax
 * tree: declarations/definitions/items and Markdown headings, with their names.
 */
import { EditorView } from "codemirror";
import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

/** Live editors by `worktreeId:path`, registered by Editor.svelte. */
export const editors = new Map<string, EditorView>();

/** Each live editor's save (writes the document if it changed), by the same key. */
export const savers = new Map<string, () => Promise<void>>();

/**
 * ⌘S: save the editor `key` (the focused pane's) now. `false` when there is none, so the
 * caller can say "Nothing to save" instead of silently doing nothing.
 */
export async function saveEditor(key: string | undefined): Promise<boolean> {
	const save = key ? savers.get(key) : undefined;
	if (!save) return false;
	await save();
	return true;
}

/** ⌥⌘S: save every open file editor (the atlas.yaml review dialog is only for ⌘S). Returns how many. */
export async function saveAllEditors(): Promise<number> {
	const all = [...savers].filter(([key]) => editors.has(key)).map(([, save]) => save);
	await Promise.all(all.map((s) => s()));
	return all.length;
}

/** Line (and column) requested for an editor that is not mounted yet; consumed on registration. */
const pendingGoto = new Map<string, { line: number; col?: number }>();

/** Put the caret at `line[:col]` (1-based) and scroll it to the middle; clamps to the document. */
export function gotoLine(view: EditorView, line: number, col?: number) {
	const doc = view.state.doc;
	const ln = doc.line(Math.min(Math.max(1, line), doc.lines));
	const pos = ln.from + Math.min(Math.max(0, (col ?? 1) - 1), ln.length);
	view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
	view.focus();
}

/** Jump to a line of an editor: now if it is open, otherwise as soon as it registers. */
export function requestGoto(key: string, line: number, col?: number) {
	const view = editors.get(key);
	if (view) gotoLine(view, line, col);
	else pendingGoto.set(key, { line, col });
}

/** Called by Editor.svelte right after it registers `view`. */
export function registerEditor(key: string, view: EditorView, save?: () => Promise<void>) {
	editors.set(key, view);
	if (save) savers.set(key, save);
	const want = pendingGoto.get(key);
	if (want) {
		pendingGoto.delete(key);
		gotoLine(view, want.line, want.col);
	}
}

/** Editor.svelte on destroy: forget `key` if it is still this `view`. */
export function unregisterEditor(key: string, view: EditorView) {
	if (editors.get(key) !== view) return;
	editors.delete(key);
	savers.delete(key);
}

export interface OutlineSymbol {
	name: string;
	/** Node kind without the grammar suffix (`Function`, `Class`, `Struct`, `H2`…). */
	kind: string;
	from: number;
	line: number;
}

/** Grammar node names that carry a symbol (JS/TS, Rust, Python, Markdown). */
const DECL = /^(.+?)(Declaration|Definition|Item)$|^ATXHeading(\d)$/;
/** Declarations that are not symbols (`Field`: struct members; `Ambient`: wrapper of the real declaration). */
const SKIP = /^(Import|Export|Use|ExternCrate|Variable|Lexical|Macro|Attribute|Field|Ambient)/;
/** Child nodes holding the declared name. */
const NAME =
	/^(VariableDefinition|PropertyDefinition|TypeDefinition|BoundIdentifier|TypeIdentifier|VariableName|Identifier|PropertyName)$/;

export function outline(state: EditorState): OutlineSymbol[] {
	const out: OutlineSymbol[] = [];
	const doc = state.doc;
	syntaxTree(state).iterate({
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
