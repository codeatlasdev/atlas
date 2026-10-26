import { EditorView } from "codemirror";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { RangeSetBuilder, type Extension } from "@codemirror/state";
import { Decoration, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

/** Style for one Zed syntax role: theme var first, then the kit token that was used before themes. */
const syn = (key: string, fallback: string) => ({
	color: `var(--cm-syn-${key}, ${fallback})`,
	fontStyle: `var(--cm-syn-${key}-italic, normal)`,
	fontWeight: `var(--cm-syn-${key}-weight, normal)`,
});

const FG = "var(--cm-fg, var(--ca-text-primary))";
const ERR = "var(--ca-semantic-error)";
const WARN = "var(--ca-semantic-warning)";
const INFO = "var(--ca-semantic-info)";
const SELECTION = "var(--cm-selection, var(--ca-action-selected))";
const SEARCH = "var(--cm-search-match, color-mix(in oklab, var(--ca-semantic-warning) 35%, transparent))";
const SEARCH_ACTIVE =
	"var(--cm-search-match-active, color-mix(in oklab, var(--ca-semantic-warning) 65%, transparent))";
const DISABLED = "var(--cm-syn-comment, var(--ca-text-disabled))";

/**
 * Token colors come from the active theme's `syntax.*` roles (see theme.ts; every role is always
 * defined, refinements inherit from their base role). Fallbacks only matter before a theme loads.
 */
export const highlight = HighlightStyle.define([
	{ tag: [t.variableName, t.definition(t.variableName), t.local(t.variableName)], ...syn("variable", FG) },
	{ tag: [t.special(t.variableName), t.self], ...syn("variable-special", "var(--cm-syn-variable)") },
	{ tag: [t.propertyName, t.definition(t.propertyName)], ...syn("property", FG) },
	{
		tag: [t.function(t.variableName), t.function(t.definition(t.variableName)), t.function(t.propertyName)],
		...syn("function", "var(--ca-brand-main)"),
	},
	{ tag: [t.macroName], ...syn("macro", "var(--cm-syn-function)") },
	{
		tag: [t.keyword, t.definitionKeyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword, t.modifier],
		...syn("keyword", INFO),
	},
	{ tag: [t.className, t.typeName, t.standard(t.typeName)], ...syn("type", INFO) },
	{ tag: [t.definition(t.typeName)], ...syn("type-parameter", "var(--cm-syn-type)") },
	{ tag: [t.namespace], ...syn("namespace", "var(--cm-syn-type)") },
	{ tag: [t.labelName], ...syn("label", INFO) },
	{ tag: [t.string, t.character, t.special(t.string)], ...syn("string", "var(--ca-semantic-success)") },
	{ tag: [t.escape], ...syn("string-escape", "var(--cm-syn-string)") },
	{ tag: [t.regexp], ...syn("string-regex", "var(--cm-syn-string)") },
	{ tag: [t.number, t.integer, t.float, t.unit], ...syn("number", WARN) },
	{ tag: [t.bool, t.null], ...syn("boolean", WARN) },
	{ tag: [t.atom, t.color, t.constant(t.name), t.constant(t.className)], ...syn("constant", WARN) },
	{
		tag: [t.comment, t.lineComment, t.blockComment],
		...syn("comment", "var(--ca-text-disabled)"),
	},
	{ tag: [t.docComment], ...syn("comment-doc", "var(--cm-syn-comment)") },
	{
		tag: [
			t.operator,
			t.compareOperator,
			t.logicOperator,
			t.arithmeticOperator,
			t.bitwiseOperator,
			t.definitionOperator,
			t.derefOperator,
			t.updateOperator,
			t.typeOperator,
			t.controlOperator,
		],
		...syn("operator", "var(--cm-syn-punctuation, var(--ca-text-secondary))"),
	},
	{
		tag: [t.punctuation, t.separator, t.special(t.brace)],
		...syn("punctuation", "var(--ca-text-secondary)"),
	},
	{
		tag: [t.brace, t.paren, t.squareBracket, t.angleBracket, t.bracket],
		...syn("punctuation-bracket", "var(--cm-syn-punctuation, var(--ca-text-secondary))"),
	},
	{ tag: [t.tagName], ...syn("tag", INFO) },
	{ tag: [t.attributeName], ...syn("attribute", "var(--ca-brand-main)") },
	{ tag: [t.attributeValue], ...syn("string", "var(--ca-semantic-success)") },
	{
		tag: [t.heading, t.heading1, t.heading2, t.heading3, t.heading4, t.heading5, t.heading6],
		...syn("title", "inherit"),
		fontWeight: "var(--cm-syn-title-weight, 700)",
	},
	{
		tag: [t.strong],
		...syn("emphasis-strong", "inherit"),
		fontWeight: "var(--cm-syn-emphasis-strong-weight, 700)",
	},
	{ tag: [t.emphasis], ...syn("emphasis", "inherit"), fontStyle: "var(--cm-syn-emphasis-italic, italic)" },
	{ tag: [t.strikethrough], textDecoration: "line-through" },
	{ tag: [t.monospace], ...syn("text-literal", "var(--cm-syn-string)") },
	{ tag: [t.link], ...syn("link-text", "var(--cm-syn-link-uri)"), textDecoration: "underline" },
	{ tag: [t.url], ...syn("link-uri", INFO), textDecoration: "underline" },
	{ tag: [t.list], ...syn("punctuation-list_marker", "var(--cm-syn-punctuation)") },
	{ tag: [t.quote], ...syn("comment", "var(--ca-text-secondary)") },
	{ tag: [t.processingInstruction, t.meta], ...syn("punctuation-list_marker", "var(--cm-syn-punctuation)") },
	{ tag: [t.invalid], color: ERR, textDecoration: "underline wavy" },
]);

const TOOLTIP_BG = "var(--ca-surface-overlay)";
const POPUP = {
	background: TOOLTIP_BG,
	border: "0.5px solid var(--ca-outline-default)",
	borderRadius: "8px",
	color: "var(--ca-text-primary)",
	boxShadow: "0 8px 24px color-mix(in oklab, black 35%, transparent)",
};

/** A lint underline: wavy text-decoration in `color` (replaces lint's fixed-color SVG squiggle). */
const squiggle = (color: string) => ({
	backgroundImage: "none",
	textDecoration: `underline wavy ${color}`,
	textDecorationSkipInk: "none",
	textUnderlineOffset: "3px",
});

/**
 * Dracula spec: parameters are orange italic. Lezer tags a parameter as a plain `variableName`, so the
 * declaration is marked from the tree: `VariableDefinition` inside a JS/TS `ParamList`, `BoundIdentifier`
 * inside a Rust `Parameter`/`ParamList`, `Parameter`/`Identifier` in a Python `ParamList`.
 */
const PARAM_NODES: Record<string, true> = {
	VariableDefinition: true,
	BoundIdentifier: true,
	Identifier: true,
};
const PARAM_PARENTS: Record<string, true> = { ParamList: true, Parameter: true, ArrowParams: true };
const paramMark = Decoration.mark({ class: "cm-param" });

function paramDecorations(view: EditorView): DecorationSet {
	const b = new RangeSetBuilder<Decoration>();
	for (const { from, to } of view.visibleRanges)
		syntaxTree(view.state).iterate({
			from,
			to,
			enter: (n) => {
				if (n.name in PARAM_NODES && n.node.parent && n.node.parent.name in PARAM_PARENTS)
					b.add(n.from, n.to, paramMark);
			},
		});
	return b.finish();
}

const paramHighlight = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;
		constructor(view: EditorView) {
			this.decorations = paramDecorations(view);
		}
		update(u: ViewUpdate) {
			if (u.docChanged || u.viewportChanged || syntaxTree(u.state) !== syntaxTree(u.startState))
				this.decorations = paramDecorations(u.view);
		}
	},
	{ decorations: (v) => v.decorations },
);

