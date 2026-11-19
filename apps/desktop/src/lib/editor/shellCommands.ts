/**
 * Editor commands of the focused file editor for the app command palette (rebindable in the
 * keymap, listed with their binding). CodeMirror and its commands are fetched when one first
 * runs: the shell must not load the editor stack at startup (same rule as `lsp/commands.ts`).
 */
import type { EditorView } from "@codemirror/view";
import { named, type Command } from "#lib/keymap.svelte.ts";
import { activeEditor } from "#lib/shell/editors.ts";
import { sessionOf } from "#lib/editor/registry.ts";
import { m } from "#lib/i18n/m.ts";

const focused = () => {
	const v = activeEditor();
	return v && v.dom.contains(document.activeElement) ? v : undefined;
};

/** Palette entries only: ids and names, the work lives in `editor/keymap.ts` (lazy). A name is a message read when the palette opens. */
const COMMANDS: { id: string; label: () => string }[] = [
	{ id: "editor::TrimTrailingWhitespace", label: () => m.workbench_editor_cmd_trim_whitespace() },
	{ id: "editor::ConvertIndentationToSpaces", label: () => m.workbench_editor_convert_to_spaces() },
	{ id: "editor::ConvertIndentationToTabs", label: () => m.workbench_editor_convert_to_tabs() },
	{ id: "editor::FoldAllCommand", label: () => m.workbench_editor_cmd_fold_all() },
	{ id: "editor::UnfoldAllCommand", label: () => m.workbench_editor_cmd_unfold_all() },
	{ id: "editor::SortLinesAscending", label: () => m.workbench_editor_cmd_sort_ascending() },
	{ id: "editor::SortLinesDescending", label: () => m.workbench_editor_cmd_sort_descending() },
	{ id: "editor::DuplicateLine", label: () => m.workbench_editor_cmd_duplicate_line() },
	{ id: "editor::ReindentSelectionCommand", label: () => m.workbench_editor_cmd_reindent() },
	{ id: "editor::TransformToUppercase", label: () => m.workbench_editor_cmd_uppercase() },
	{ id: "editor::TransformToLowercase", label: () => m.workbench_editor_cmd_lowercase() },
	{ id: "editor::JoinLinesCommand", label: () => m.workbench_editor_cmd_join_lines() },
];

const run = (id: string, v: EditorView) =>
	void import("#lib/editor/keymap.ts").then((mod) => mod.editorCommands.find((c) => c.id === id)?.run(v));

const session = () => {
	const v = focused();
	return v ? sessionOf(v) : undefined;
};

/** Commands are registered once at startup: `named` makes `label` a getter, so the palette reads the app's language each time it shows one. */
export const editorShellCommands = (): Command[] =>
	named([
		...COMMANDS.map(({ id, label }) => ({
			id,
			label,
			run: () => {
				const v = focused();
				if (v) run(id, v);
			},
			when: () => !!focused(),
		})),
		{
			id: "editor::ToggleSoftWrap",
			label: () => m.workbench_editor_cmd_toggle_wrap(),
			run: () => {
				const s = session();
				if (s) s.setWrap(!s.wrap);
			},
			when: () => !!session(),
		},
		{
			id: "editor::RevertFile",
			label: () => m.workbench_editor_cmd_revert_file(),
			run: () => void session()?.revert(),
			when: () => !!session()?.dirty,
		},
	]);
