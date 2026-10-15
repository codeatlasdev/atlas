/**
 * The registry of every token the app reads from a theme. A theme file may define any subset:
 * `derive.ts` completes the rest, and the tests prove that every token below resolves for
 * every bundled theme. Add a token HERE first, then read its CSS var.
 *
 * Keys follow Zed's theme schema (https://zed.dev/schema/themes/v0.2.0.json) where it has one;
 * `atlas`-specific keys are noted. Values are `#rrggbb` / `#rrggbbaa`.
 */

/** Style key → kit custom properties it drives (`--ca-*`) or app-level vars (`--atlas-*`). */
export const UI_TOKENS: Record<string, string[]> = {
	/** The chrome ground; also the fill of a disabled control, so its `--ca-text-disabled` label (3:1) sits on a chrome surface. */
	background: ["--ca-surface-sunken", "--ca-action-disabled-bg"],
	"editor.background": ["--ca-surface-base"],
	"surface.background": ["--ca-surface-raised"],
	"elevated_surface.background": ["--ca-surface-overlay"],
	text: ["--ca-text-primary"],
	"text.muted": ["--ca-text-secondary", "--ca-text-hint"],
	"text.disabled": ["--ca-text-disabled"],
	"text.accent": [
		"--ca-brand-main",
		"--ca-brand-light",
		"--ca-brand-dark",
		"--ca-brand-fill",
		"--ca-brand-fill-hover",
		"--ca-brand-text",
	],
	/** Text on a filled accent button. */
	"text.on_accent": ["--ca-brand-contrast"],
	"border.variant": ["--ca-outline-subtle"],
	border: ["--ca-outline-default"],
	"border.focused": ["--ca-outline-strong"],
	/** The boundary of a text field / select / control: 3:1 on every surface it sits on (WCAG 2.2 1.4.11). The kit draws
	 * its fields, checkboxes and radios from `--ca-outline-control`; `--atlas-control-border` is the app's own fields. */
	"border.control": ["--ca-outline-control", "--atlas-control-border"],
	/** Label on the destructive button's fill (the error colour): black or white by contrast. */
	"error.on_fill": ["--ca-semantic-error-contrast"],
	/** The tooltip is inverted (text colour as fill, surface as ink): 7:1+ on any theme. */
	"tooltip.background": ["--ca-tooltip-bg"],
	"tooltip.foreground": ["--ca-tooltip-fg"],
	/** The overlay (menus, popovers) is opaque: what is behind it never changes what text on it reads. */
	"overlay.background": ["--ca-overlay-bg"],
	/** Keyboard focus ring: the kit's own components read `--ca-focus-ring`, the app's controls `--atlas-focus-ring`. */
	"border.selected": ["--ca-focus-ring", "--atlas-focus-ring"],
	"element.hover": ["--ca-action-hover"],
	"element.selected": ["--ca-action-selected"],
	/** Success / warning / info / error as TEXT and fills: AA on every surface of the theme. */
	created: ["--ca-semantic-success"],
	warning: ["--ca-semantic-warning"],
	info: ["--ca-semantic-info"],
	error: ["--ca-semantic-error", "--ca-semantic-error-fill"],
	/** Non-text indicators (dots, rings, stripes); fall back to the semantic colors. */
	"state.success": ["--atlas-state-success"],
	"state.warning": ["--atlas-state-warning"],
	"state.info": ["--atlas-state-info"],
	"state.error": ["--atlas-state-error"],
	"version_control.added": ["--atlas-vcs-added"],
	"version_control.modified": ["--atlas-vcs-modified"],
	"version_control.deleted": ["--atlas-vcs-deleted"],
	"version_control.renamed": ["--atlas-vcs-renamed"],
	/** Gitignored files and folders in the explorer: dimmed (quieter than `text`) yet AA on every surface. */
	"version_control.ignored": ["--atlas-vcs-ignored"],
	"scrollbar.thumb.background": ["--atlas-scrollbar-thumb"],
	"scrollbar.thumb.hover_background": ["--atlas-scrollbar-thumb-hover"],
	"link_text.hover": ["--atlas-link"],
};

