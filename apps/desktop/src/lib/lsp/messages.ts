/**
 * The sentence an LSP feature (F12, F2, ⇧F12, ⌘., format) shows when no server serves the file,
 * and the once-per-document gate so a held key never repeats it. Pure: the caller renders it.
 */
import { m } from "#lib/i18n/m.ts";
import type { ActionId, LspUiState } from "#lib/lsp/status.ts";

export type Feature =
	"definition" | "type-definition" | "implementation" | "references" | "rename" | "code-actions" | "format";

const NEEDS: Record<Feature, () => string> = {
	definition: () => m.workbench_lsp_needs_definition(),
	"type-definition": () => m.workbench_lsp_needs_type_definition(),
	implementation: () => m.workbench_lsp_needs_implementation(),
	references: () => m.workbench_lsp_needs_references(),
	rename: () => m.workbench_lsp_needs_rename(),
	"code-actions": () => m.workbench_lsp_needs_code_actions(),
	format: () => m.workbench_lsp_needs_format(),
};

/** What to say about why, per state; `undefined` = the state is serving (nothing to explain). */
function why(s: LspUiState): string | undefined {
	switch (s.kind) {
		case "ready":
			return undefined;
		case "starting":
			return m.workbench_lsp_why_starting();
		case "installing":
			return s.progress
				? m.workbench_lsp_why_installing_progress({ progress: s.progress })
				: m.workbench_lsp_why_installing();
		case "host-old":
			return m.workbench_lsp_why_host_old();
		case "host-updating":
			return m.workbench_lsp_why_host_updating();
		case "disabled-setting":
			return m.workbench_lsp_why_disabled_setting();
		case "disabled-language":
			return m.workbench_lsp_why_disabled_language();
		case "no-language":
			return m.workbench_lsp_why_no_language();
		case "crashed":
			return m.workbench_lsp_why_crashed();
		case "outside-root":
			return m.workbench_lsp_why_outside_root();
		default:
			return s.summary;
	}
}

/** One inline sentence for `feature` given the file's state; `undefined` when a server serves it. */
export function reasonForFeature(feature: Feature, s: LspUiState): string | undefined {
	const reason = why(s);
	return reason === undefined ? undefined : `${NEEDS[feature]()} ${reason}`;
}

/** The action to offer next to the sentence (the most useful one for the state), if any. */
export function hintAction(s: LspUiState): { id: ActionId; label: string } | undefined {
	const a = s.actions.find((x) => x.primary) ?? s.actions[0];
	return a ? { id: a.id, label: a.label } : undefined;
}

/**
 * Each (document, feature) shows its explanation once; afterwards the shortcut is silent so a
 * repeated F12 does not nag. `session` is the document (any object) the editor holds.
 */
const shown = new WeakMap<object, Set<Feature>>();

/** True the first time `feature` is explained for `session`; false afterwards. */
export function firstTime(session: object, feature: Feature): boolean {
	let set = shown.get(session);
	if (!set) shown.set(session, (set = new Set()));
	if (set.has(feature)) return false;
	set.add(feature);
	return true;
}

/** A held key repeats every ~30 ms; anything slower is a deliberate press. */
const HOLD_MS = 400;
const lastShown = new WeakMap<object, Map<Feature, number>>();

/**
 * What to do with a shortcut that has nothing to run: `show` the reason, or `hold` (a key held down
 * repeats; one note is enough). Unlike `firstTime`, a later deliberate press is answered again: a
 * shortcut that does nothing and says nothing is the bug this exists to prevent.
 */
export function feedbackFor(session: object, feature: Feature, now = Date.now()): "show" | "hold" {
	let last = lastShown.get(session);
	if (!last) lastShown.set(session, (last = new Map()));
	const prev = last.get(feature);
	last.set(feature, now);
	return prev !== undefined && now - prev < HOLD_MS ? "hold" : "show";
}

/** Forget what was shown for `session` (the state changed: a new explanation is news again). */
export function resetHints(session: object) {
	shown.delete(session);
}
