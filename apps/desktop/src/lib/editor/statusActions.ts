/**
 * What clicking a status bar item does: each opens the shared list picker with the choices, or
 * jumps (go to line). The component supplies the editor operations; this module is the menu
 * content, so the wording and the choices live in one place.
 */
import type { PaletteItem } from "#lib/components/Palette.svelte";
import { showPicker } from "#lib/lsp/picker.svelte.ts";
import type { Eol } from "#lib/editor/detect.ts";
import { indentText } from "#lib/editor/statusbar.ts";
import { m } from "#lib/i18n/m.ts";

export interface StatusOps {
	indent: { style: "space" | "tab"; size: number };
	eol: Eol | "mixed";
	setIndent: (style: "space" | "tab", size: number) => void;
	/** Rewrite the file's existing indentation to the given style (undoable). */
	convertIndent: (style: "space" | "tab") => void;
	setEol: (eol: Eol) => void;
	gotoLine: () => void;
	showProblems: () => void;
}

const SIZES = [1, 2, 3, 4, 6, 8];
// Operating system names are proper names (`Windows`), the same in every language.
const EOLS: { id: Eol; label: string; description: string }[] = [
	{ id: "lf", label: "LF", description: "Unix and macOS" },
	{ id: "crlf", label: "CRLF", description: "Windows" },
	{ id: "cr", label: "CR", description: "Classic Mac OS" },
];

const item = (
	group: string,
	label: string,
	run: () => void,
	extra: Partial<PaletteItem> = {},
): PaletteItem => ({
	id: `${group}:${label}`,
	group,
	label,
	run,
	...extra,
});

/** Open the picker (or act) for status item `id`. Unknown ids do nothing. */
export function runStatusAction(id: string, ops: StatusOps) {
	switch (id) {
		case "position":
			return ops.gotoLine();
		case "problems":
			return ops.showProblems();
		case "indent": {
			const cur = ops.indent;
			const items: PaletteItem[] = [
				...(["space", "tab"] as const).flatMap((style) =>
					SIZES.map((size) =>
						item(
							style === "space" ? m.workbench_editor_indent_using_spaces() : m.workbench_editor_indent_using_tabs(),
							indentText({ style, size }),
							() => ops.setIndent(style, size),
							{ description: cur.style === style && cur.size === size ? m.workbench_editor_current() : undefined },
						),
					),
				),
				item(m.workbench_editor_convert_file(), m.workbench_editor_convert_to_spaces(), () => ops.convertIndent("space")),
				item(m.workbench_editor_convert_file(), m.workbench_editor_convert_to_tabs(), () => ops.convertIndent("tab")),
			];
			return showPicker(m.workbench_editor_select_indentation(), items);
		}
		case "eol":
			return showPicker(
				m.workbench_editor_select_eol(),
				EOLS.map((e) =>
					item(m.workbench_editor_line_ending(), e.label, () => ops.setEol(e.id), {
						description: ops.eol === e.id ? `${e.description} · ${m.workbench_editor_current()}` : e.description,
					}),
				),
			);
		default:
			return undefined;
	}
}