/** Editor chrome keys → `--cm-*` vars read by cm.ts and the LSP/viewers (they fall back to kit tokens). */
export const EDITOR_TOKENS: Record<string, string> = {
	"editor.foreground": "--cm-fg",
	"editor.line_number": "--cm-gutter",
	"editor.active_line_number": "--cm-gutter-active",
	"editor.active_line.background": "--cm-active-line",
	"editor.document_highlight.read_background": "--cm-lsp-highlight",
	"editor.document_highlight.bracket_background": "--cm-bracket-match",
	"search.match_background": "--cm-search-match",
	"search.active_match_background": "--cm-search-match-active",
};

/** Colors in `players[0]`. */
export const PLAYER_TOKENS = { cursor: "--cm-caret", selection: "--cm-selection" } as const;

/** Terminal keys (`terminal.ansi.*` is listed in `ANSI_NAMES`). */
export const TERMINAL_KEYS = [
	"terminal.background",
	"terminal.foreground",
	"terminal.cursor",
	"terminal.cursor_text",
	"terminal.selection.background",
	"terminal.selection.foreground",
] as const;

export const ANSI_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"] as const;

/** `terminal.ansi.<name>` for slots 0…15 (normal, then bright). */
export const ANSI_KEYS: string[] = [
	...ANSI_NAMES.map((n) => `terminal.ansi.${n}`),
	...ANSI_NAMES.map((n) => `terminal.ansi.bright_${n}`),
];

/**
 * Syntax roles (`style.syntax.<role>`). CSS: `--cm-syn-<role>` (dots become dashes) plus
 * `-italic` and `-weight` companions. Roles after the first group are optional refinements:
 * a theme that omits one inherits `ROLE_FALLBACK[role]`.
 */
export const SYNTAX_ROLES = [
	"variable",
	"property",
	"string",
	"string.special",
	"number",
	"boolean",
	"constant",
	"comment",
	"comment.doc",
	"punctuation",
	"keyword",
	"type",
	"label",
	"function",
	"title",
	"operator",
	"attribute",
	"tag",
	"link_uri",
	"emphasis",
	// refinements
	"variable.special",
	"variable.parameter",
	"string.escape",
	"string.regex",
	"constructor",
	"type.parameter",
	"namespace",
	"macro",
	"hint",
	"text.literal",
	"emphasis.strong",
	"punctuation.bracket",
	"punctuation.list_marker",
	"link_text",
] as const;

export type SyntaxRole = (typeof SYNTAX_ROLES)[number];

/** Where a role not defined by the theme inherits from. Chains end in a base role. */
export const ROLE_FALLBACK: Partial<Record<SyntaxRole, SyntaxRole>> = {
	"comment.doc": "comment",
	"string.special": "string",
	"string.escape": "string",
	"string.regex": "string",
	"variable.special": "variable",
	"variable.parameter": "variable",
	constructor: "type",
	"type.parameter": "type",
	namespace: "type",
	macro: "function",
	hint: "comment",
	"text.literal": "string",
	"emphasis.strong": "emphasis",
	"punctuation.bracket": "punctuation",
	"punctuation.list_marker": "punctuation",
	link_text: "link_uri",
	label: "keyword",
	boolean: "number",
	constant: "number",
	attribute: "property",
	title: "variable",
	operator: "punctuation",
	property: "variable",
};

/** CSS custom property of a syntax role: `string.special` → `--cm-syn-string-special`. */
export const synVar = (role: string): string => `--cm-syn-${role.replaceAll(".", "-")}`;

/** Every CSS var a theme can set, for tests and for clearing a previous theme. */
export function allVars(): string[] {
	return [
		...Object.values(UI_TOKENS).flat(),
		...Object.values(EDITOR_TOKENS),
		...Object.values(PLAYER_TOKENS),
		...SYNTAX_ROLES.flatMap((r) => [synVar(r), `${synVar(r)}-italic`, `${synVar(r)}-weight`]),
	];
}
