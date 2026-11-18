/** Keyboard-driven LSP actions on an editor view: definition, references, rename, format, code actions. */
import { toast } from "@codeatlasdev/ui";
import { indentUnit } from "@codemirror/language";
import type { EditorView } from "@codemirror/view";
import { promptDialog } from "#lib/stores/ask.svelte.ts";
import { m } from "#lib/i18n/m.ts";
import { envOf, posParams, type Location } from "#lib/lsp/context.ts";
import { applyWorkspaceEdit } from "#lib/lsp/apply.ts";
import {
	editsToChanges,
	offsetToPos,
	rangeToOffsets,
	uriToPath,
	type Range,
	type TextEdit,
	type WorkspaceEdit,
} from "#lib/lsp/convert.ts";
import type { LspDiagnostic } from "#lib/lsp/client.svelte.ts";
import { explainUnserved } from "#lib/lsp/hints.svelte.ts";
import type { Feature } from "#lib/lsp/messages.ts";
import { toTargets, type Target } from "#lib/lsp/navigation.ts";

const toLocation = (t: Target): Location => ({
	path: t.path,
	line: t.start.line,
	character: t.start.character,
	end: t.end,
});

async function locate(view: EditorView, method: string, feature: Feature, none: string, title: string) {
	const env = envOf(view);
	if (!env || !explainUnserved(view, feature)) return;
	const res = await env.doc.request<unknown>(method, posParams(view, env));
	const list = toTargets(res).map(toLocation);
	if (list.length === 0) return toast.info(none);
	if (list.length === 1)
		env.openAt(list[0].path, { line: list[0].line, character: list[0].character }, list[0].end);
	else env.showLocations(title, list);
}

export const goToDefinition = (view: EditorView) =>
	locate(
		view,
		"textDocument/definition",
		"definition",
		m.workbench_lsp_none_definition(),
		m.workbench_lsp_title_definitions(),
	);
export const goToTypeDefinition = (view: EditorView) =>
	locate(
		view,
		"textDocument/typeDefinition",
		"type-definition",
		m.workbench_lsp_none_type_definition(),
		m.workbench_lsp_title_type_definitions(),
	);
export const goToImplementation = (view: EditorView) =>
	locate(
		view,
		"textDocument/implementation",
		"implementation",
		m.workbench_lsp_none_implementation(),
		m.workbench_lsp_title_implementations(),
	);

export async function findReferences(view: EditorView) {
	const env = envOf(view);
	if (!env || !explainUnserved(view, "references")) return;
	const res = await env.doc.request<unknown>("textDocument/references", {
		...posParams(view, env),
		context: { includeDeclaration: true },
	});
	const doc = view.state.doc;
	const list = toTargets(res).map((t) => {
		const loc = toLocation(t);
		// Same-file hits get their source line; other files would need a read per result.
		if (loc.path === env.doc.path && loc.line < doc.lines) loc.preview = doc.line(loc.line + 1).text.trim();
		return loc;
	});
	if (list.length === 0) return toast.info(m.workbench_lsp_none_references());
	env.showLocations(m.workbench_lsp_title_references({ count: list.length }), list);
}

