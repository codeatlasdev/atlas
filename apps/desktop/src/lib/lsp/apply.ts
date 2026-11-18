/**
 * Apply an LSP WorkspaceEdit: open editors take it as a CodeMirror transaction (undoable, and the
 * normal change flow tells the server), every other file is read, edited and written through hostd.
 * File operations (create/rename/delete) go through the fs commands.
 */
import type { EditorView } from "@codemirror/view";
import { b64decode, host } from "#lib/host.ts";
import { planWorkspaceEdit, type WorkspaceEdit } from "#lib/lsp/convert.ts";

/** Open editor by absolute path; registry keys are `worktreeId:relPath`, so the workspace maps them. */
type Resolve = (absPath: string) => EditorView | undefined;

let resolver: Resolve = () => undefined;

/** The workspace tells us how to find the editor of an absolute path (it knows the worktree roots). */
export const setEditorResolver = (r: Resolve) => (resolver = r);

export interface ApplyResult {
	applied: boolean;
	files: number;
	error?: string;
}

export async function applyWorkspaceEdit(edit: WorkspaceEdit): Promise<ApplyResult> {
	const { files, ops } = planWorkspaceEdit(edit);
	let n = 0;
	try {
		// CodeMirror's half loads here, not at startup (a rename is the rare path; the editor's chunk is usually warm).
		const { applyTextEdits, editsToChanges } = await import("#lib/lsp/changes.ts");
		for (const op of ops) {
			if (op.kind === "rename") await host.fsRename(op.from!, op.to!);
			else if (op.kind === "delete") await host.fsRemove(op.path!, true);
			else await host.fsWrite(op.path!, "");
		}
		for (const [path, edits] of files) {
			const view = resolver(path);
			if (view) {
				view.dispatch({
					changes: editsToChanges(view.state.doc, edits),
					userEvent: "input.lsp",
					scrollIntoView: false,
				});
			} else {
				const r = await host.fsRead(path, 16 * 1024 * 1024);
				await host.fsWrite(path, applyTextEdits(b64decode(r.data), edits));
			}
			n++;
		}
	} catch (e) {
		return { applied: false, files: n, error: e instanceof Error ? e.message : String(e) };
	}
	return { applied: true, files: n };
}
