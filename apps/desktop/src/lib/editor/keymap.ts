/**
 * VS Code / Cursor keymap of the editor (macOS: Mod = ⌘) as one table: every binding has a unique
 * `id`, so a test can assert that no two share a key and that ids are unique. `editorKeymap` is the
 * `keymap.of(...)` extension; `editorCommands` are the palette-only commands for the integrator.
 *
 * App-level shadowing: `lib/shell/windowKeys.ts` dispatches the app keymap in the *capture* phase,
 * so a chord the shell binds never reaches CodeMirror. `ownedKeys.ts` (`EDITOR_OWNED`) lists the
 * ones a focused editor keeps (⌘[ ⌘] ⌘/ ⌘L ⌘⇧\); `APP_SHADOWED` below is the other side of that
 * contract: the editor bindings the shell still wins, which a test keeps in sync.
 */
import {
	addCursorAbove,
	addCursorBelow,
	copyLineDown,
	copyLineUp,
	cursorMatchingBracket,
	deleteLine,
	indentLess,
	indentSelection,
	insertBlankLine,
	moveLineDown,
	moveLineUp,
	selectLine,
	toggleBlockComment,
	toggleComment,
	transposeChars,
} from "@codemirror/commands";
import { foldAll, foldCode, unfoldAll, unfoldCode } from "@codemirror/language";
import {
	findNext,
	findPrevious,
	gotoLine,
	openSearchPanel,
	selectNextOccurrence,
	selectSelectionMatches,
} from "@codemirror/search";
import type { Command } from "@codemirror/view";
import { EditorView, keymap, type KeyBinding } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import {
	convertIndentation,
	expandSelection,
	indentOrInsert,
	joinLines,
	lowercase,
	shrinkSelection,
	sortLines,
	trimWhitespace,
	uppercase,
} from "#lib/editor/commands.ts";

export interface EditorBinding {
	/** Stable id (`editor::…`, Zed style) for docs, tests and a future rebind UI. */
	id: string;
	/** CodeMirror key spelling; a space separates the strokes of a chord (`Mod-k Mod-0`). */
	key: string;
	run: Command;
	/** Also runs with a non-empty selection / shift variants handled by the command itself. */
	preventDefault?: boolean;
}

/** Insert an empty line below / above the caret line and move there (VS Code ⌘⏎ / ⌘⇧⏎). */
const insertLineBelow: Command = (view) => {
	const { state } = view;
	const line = state.doc.lineAt(state.selection.main.head);
	view.dispatch(
		state.update({
			changes: { from: line.to, insert: "\n" },
			selection: { anchor: line.to + 1 },
			userEvent: "input",
			scrollIntoView: true,
		}),
	);
	return true;
};
const insertLineAbove: Command = (view) => {
	const { state } = view;
	const line = state.doc.lineAt(state.selection.main.head);
	view.dispatch(
		state.update({
			changes: { from: line.from, insert: "\n" },
			selection: { anchor: line.from },
			userEvent: "input",
			scrollIntoView: true,
		}),
	);
	return true;
};

/** Replace panel: open the search panel (the themed one shows the replace row once it has text; ⌘⌥F focuses it). */
const openReplace: Command = (view) => {
	openSearchPanel(view);
	requestAnimationFrame(() => {
		const toggle = view.dom.querySelector<HTMLButtonElement>(".cm-find-chevron");
		if (toggle && toggle.getAttribute("aria-expanded") !== "true") toggle.click();
	});
	return true;
};

