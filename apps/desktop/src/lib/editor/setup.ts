/**
 * The editor's extension bundle, replacing `basicSetup`. `editorExtensions` returns the static base
 * plus the compartments the integrator reconfigures when a preference or a large-file tier changes
 * (`reconfigure(view, compartments, opts)`); nothing here reads the DOM or the prefs store.
 *
 * Not included on purpose: the LSP completion source (`override`, added by `lib/lsp`), the theme and
 * syntax highlighting (`lib/cm.ts`), languages, and `lineWrapping`'s toggle UI (the compartment is
 * here, the button is the integrator's).
 */
import {
	autocompletion,
	closeBrackets,
	closeBracketsKeymap,
	completionKeymap,
} from "@codemirror/autocomplete";
import { history, historyKeymap, standardKeymap } from "@codemirror/commands";
import { bracketMatching, foldGutter, foldKeymap, indentOnInput, indentUnit } from "@codemirror/language";
import { lintKeymap } from "@codemirror/lint";
import { highlightSelectionMatches } from "@codemirror/search";
import { Compartment, EditorState, Prec, type Extension } from "@codemirror/state";
import {
	EditorView,
	crosshairCursor,
	drawSelection,
	dropCursor,
	highlightActiveLine,
	highlightActiveLineGutter,
	highlightSpecialChars,
	keymap,
	lineNumbers,
	rectangularSelection,
} from "@codemirror/view";
import { bracketColors } from "#lib/editor/bracketColors.ts";
import { indentGuides, renderWhitespace, rulers } from "#lib/editor/guides.ts";
import { editorKeymap } from "#lib/editor/keymap.ts";
import { minimap } from "#lib/editor/minimap.ts";
import { scrollPastEnd } from "#lib/editor/scrollPastEnd.ts";
import { findPanel } from "#lib/editor/search.ts";
import type { EditorPrefs } from "#lib/editor/settings.ts";
import { stickyScroll } from "#lib/editor/sticky.ts";
import { editorLayoutTheme } from "#lib/editor/theme.ts";

/** What a large-file tier allows; the integrator maps `largeFile.ts` output onto it. */
export interface Features {
	bracketColors: boolean;
	indentGuides: boolean;
	stickyScroll: boolean;
	foldGutter: boolean;
	selectionMatches: boolean;
	minimap: boolean;
	wrap: boolean;
}

export interface EditorSetupOptions {
	prefs: EditorPrefs;
	features: Features;
}

/** Everything that can change after the view exists. */
export interface EditorCompartments {
	lineNumbers: Compartment;
	foldGutter: Compartment;
	activeLine: Compartment;
	wrap: Compartment;
	indent: Compartment;
	guides: Compartment;
	brackets: Compartment;
	sticky: Compartment;
	minimap: Compartment;
	whitespace: Compartment;
	rulers: Compartment;
	scrollPastEnd: Compartment;
	selectionMatches: Compartment;
}

export interface EditorSetup {
	base: Extension;
	compartments: EditorCompartments;
}

/**
 * Extra auto-closing pairs on top of `()[]{}''""`: backticks everywhere, and `<>` is left to
 * `autoCloseTags` (JSX/HTML) because it would close comparison operators.
 */
const closePairs = EditorState.languageData.of(() => [
	{ closeBrackets: { brackets: ["(", "[", "{", "'", '"', "`"] } },
]);

const attributes = EditorView.contentAttributes.of({
	spellcheck: "false",
	autocorrect: "off",
	autocapitalize: "off",
	"aria-label": "Editor",
});

/** Indent unit string for the prefs: N spaces or one tab. */
export function indentUnitFor(prefs: Pick<EditorPrefs, "tab_size" | "insert_spaces">): string {
	return prefs.insert_spaces ? " ".repeat(prefs.tab_size) : "\t";
}

/** The compartment contents for `prefs` and `features`, keyed like `EditorCompartments`. */
export function slots({ prefs, features }: EditorSetupOptions): Record<keyof EditorCompartments, Extension> {
	return {
		lineNumbers: prefs.line_numbers ? lineNumbers() : [],
		foldGutter: features.foldGutter ? foldGutter() : [],
		activeLine: prefs.highlight_active_line ? [highlightActiveLine(), highlightActiveLineGutter()] : [],
		wrap: features.wrap || prefs.word_wrap ? EditorView.lineWrapping : [],
		indent: [EditorState.tabSize.of(prefs.tab_size), indentUnit.of(indentUnitFor(prefs))],
		guides: features.indentGuides && prefs.indent_guides ? indentGuides : [],
		brackets: features.bracketColors && prefs.bracket_pair_colorization ? bracketColors : [],
		sticky: features.stickyScroll && prefs.sticky_scroll ? stickyScroll : [],
		minimap: features.minimap && prefs.minimap ? minimap : [],
		whitespace: renderWhitespace(prefs.render_whitespace),
		rulers: rulers(prefs.rulers),
		scrollPastEnd: prefs.scroll_past_end ? scrollPastEnd() : [],
		selectionMatches: features.selectionMatches ? highlightSelectionMatches() : [],
	};
}

export function editorExtensions(opts: EditorSetupOptions): EditorSetup {
	const compartments: EditorCompartments = {
		lineNumbers: new Compartment(),
		foldGutter: new Compartment(),
		activeLine: new Compartment(),
		wrap: new Compartment(),
		indent: new Compartment(),
		guides: new Compartment(),
		brackets: new Compartment(),
		sticky: new Compartment(),
		minimap: new Compartment(),
		whitespace: new Compartment(),
		rulers: new Compartment(),
		scrollPastEnd: new Compartment(),
		selectionMatches: new Compartment(),
	};
	const initial = slots(opts);
	const dynamic = (Object.keys(compartments) as (keyof EditorCompartments)[]).map((k) =>
		compartments[k].of(initial[k]),
	);
	const base: Extension = [
		highlightSpecialChars(),
		history(),
		drawSelection(),
		dropCursor(),
		EditorState.allowMultipleSelections.of(true),
		indentOnInput(),
		bracketMatching(),
		closeBrackets(),
		closePairs,
		autocompletion({ defaultKeymap: true, closeOnBlur: false }),
		rectangularSelection({ eventFilter: (e) => e.altKey && e.shiftKey }),
		crosshairCursor({ key: "Alt" }),
		findPanel,
		attributes,
		dynamic,
		editorLayoutTheme,
		// Editor bindings win over the standard CodeMirror ones (Tab, ⌘Enter…); the rest keep their defaults.
		Prec.high(editorKeymap),
		keymap.of([
			...closeBracketsKeymap,
			...standardKeymap,
			...historyKeymap,
			...foldKeymap,
			...completionKeymap,
			...lintKeymap,
		]),
	];
	return { base, compartments };
}

/** Apply new prefs/features to a live view through the compartments; unchanged slots are skipped by CodeMirror. */
export function reconfigure(
	view: EditorView,
	compartments: EditorCompartments,
	opts: EditorSetupOptions,
): void {
	const next = slots(opts);
	view.dispatch({
		effects: (Object.keys(compartments) as (keyof EditorCompartments)[]).map((k) =>
			compartments[k].reconfigure(next[k]),
		),
	});
}
