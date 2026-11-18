/** LSP commands of the focused editor, registered in the app keymap (rebindable, in the palette). */
import type { Command } from "#lib/keymap.svelte.ts";
import { activeEditor } from "#lib/shell/editors.ts";
import { ops } from "#lib/symbols.ts";
import { navigator } from "#lib/lsp/history.ts";
import { m } from "#lib/i18n/m.ts";
import type { EditorView } from "@codemirror/view";

/**
 * The actions pull in the whole editor stack (CodeMirror, language grammars, prettier), which the
 * shell must not load at startup: they are fetched when a command first runs, by which time an
 * editor (and its chunk) exists anyway.
 */
const actions = () => import("#lib/lsp/actions.ts");

/**
 * The focused editor of a file that has a language session (served or not) with the keyboard focus
 * inside it: F2/F12 must not fire from the palette, the rename prompt or a terminal that merely
 * sits in the same pane. An unserved file still gets the command so it can say why nothing happens.
 */
const lspView = () => {
	const v = activeEditor();
	return v && ops.lspEnv(v) && v.dom.contains(document.activeElement) ? v : undefined;
};

/** Any focused file editor, served by a language server or not (Format falls back to the built-in formatter). */
const focusedEditor = () => {
	const v = activeEditor();
	return v && v.dom.contains(document.activeElement) ? v : undefined;
};

const cmd = (id: string, label: string, fn: (v: EditorView) => unknown): Command => ({
	id,
	label,
	run: () => {
		const v = lspView();
		if (v) void fn(v);
	},
	when: () => !!lspView(),
});

/** ⌃- / ⌃⇧-: back and forward over jump positions (does not need a server). */
const navCommands = (): Command[] => [
	{
		id: "editor::GoBack",
		label: m.workbench_lsp_cmd_go_back(),
		run: () => void navigator.go(-1),
		when: () => navigator.canGo(-1),
	},
	{
		id: "editor::GoForward",
		label: m.workbench_lsp_cmd_go_forward(),
		run: () => void navigator.go(1),
		when: () => navigator.canGo(1),
	},
];

export const lspCommands = (): Command[] => [
	...navCommands(),
	cmd("editor::GoToDefinition", m.workbench_lsp_cmd_go_to_definition(), async (v) =>
		(await actions()).goToDefinition(v),
	),
	cmd("editor::GoToTypeDefinition", m.workbench_lsp_cmd_go_to_type_definition(), async (v) =>
		(await actions()).goToTypeDefinition(v),
	),
	cmd("editor::GoToImplementation", m.workbench_lsp_cmd_go_to_implementation(), async (v) =>
		(await actions()).goToImplementation(v),
	),
	cmd("editor::FindAllReferences", m.workbench_lsp_cmd_find_references(), async (v) =>
		(await actions()).findReferences(v),
	),
	cmd("editor::Rename", m.workbench_lsp_cmd_rename(), async (v) => (await actions()).renameSymbol(v)),
	{
		id: "editor::Format",
		label: m.workbench_lsp_cmd_format(),
		run: () => {
			const v = focusedEditor();
			if (v) void import("#lib/viewers/formatAction.ts").then((mod) => mod.formatActive(v));
		},
		when: () => !!focusedEditor(),
	},
	cmd("editor::ToggleCodeActions", m.workbench_lsp_cmd_code_actions(), async (v) =>
		(await import("#lib/lsp/extension.ts")).codeActionMenu(v),
	),
];
