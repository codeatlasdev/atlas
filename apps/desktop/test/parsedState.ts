// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * An `EditorState` whose syntax tree covers the whole document, for tests that read `syntaxTree(state)`.
 *
 * `EditorState.create` parses within a small time budget, so on a loaded machine even a one-line document can start
 * with a partial tree (CI saw `expandSelection` and the outline find nothing past the first node). `ensureSyntaxTree`
 * finishes the parse but returns the tree without updating the state's own field; an empty transaction makes the
 * state pick it up.
 */
import { EditorState, type EditorStateConfig } from "@codemirror/state";
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";

export function parsedState(config: EditorStateConfig): EditorState {
	const created = EditorState.create(config);
	if (!ensureSyntaxTree(created, created.doc.length, 10_000))
		throw new Error("the parse did not finish in 10 s");
	const state = created.update({}).state;
	if (syntaxTree(state).length < state.doc.length)
		throw new Error("the state did not pick up the finished tree");
	return state;
}
