/**
 * Edit menu commands (⌘Z ⇧⌘Z ⌘A). The native menu owns these keys and forwards them as
 * `menu://command`: the predefined AppKit items send `undo:` / `redo:` / `selectAll:` to the
 * WKWebView, which only knows its own undo stack. CodeMirror keeps a private history (so
 * ⌘Z did nothing) and the GPU terminal has no text to select in the page. So the page
 * decides: focused CodeMirror → its history/selection commands, GPU terminal → its own
 * selection, a plain field → `execCommand`. Anything else is left alone.
 */
import { redo, selectAll, undo } from "@codemirror/commands";
import type { EditorView } from "codemirror";

export type EditAction = "undo" | "redo" | "selectAll";

/** Command id → action, for the menu items in `menu.rs`. */
export const EDIT_COMMANDS: Record<string, EditAction> = {
	"edit::Undo": "undo",
	"edit::Redo": "redo",
	"edit::SelectAll": "selectAll",
};

/** Where the focus is, as far as editing is concerned. */
export type EditTarget =
	| { kind: "codemirror"; view: EditorView }
	| { kind: "terminal"; tab: string }
	| { kind: "field" }
	| { kind: "none" };

/** What can be done with the focused element; injected so the routing is testable without a DOM. */
export interface EditEnv {
	/** CodeMirror view owning `el`, if it is inside an editor. */
	viewOf: (el: Element) => EditorView | undefined;
	/** GPU terminal tab whose input sink is `el`. */
	terminalOf: (el: Element) => string | undefined;
	/** `el` takes text (input, textarea, contenteditable). */
	isField: (el: Element) => boolean;
}

export function targetOf(el: Element | null, env: EditEnv): EditTarget {
	if (!el) return { kind: "none" };
	const view = env.viewOf(el);
	if (view) return { kind: "codemirror", view };
	const tab = env.terminalOf(el);
	if (tab) return { kind: "terminal", tab };
	return env.isField(el) ? { kind: "field" } : { kind: "none" };
}

const CM = { undo, redo, selectAll };
/** `execCommand` names of the same actions, for native fields. */
const NATIVE: Record<EditAction, string> = { undo: "undo", redo: "redo", selectAll: "selectAll" };

export interface EditRun {
	/** `execCommand` for native fields. */
	exec: (name: string) => void;
	/** Select all in a GPU terminal. */
	terminalSelectAll: (tab: string) => void;
}

/**
 * Run `action` on `target`. Returns whether something handled it. A terminal has no
 * history, so undo/redo there do nothing (and never reach the shell).
 */
export function runEdit(action: EditAction, target: EditTarget, run: EditRun): boolean {
	switch (target.kind) {
		case "codemirror":
			return CM[action](target.view);
		case "terminal":
			if (action !== "selectAll") return false;
			run.terminalSelectAll(target.tab);
			return true;
		case "field":
			run.exec(NATIVE[action]);
			return true;
		case "none":
			return false;
	}
}
