import { EditorView } from "codemirror";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";

/** Style for one Zed syntax key: theme var first, then the kit token that was hardcoded before themes. */
const syn = (key: string, fallback: string, weight?: string, style = "normal") => ({
	color: `var(--cm-syn-${key}, ${fallback})`,
	fontStyle: `var(--cm-syn-${key}-italic, ${style})`,
	fontWeight: `var(--cm-syn-${key}-weight, ${weight ?? "normal"})`,
});

/** Token colors come from the active theme's `syntax.*` (see theme.ts); fallbacks keep the Atlas look. */
export const highlight = HighlightStyle.define([
	{ tag: [t.propertyName, t.definition(t.propertyName)], ...syn("property", "var(--ca-brand-main)", "500") },
	{ tag: [t.string, t.special(t.string)], ...syn("string", "var(--ca-semantic-success)") },
	{ tag: [t.number, t.atom], ...syn("number", "var(--ca-semantic-warning)") },
	{ tag: [t.bool, t.null], ...syn("boolean", "var(--ca-semantic-warning)") },
	{
		tag: [t.comment, t.lineComment, t.blockComment],
		...syn("comment", "var(--ca-text-disabled)", undefined, "italic"),
	},
	{ tag: [t.punctuation, t.separator, t.meta], ...syn("punctuation", "var(--ca-text-secondary)") },
	{ tag: [t.keyword, t.controlKeyword], ...syn("keyword", "var(--ca-semantic-info)") },
	{ tag: [t.typeName, t.className], ...syn("type", "var(--ca-semantic-info)") },
	{ tag: [t.labelName], ...syn("label", "var(--ca-semantic-info)") },
	{
		tag: [t.function(t.variableName), t.function(t.propertyName)],
		...syn("function", "var(--ca-brand-main)"),
	},
	{ tag: [t.operator], ...syn("operator", "var(--cm-syn-punctuation, var(--ca-text-secondary))") },
	{ tag: [t.tagName], ...syn("tag", "var(--ca-semantic-info)") },
	{ tag: [t.attributeName], ...syn("attribute", "var(--ca-brand-main)") },
	{ tag: [t.heading], ...syn("title", "inherit", "700") },
]);

const theme = EditorView.theme({
	"&": {
		height: "100%",
		fontSize: "var(--atlas-buffer-font-size, 12.5px)",
		backgroundColor: "transparent",
		color: "var(--cm-fg, inherit)",
	},
	".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "var(--atlas-buffer-line-height, 1.55)" },
	".cm-gutters": {
		backgroundColor: "transparent",
		border: "none",
		color: "var(--cm-gutter, var(--ca-text-disabled))",
	},
	".cm-activeLine": { backgroundColor: "var(--cm-active-line, var(--ca-action-hover))" },
	".cm-activeLineGutter": {
		backgroundColor: "var(--cm-active-line, var(--ca-action-hover))",
		color: "var(--cm-gutter-active, var(--cm-gutter, var(--ca-text-disabled)))",
	},
	"&.cm-focused": { outline: "none" },
	".cm-content": { caretColor: "var(--cm-caret, var(--ca-brand-main))" },
	".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--cm-caret, var(--ca-brand-main))" },
	"&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground": {
		backgroundColor: "var(--cm-selection, var(--ca-action-selected))",
	},
	".cm-tooltip, .cm-panels": {
		background: "var(--ca-surface-overlay)",
		border: "0.5px solid var(--ca-outline-subtle)",
		borderRadius: "8px",
		color: "var(--ca-text-primary)",
	},
});

/** Atlas look for every CodeMirror editor. */
export const atlasTheme: Extension = [theme, syntaxHighlighting(highlight)];