const theme = EditorView.theme({
	".cm-param": {
		color: "var(--cm-syn-variable-parameter, var(--cm-fg, var(--ca-text-primary)))",
		fontStyle: "var(--cm-syn-variable-parameter-italic, normal)",
	},
	// Layout: the editor is the whole pane, whatever the document holds. CodeMirror's base theme pins
	// `.cm-editor` to `position: relative !important`, so it cannot be taken out of flow: its host is
	// a plain block (see Editor.svelte `.cm`) and the editor fills it with `width`/`height` 100%.
	// Inside a flex row it was a content-sized item, i.e. as wide as the longest line. The scroller
	// is the only scroll container (both axes, contained so the app never rubber-bands) and the
	// content is at least as wide as the pane.
	"&": {
		width: "100%",
		height: "100%",
		fontSize: "var(--atlas-buffer-font-size, 12.5px)",
		backgroundColor: "transparent",
		color: FG,
	},
	".cm-scroller": {
		fontFamily: "var(--mono)",
		lineHeight: "var(--atlas-buffer-line-height, 1.55)",
		overflow: "auto",
		overscrollBehavior: "contain",
		scrollbarGutter: "auto",
	},
	// The window is `user-select: none` (app.css) and that inherits. WKWebView puts no caret and no selection in an
	// editable element that inherits `none`: the active line and the status bar moved, nothing blinked. Opt back in,
	// prefixed too (style-mod does not add the prefix).
	".cm-content": {
		caretColor: "var(--cm-caret, var(--ca-brand-main))",
		minWidth: "100%",
		boxSizing: "border-box",
		userSelect: "text",
		"-webkit-user-select": "text",
	},
	"&.cm-focused": { outline: "none" },
	".cm-cursor, .cm-dropCursor": {
		borderLeftColor: "var(--cm-caret, var(--ca-brand-main))",
		borderLeftWidth: "2px",
	},
	".cm-fat-cursor": {
		backgroundColor: "var(--cm-caret, var(--ca-brand-main))",
		color: "var(--ca-surface-base)",
	},

	// Gutters and lines
	".cm-gutters": {
		position: "sticky",
		left: 0,
		zIndex: 2,
		flexShrink: 0,
		backgroundColor: "var(--cm-gutter-bg, var(--ca-surface-base))",
		border: "none",
		color: "var(--cm-gutter, var(--ca-text-disabled))",
	},
	".cm-activeLine": { backgroundColor: "var(--cm-active-line, var(--ca-action-hover))" },
	".cm-activeLineGutter": {
		backgroundColor: "var(--cm-active-line, var(--ca-action-hover))",
		color: "var(--cm-gutter-active, var(--ca-text-primary))",
	},
	// UI icon: 3:1 needed; `opacity` multiplied into the colour and measured 3.6:1
	".cm-foldGutter span": { color: "var(--ca-text-secondary)" },
	".cm-foldGutter span:hover": { color: "var(--ca-text-primary)", opacity: 1 },
	".cm-foldPlaceholder": {
		backgroundColor: "color-mix(in oklab, var(--ca-text-primary) 10%, transparent)",
		border: "0.5px solid var(--ca-outline-default)",
		color: DISABLED,
		borderRadius: "4px",
		padding: "0 5px",
	},

	// Selection: the unfocused layer is dimmer so the active pane stays obvious.
	".cm-selectionBackground": {
		backgroundColor: "color-mix(in oklab, var(--cm-selection, var(--ca-action-selected)) 55%, transparent)",
	},
	"&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-content ::selection": {
		backgroundColor: SELECTION,
	},
	".cm-selectionMatch": {
		backgroundColor:
			"color-mix(in oklab, var(--cm-lsp-highlight, var(--ca-action-selected)) 60%, transparent)",
	},

	// Brackets
	"&.cm-focused .cm-matchingBracket, .cm-matchingBracket": {
		backgroundColor: "var(--cm-bracket-match, var(--ca-action-selected))",
		outline: "0.5px solid var(--ca-outline-strong)",
		borderRadius: "2px",
	},
	"&.cm-focused .cm-nonmatchingBracket, .cm-nonmatchingBracket": {
		backgroundColor: "color-mix(in oklab, var(--ca-semantic-error) 25%, transparent)",
		color: ERR,
		borderRadius: "2px",
	},

	// Search
	".cm-searchMatch": { backgroundColor: SEARCH, borderRadius: "2px" },
	".cm-searchMatch.cm-searchMatch-selected": {
		backgroundColor: SEARCH_ACTIVE,
		outline: "1px solid var(--ca-outline-strong)",
	},

	// Panels, tooltips
	".cm-tooltip, .cm-panels": POPUP,
	".cm-panels": { borderRadius: 0, boxShadow: "none" },
	".cm-panels-top": { borderTop: "none", borderLeft: "none", borderRight: "none" },
	".cm-panels-bottom": { borderBottom: "none", borderLeft: "none", borderRight: "none" },
	".cm-panel.cm-search": { color: "var(--ca-text-secondary)" },
	".cm-panel.cm-search input, .cm-panel.cm-search button": { font: "inherit", fontSize: "12px" },
	".cm-textfield": {
		background: "var(--ca-surface-base)",
		color: "var(--ca-text-primary)",
		border: "0.5px solid var(--atlas-control-border, var(--ca-outline-default))",
		borderRadius: "5px",
		padding: "2px 6px",
	},
	".cm-textfield:focus": {
		outline: "1px solid var(--atlas-focus-ring, var(--ca-brand-main))",
		outlineOffset: 0,
	},
	".cm-button": {
		backgroundImage: "none",
		background: "var(--ca-action-hover)",
		color: "var(--ca-text-primary)",
		border: "0.5px solid var(--atlas-control-border, var(--ca-outline-default))",
		borderRadius: "5px",
	},
	".cm-button:active": { backgroundImage: "none", background: "var(--ca-action-selected)" },
	".cm-panel.cm-search [name=close]": { color: "var(--ca-text-secondary)" },

	// Autocomplete
	".cm-tooltip.cm-tooltip-autocomplete > ul > li": { color: "var(--ca-text-primary)", padding: "1px 6px" },
	".cm-tooltip-autocomplete ul li[aria-selected]": {
		background: SELECTION,
		color: "var(--ca-text-primary)",
	},
	".cm-completionMatchedText": {
		textDecoration: "underline",
		textDecorationColor: "var(--ca-brand-main)",
		textUnderlineOffset: "2px",
		color: "var(--ca-brand-main)",
		fontWeight: 600,
	},
	".cm-completionDetail": { color: "var(--ca-text-secondary)", fontStyle: "normal" },
	".cm-completionInfo": { ...POPUP, color: "var(--ca-text-secondary)" },
	".cm-completionIcon": { opacity: 0.7 },

	// Lint
	".cm-lintRange-error": squiggle(ERR),
	".cm-lintRange-warning": squiggle(WARN),
	".cm-lintRange-info": squiggle(INFO),
	".cm-lintRange-hint": squiggle("var(--cm-syn-hint, var(--ca-text-disabled))"),
	".cm-lintRange-active": {
		backgroundColor: "color-mix(in oklab, var(--ca-semantic-error) 18%, transparent)",
	},
	".cm-tooltip-lint": { color: "var(--ca-text-primary)" },
	".cm-diagnostic": { padding: "4px 8px", color: "var(--ca-text-primary)" },
	".cm-diagnostic-error": { borderLeft: `3px solid ${ERR}` },
	".cm-diagnostic-warning": { borderLeft: `3px solid ${WARN}` },
	".cm-diagnostic-info": { borderLeft: `3px solid ${INFO}` },
	".cm-diagnostic-hint": { borderLeft: "3px solid var(--cm-syn-hint, var(--ca-text-disabled))" },
	".cm-diagnosticSource": { color: "var(--ca-text-secondary)" },
	".cm-diagnosticAction": {
		background: "var(--ca-action-hover)",
		color: "var(--ca-brand-main)",
		borderRadius: "4px",
	},
	// @codemirror/lint draws its gutter markers as fixed-color SVG `content`: replace with themed shapes.
	".cm-lint-marker": { width: "0.9em", height: "0.9em" },
	".cm-lint-marker-error": { content: '""', background: ERR, borderRadius: "50%" },
	".cm-lint-marker-warning": {
		content: '""',
		background: WARN,
		clipPath: "polygon(50% 0, 100% 100%, 0 100%)",
	},
	".cm-lint-marker-info": { content: '""', background: INFO, borderRadius: "2px" },
	".cm-lint-marker-hint": {
		content: '""',
		background: "var(--cm-syn-hint, var(--ca-text-disabled))",
		borderRadius: "2px",
	},
});

/** Atlas look for every CodeMirror editor. */
export const atlasTheme: Extension = [theme, syntaxHighlighting(highlight), paramHighlight];
