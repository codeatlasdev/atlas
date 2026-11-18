/** "Format document": the language server when it can, else the built-in formatter (JSON, Prettier for web formats). */
import { toast } from "@codeatlasdev/ui";
import { indentUnit } from "@codemirror/language";
import type { EditorView } from "@codemirror/view";
import { host, b64bytes } from "#lib/host.ts";
import { formatDocument } from "#lib/lsp/actions.ts";
import { explainUnserved } from "#lib/lsp/hints.svelte.ts";
import { editorFiles } from "#lib/symbols.ts";
import { fileType } from "#lib/viewers/fileType.ts";
import { editorConfigDirs, resolveEditorConfig, type EditorConfigProps } from "#lib/editor/editorconfig.ts";
import { canFormat, formatText } from "#lib/viewers/format.ts";
import { m } from "#lib/i18n/m.ts";

/** `.editorconfig` files from the file's folder up to the worktree root. */
async function editorConfigs(root: string, abs: string): Promise<{ dir: string; text: string }[]> {
	const found = await Promise.all(
		editorConfigDirs(abs, root).map(async (dir) => {
			try {
				const r = await host.fsRead(`${dir}/.editorconfig`, 64 * 1024);
				return { dir, text: new TextDecoder().decode(b64bytes(r.data)) };
			} catch {
				return null;
			}
		}),
	);
	return found.filter((f): f is { dir: string; text: string } => f !== null);
}

/** The indentation string a `.editorconfig` asks for (`\t` or N spaces); `null` when it says nothing. */
function editorConfigIndent(ec: EditorConfigProps): string | null {
	if (ec.indent_style === "tab") return "\t";
	if (ec.indent_style === "space" || typeof ec.indent_size === "number") {
		return " ".repeat(typeof ec.indent_size === "number" ? ec.indent_size : 2);
	}
	return null;
}

/** Server first; the built-in formatter when there is no server or it has no formatter. Returns whether anything formatted. */
export async function formatActive(view: EditorView): Promise<boolean> {
	if (await formatDocument(view)) return true;
	const file = editorFiles.get(view);
	const lang = file ? fileType(file.path).lang : null;
	if (!file || !canFormat(lang)) {
		// With an unserved language session the hint explains why; otherwise nothing can format it.
		if (explainUnserved(view, "format")) toast.info(m.viewers_files_format_action_no_formatter());
		return false;
	}
	const abs = `${file.root}/${file.path}`;
	const ec = resolveEditorConfig(abs, await editorConfigs(file.root, abs));
	const unit = view.state.facet(indentUnit);
	const text = view.state.doc.toString();
	try {
		const out = await formatText(text, lang, {
			indent: editorConfigIndent(ec) ?? (unit.startsWith("\t") ? "\t" : unit),
			finalNewline: ec.insert_final_newline ?? true,
		});
		if (out === null) return false;
		// The buffer may have changed while Prettier loaded: never overwrite newer edits.
		if (view.state.doc.toString() !== text) return false;
		if (out !== text)
			view.dispatch({ changes: { from: 0, to: text.length, insert: out }, userEvent: "input.format" });
		else toast.info(m.viewers_files_format_action_already_formatted());
		return true;
	} catch (e) {
		toast.error(
			m.viewers_files_format_action_cannot_format({
				reason: e instanceof Error ? e.message.split("\n")[0] : String(e),
			}),
		);
		return false;
	}
}
