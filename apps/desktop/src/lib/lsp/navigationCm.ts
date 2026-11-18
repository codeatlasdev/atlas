/**
 * The CodeMirror-bound half of go-to-definition (`navigation.ts` is the pure half, which the shell reaches at
 * startup): a selection from LSP positions and the read-only extension of dependency sources. Loaded with the editor.
 */
import { EditorSelection, EditorState, type Text } from "@codemirror/state";
import { posToOffset, type Position } from "#lib/lsp/convert.ts";

/** Selection covering `start..end`, clamped to the document (a server may point past a line's end). */
export function selectionOf(doc: Text, start: Position, end: Position) {
	return EditorSelection.single(posToOffset(doc, start), posToOffset(doc, end)).main;
}

/** Dependency and external sources: browsable (caret, selection, find, folding) but nothing can change the text. */
export function readOnlyExtension() {
	return [EditorState.readOnly.of(true), EditorState.transactionFilter.of((tr) => (tr.docChanged ? [] : tr))];
}
