import { EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { m } from "#lib/i18n/m.ts";

/**
 * The words CodeMirror's own panels and announcements use (`state.phrase("Go to line")`), keyed by its English, in the
 * app's language: the Go to line box, the fold marker, the completion and diagnostics lists and what a screen reader hears
 * after a replace. The keys are exactly the strings the `@codemirror/*` packages ask for, so a package update that adds a
 * phrase shows English for it until it is added here.
 *
 * Read inside `slots()` (an effect), so a language change reaches editors that are already open.
 */
export function localizedEditor(): Extension {
	return [
		EditorState.phrases.of({
			"Go to line": m.workbench_editor_goto_line(),
			go: m.workbench_editor_cm_go(),
			close: m.workbench_editor_cm_close(),
			"current match": m.workbench_editor_cm_current_match(),
			"on line": m.workbench_editor_cm_on_line(),
			"replaced $ matches": m.workbench_editor_cm_replaced_matches(),
			"replaced match on line $": m.workbench_editor_cm_replaced_match_on_line(),
			"folded code": m.workbench_editor_cm_folded_code(),
			unfold: m.workbench_editor_cm_unfold(),
			to: m.workbench_editor_cm_to(),
			Completions: m.workbench_editor_cm_completions(),
			Diagnostics: m.workbench_editor_cm_diagnostics(),
			"No diagnostics": m.workbench_editor_cm_no_diagnostics(),
			"Control character": m.workbench_editor_cm_control_character(),
			"Selection deleted": m.workbench_editor_cm_selection_deleted(),
		}),
		EditorView.contentAttributes.of({ "aria-label": m.workbench_editor_label() }),
	];
}