export const BINDINGS: EditorBinding[] = [
	// Multi-cursor
	{ id: "editor::SelectNextOccurrence", key: "Mod-d", run: selectNextOccurrence, preventDefault: true },
	{ id: "editor::SelectAllOccurrences", key: "Mod-Shift-l", run: selectSelectionMatches },
	{ id: "editor::AddCursorAbove", key: "Alt-Mod-ArrowUp", run: addCursorAbove },
	{ id: "editor::AddCursorBelow", key: "Alt-Mod-ArrowDown", run: addCursorBelow },
	// Lines
	{ id: "editor::MoveLineUp", key: "Alt-ArrowUp", run: moveLineUp },
	{ id: "editor::MoveLineDown", key: "Alt-ArrowDown", run: moveLineDown },
	{ id: "editor::CopyLineUp", key: "Alt-Shift-ArrowUp", run: copyLineUp },
	{ id: "editor::CopyLineDown", key: "Alt-Shift-ArrowDown", run: copyLineDown },
	{ id: "editor::DeleteLine", key: "Mod-Shift-k", run: deleteLine },
	{ id: "editor::SelectLine", key: "Mod-l", run: selectLine, preventDefault: true },
	{ id: "editor::InsertLineBelow", key: "Mod-Enter", run: insertLineBelow },
	{ id: "editor::InsertLineAbove", key: "Mod-Shift-Enter", run: insertLineAbove },
	{ id: "editor::JoinLines", key: "Ctrl-j", run: joinLines },
	{ id: "editor::InsertBlankLine", key: "Ctrl-Enter", run: insertBlankLine },
	// Comments
	{ id: "editor::ToggleComment", key: "Mod-/", run: toggleComment },
	{ id: "editor::ToggleBlockComment", key: "Alt-Shift-a", run: toggleBlockComment },
	// Indentation
	{ id: "editor::Indent", key: "Tab", run: indentOrInsert },
	{ id: "editor::Outdent", key: "Shift-Tab", run: indentLess },
	{ id: "editor::IndentMore", key: "Mod-]", run: indentOrInsert, preventDefault: true },
	{ id: "editor::IndentLess", key: "Mod-[", run: indentLess, preventDefault: true },
	{ id: "editor::ReindentSelection", key: "Mod-Alt-\\", run: indentSelection },
	// Folding
	{ id: "editor::Fold", key: "Alt-Mod-[", run: foldCode },
	{ id: "editor::Unfold", key: "Alt-Mod-]", run: unfoldCode },
	// Navigation
	{ id: "editor::GoToLine", key: "Ctrl-g", run: gotoLine, preventDefault: true },
	{ id: "editor::MatchingBracket", key: "Mod-Shift-\\", run: cursorMatchingBracket },
	{ id: "editor::ExpandSelection", key: "Alt-Shift-ArrowRight", run: expandSelection },
	{ id: "editor::ShrinkSelection", key: "Alt-Shift-ArrowLeft", run: shrinkSelection },
	{ id: "editor::Transpose", key: "Ctrl-t", run: transposeChars },
	// Find and replace
	{ id: "editor::Find", key: "Mod-f", run: openSearchPanel, preventDefault: true },
	{ id: "editor::Replace", key: "Mod-Alt-f", run: openReplace, preventDefault: true },
	{ id: "editor::ReplaceH", key: "Mod-h", run: openReplace, preventDefault: true },
	{ id: "editor::FindNext", key: "Mod-g", run: findNext, preventDefault: true },
	{ id: "editor::FindPrevious", key: "Mod-Shift-g", run: findPrevious, preventDefault: true },
];

/** Key spelling normalised for collision checks (`Alt-Mod-x` and `Mod-Alt-x` are the same chord). */
export function normalizeKey(key: string): string {
	return key
		.split(" ")
		.map((stroke) => {
			// A trailing `-` is the minus key (`Mod--`): split on `-` but keep that last one.
			const minus = stroke.endsWith("--");
			const k = minus ? "-" : stroke.slice(stroke.lastIndexOf("-") + 1);
			const head = minus ? stroke.slice(0, -2) : stroke.slice(0, Math.max(0, stroke.lastIndexOf("-")));
			const mods = head ? head.split("-").sort() : [];
			return [...mods, k.length === 1 ? k.toLowerCase() : k].join("-");
		})
		.join(" ");
}

/** Ids and normalised keys that occur more than once in `bindings` (both empty when the table is sound). */
export function collisions(bindings: readonly EditorBinding[]): { ids: string[]; keys: string[] } {
	const seenId = new Set<string>();
	const seenKey = new Set<string>();
	const ids: string[] = [];
	const keys: string[] = [];
	for (const b of bindings) {
		if (seenId.has(b.id)) ids.push(b.id);
		seenId.add(b.id);
		const k = normalizeKey(b.key);
		if (seenKey.has(k)) keys.push(k);
		seenKey.add(k);
	}
	return { ids, keys };
}

const toKeyBinding = (b: EditorBinding): KeyBinding => ({
	key: b.key,
	run: b.run,
	preventDefault: b.preventDefault ?? true,
});

/**
 * The editor keymap plus ⌥-click multi-cursor (`clickAddsSelectionRange`). Put it before any other
 * editor keymap and `Prec.high` if the language server adds conflicting keys.
 */
export const editorKeymap: Extension = [
	keymap.of(BINDINGS.map(toKeyBinding)),
	EditorView.clickAddsSelectionRange.of((e) => e.altKey && !e.shiftKey),
];

/** A palette-only command: what it does. Its name is in `shellCommands.ts`, in the app's language. */
export interface EditorCommand {
	id: string;
	run: (view: EditorView) => boolean;
}

/**
 * Palette commands (no default key). Toggles that need app state — word wrap, whitespace, EOL,
 * format selection — are not here: the integrator owns those (`Editor.svelte` state), see report.
 */
export const editorCommands: EditorCommand[] = [
	{ id: "editor::TrimTrailingWhitespace", run: (v) => trimWhitespace(v) },
	{ id: "editor::ConvertIndentationToSpaces", run: (v) => convertIndentation("space")(v) },
	{ id: "editor::ConvertIndentationToTabs", run: (v) => convertIndentation("tab")(v) },
	{ id: "editor::FoldAllCommand", run: foldAll },
	{ id: "editor::UnfoldAllCommand", run: unfoldAll },
	{ id: "editor::SortLinesAscending", run: (v) => sortLines(false)(v) },
	{ id: "editor::SortLinesDescending", run: (v) => sortLines(true)(v) },
	{ id: "editor::DuplicateLine", run: copyLineDown },
	{ id: "editor::ReindentSelectionCommand", run: (v) => indentSelection(v) },
	{ id: "editor::TransformToUppercase", run: (v) => uppercase(v) },
	{ id: "editor::TransformToLowercase", run: (v) => lowercase(v) },
	{ id: "editor::JoinLinesCommand", run: (v) => joinLines(v) },
];
