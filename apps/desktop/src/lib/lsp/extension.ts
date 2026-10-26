import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { toast } from "@codeatlasdev/ui";
import type { PaletteItem } from "#lib/components/Palette.svelte";
import { lspCompletion } from "#lib/lsp/completion.ts";
import { envOf, lspEnv, type LspEnv } from "#lib/lsp/context.ts";
import { lspGutter } from "#lib/lsp/diagnostics.ts";
import { lspFeatureTheme, lspHighlight, lspHover, lspInlayHints, lspSignature } from "#lib/lsp/features.ts";
import { codeActionsAt, formatDocument, goToDefinition, runCodeAction } from "#lib/lsp/actions.ts";
import { explainUnserved } from "#lib/lsp/hints.svelte.ts";
import { showPicker } from "#lib/lsp/picker.svelte.ts";
import { isGoToClick, isMac } from "#lib/lsp/navigation.ts";
import { linkHover, setLink } from "#lib/lsp/linkHover.ts";

export async function codeActionMenu(view: EditorView) {
	const env = envOf(view);
	if (!env || !explainUnserved(view, "code-actions")) return;
	const sel = view.state.selection.main;
	const actions = await codeActionsAt(view, sel.from, sel.to);
	if (actions.length === 0) return toast.info("No code actions here");
	const items: PaletteItem[] = actions.map((a, i) => ({
		id: `${i}`,
		group: a.kind?.startsWith("quickfix") ? "Quick fixes" : "Code actions",
		label: a.title,
		description: a.kind,
		run: () => void runCodeAction(view, a).then(() => view.focus()),
	}));
	showPicker("Code actions", items);
}

/** The click: a go-to modifier + primary button moves the caret there first, then jumps. */
const cmdClick = EditorView.domEventHandlers({
	mousedown(e, view) {
		if (!isGoToClick(e, isMac()) || !envOf(view)) return false;
		const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
		if (pos === null) return false;
		e.preventDefault();
		view.dispatch({ selection: { anchor: pos }, effects: setLink.of(null) });
		void goToDefinition(view);
		return true;
	},
});

export interface LspExtensionOptions {
	env: LspEnv;
}

export { lspCompletion };

/** Every LSP editor feature for one view, driven by `env` (document sync + host effects). */
export function lspExtension({ env }: LspExtensionOptions): Extension {
	return [
		lspEnv.of(env),
		lspHover,
		lspSignature,
		lspHighlight,
		lspInlayHints,
		lspGutter,
		linkHover,
		cmdClick,
		lspFeatureTheme,
	];
}

/** Format on save: ask the server first; the caller saves the (possibly reformatted) text after. */
export async function formatBeforeSave(view: EditorView): Promise<void> {
	const env = envOf(view);
	if (!env?.formatOnSave() || !env.doc.open) return;
	await formatDocument(view).catch(() => false);
}
