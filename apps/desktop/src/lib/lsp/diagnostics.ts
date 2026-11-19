import {
	forEachDiagnostic,
	lintGutter,
	setDiagnostics,
	type Action,
	type Diagnostic,
} from "@codemirror/lint";
import type { EditorView } from "@codemirror/view";
import { rangeToOffsets } from "#lib/lsp/convert.ts";
import type { LspDiagnostic } from "#lib/lsp/client.svelte.ts";
import { codeActionsAt, runCodeAction, type CodeAction } from "#lib/lsp/actions.ts";
import { m } from "#lib/i18n/m.ts";

const SEVERITY: Record<number, Diagnostic["severity"]> = { 1: "error", 2: "warning", 3: "info", 4: "hint" };

/** Quick fixes of one diagnostic, fetched when the tooltip is built (`actions` are sync in CM, so load lazily). */
function quickFixActions(view: EditorView, d: LspDiagnostic, from: number, to: number): Action[] {
	return [
		{
			name: m.workbench_lsp_quick_fix(),
			apply: () => void openQuickFixes(view, d, from, to),
		},
	];
}

async function openQuickFixes(view: EditorView, d: LspDiagnostic, from: number, to: number) {
	const actions = await codeActionsAt(view, from, to, [d], ["quickfix"]);
	if (actions.length === 0) return;
	const first = actions.find((a: CodeAction) => a.isPreferred) ?? actions[0];
	await runCodeAction(view, first);
}

/** Map server diagnostics to CodeMirror's lint state (positions clamp to the current document). */
export function applyDiagnostics(
	view: EditorView,
	diags: LspDiagnostic[],
	version: number | null | undefined,
	current: number,
) {
	// A push for an older document version describes text we no longer have: ranges would be off.
	if (typeof version === "number" && version < current) return;
	const doc = view.state.doc;
	const out: Diagnostic[] = diags.map((d) => {
		const [from, to] = rangeToOffsets(doc, d.range);
		return {
			from,
			to: Math.max(from, to),
			severity: SEVERITY[d.severity ?? 1] ?? "error",
			message: d.message,
			source: [d.source, d.code].filter((x) => x !== undefined && x !== "").join(" "),
			actions: quickFixActions(view, d, from, Math.max(from, to)),
		};
	});
	view.dispatch(setDiagnostics(view.state, out));
}

/** Count of diagnostics currently shown, by severity (for the per-pane problems badge). */
export function countDiagnostics(view: EditorView) {
	const c = { error: 0, warning: 0, info: 0, hint: 0 };
	forEachDiagnostic(view.state, (d) => void c[d.severity]++);
	return c;
}

export const lspGutter = lintGutter();