export async function renameSymbol(view: EditorView) {
	const env = envOf(view);
	if (!env || !explainUnserved(view, "rename")) return;
	const p = posParams(view, env);
	const rp = env.doc.caps?.renameProvider;
	let current = "";
	if (typeof rp === "object" && rp?.prepareProvider) {
		const prep = await env.doc.request<Range | { range: Range; placeholder?: string } | null>(
			"textDocument/prepareRename",
			p,
		);
		if (!prep) return toast.info(m.workbench_lsp_rename_nothing());
		const range = "range" in prep ? prep.range : prep;
		current = "placeholder" in prep && prep.placeholder ? prep.placeholder : "";
		if (!current) {
			const [f, t] = rangeToOffsets(view.state.doc, range);
			current = view.state.sliceDoc(f, t);
		}
	} else {
		const w = view.state.wordAt(view.state.selection.main.head);
		current = w ? view.state.sliceDoc(w.from, w.to) : "";
	}
	const name = await promptDialog(m.workbench_lsp_cmd_rename(), {
		value: current,
		confirmLabel: m.common_action_rename(),
	});
	if (!name || name === current) return;
	const edit = await env.doc.request<WorkspaceEdit | null>("textDocument/rename", { ...p, newName: name });
	if (!edit) return toast.info(m.workbench_lsp_rename_no_changes());
	const r = await applyWorkspaceEdit(edit);
	if (r.applied) toast.success(m.workbench_lsp_renamed({ count: r.files }));
	else toast.error(m.workbench_lsp_rename_failed({ error: r.error ?? "" }));
}

/**
 * Format the document (or the selection when there is one) with the server's formatter. Returns
 * false when no server can: the caller falls back to the built-in formatter (see `formatAction.ts`).
 */
export async function formatDocument(view: EditorView): Promise<boolean> {
	const env = envOf(view);
	if (!env?.doc.open || !env.doc.caps?.documentFormattingProvider) return false;
	const sel = view.state.selection.main;
	const options = {
		tabSize: view.state.tabSize,
		insertSpaces: !view.state.facet(indentUnit).startsWith("\t"),
	};
	const textDocument = posParams(view, env, 0).textDocument;
	const edits = sel.empty
		? await env.doc.request<TextEdit[] | null>("textDocument/formatting", { textDocument, options })
		: await env.doc.request<TextEdit[] | null>("textDocument/rangeFormatting", {
				textDocument,
				options,
				range: { start: offsetToPos(view.state.doc, sel.from), end: offsetToPos(view.state.doc, sel.to) },
			});
	if (!edits?.length) return true;
	view.dispatch({ changes: editsToChanges(view.state.doc, edits), userEvent: "input.format" });
	return true;
}

// --- Code actions -----------------------------------------------------------------------------

export interface CodeAction {
	title: string;
	kind?: string;
	isPreferred?: boolean;
	disabled?: { reason: string };
	edit?: WorkspaceEdit;
	command?: { command: string; title: string; arguments?: unknown[] };
	data?: unknown;
	diagnostics?: LspDiagnostic[];
}

/** Code actions for a range; `only` filters kinds (`quickfix`, `refactor`, `source`). */
export async function codeActionsAt(
	view: EditorView,
	from: number,
	to: number,
	diagnostics: LspDiagnostic[] = [],
	only?: string[],
): Promise<CodeAction[]> {
	const env = envOf(view);
	if (!env?.doc.open || !env.doc.caps?.codeActionProvider) return [];
	const doc = view.state.doc;
	const res = await env.doc.request<(CodeAction | { command: string; title: string })[]>(
		"textDocument/codeAction",
		{
			textDocument: posParams(view, env, 0).textDocument,
			range: { start: offsetToPos(doc, from), end: offsetToPos(doc, to) },
			context: { diagnostics, only, triggerKind: 1 },
		},
	);
	return (res ?? [])
		.map(
			(a) =>
				("command" in a && typeof a.command === "string" ? { title: a.title, command: a } : a) as CodeAction,
		)
		.filter((a) => !a.disabled);
}

export async function runCodeAction(view: EditorView, action: CodeAction) {
	const env = envOf(view);
	if (!env) return;
	let a = action;
	if (!a.edit && !a.command && env.doc.caps?.codeActionProvider) {
		a = (await env.doc.request<CodeAction>("codeAction/resolve", action)) ?? action;
	}
	if (a.edit) {
		const r = await applyWorkspaceEdit(a.edit);
		if (!r.applied)
			return void toast.error(m.workbench_lsp_action_failed({ title: a.title, error: r.error ?? "" }));
	}
	if (a.command) await env.doc.request("workspace/executeCommand", a.command).catch(() => {});
}
