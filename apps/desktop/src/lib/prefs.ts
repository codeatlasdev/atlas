/**
 * Editor/terminal/panel preferences under Zed's setting names in settings.json
 * (`buffer_font_*`, `ui_font_*`, `editor.*`, `terminal.*`, `project_panel.*`,
 * `active_pane_modifiers`) plus `lsp` (language server overrides per language).
 * Pure: `resolvePrefs` fills defaults and drops bad values.
 */
import { ZOOM_LEVELS } from "#lib/zoom.ts";

/** User override of the language server for one LSP language id (`typescript`, `rust`…). */
export interface LspLanguagePref {
	/** Run this program instead of the one Atlas finds or installs. */
	command?: string;
	args?: string[];
	initialization_options?: Record<string, unknown>;
	settings?: Record<string, unknown>;
	/** No language server for this language. */
	disabled?: boolean;
}

export interface EditorPrefs {
	tab_size: number;
	insert_spaces: boolean;
	word_wrap: boolean;
	render_whitespace: "none" | "selection" | "all";
	line_numbers: boolean;
	minimap: boolean;
	sticky_scroll: boolean;
	bracket_pair_colorization: boolean;
	indent_guides: boolean;
	/** Columns where a vertical ruler is drawn. */
	rulers: number[];
	highlight_active_line: boolean;
	trim_trailing_whitespace_on_save: boolean;
	insert_final_newline_on_save: boolean;
	/** Files bigger than this open in a light mode (no language features). */
	large_file_threshold_bytes: number;
	scroll_past_end: boolean;
	smooth_scrolling: boolean;
	/** Reload a file changed on disk when it has no unsaved edits. */
	auto_reload: boolean;
}

export interface Prefs {
	buffer_font_family: string;
	buffer_font_size: number;
	buffer_line_height: number;
	ui_font_family: string;
	ui_font_size: number;
	/** Layout zoom (1 = 100 %), stepped by ⌘+ ⌘− ⌘0 (`zoom.ts`). */
	zoom: number;
	/** Language servers (completion, diagnostics, go to definition…) in the editor. */
	enable_language_servers: boolean;
	/** Format with the language server when saving. */
	format_on_save: boolean;
	inlay_hints: { enabled: boolean };
	/** Per-language server overrides, keyed by LSP language id. */
	lsp: Record<string, LspLanguagePref>;
	editor: EditorPrefs;
	terminal: {
		/** CSS font list, used by both renderers. The GPU terminal takes the first installed family (variable fonts included), else its bundled JetBrains Mono. */
		font_family: string;
		/** GPU terminal: heavier glyph rendering (Ghostty `font-thicken`); xterm uses weights 500/700. */
		font_thicken: boolean;
		font_size: number;
		cursor_shape: "block" | "bar" | "underline" | "hollow";
		blinking: boolean;
		max_scroll_history_lines: number;
		/** Program writes to the clipboard (OSC 52). */
		osc52: "allow" | "ask" | "deny";
		/** GPU terminal: programming ligatures (`->`, `!=`). */
		ligatures: boolean;
		/** GPU terminal: WCAG contrast ratio text is lifted to (1 = off). */
		minimum_contrast: number;
		/** Set when the user picked `minimum_contrast` themselves: a stored 1 without it is the pre-4.5 default. */
		minimum_contrast_chosen: boolean;
		/** GPU terminal: copy the selection to the clipboard on release (Ghostty `copy-on-select`). */
		copy_on_select: boolean;
		/** GPU terminal: ⌥ acts as Alt (⌥B = ESC b) instead of typing macOS characters (Ghostty `macos-option-as-alt`). */
		option_as_alt: "both" | "left" | "right" | "none";
		/** GPU terminal: a click on the prompt moves the cursor there (needs shell integration; Ghostty `cursor-click-to-move`). */
		click_to_move: boolean;
		/** Terminal inset in pt (Ghostty `window-padding-x/y`); the margin is painted with the terminal background. */
		padding_x: number;
		padding_y: number;
	};
	project_panel: {
		dock: "left" | "right";
		file_icons: boolean;
		folder_icons: boolean;
		indent_size: number;
	};
	active_pane_modifiers: { border_size: number; inactive_opacity: number };
	/** Changes tab: one column (`unified`) or old and new next to each other (`split`). */
	diff: { view: "unified" | "split" };
}

export const DEFAULT_PREFS: Prefs = {
	buffer_font_family: 'ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace',
	buffer_font_size: 12.5,
	buffer_line_height: 1.55,
	ui_font_family: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
	ui_font_size: 13,
	zoom: 1,
	enable_language_servers: true,
	format_on_save: false,
	inlay_hints: { enabled: true },
	lsp: {},
	editor: {
		tab_size: 4,
		insert_spaces: true,
		word_wrap: false,
		render_whitespace: "none",
		line_numbers: true,
		minimap: false,
		sticky_scroll: true,
		bracket_pair_colorization: true,
		indent_guides: true,
		rulers: [],
		highlight_active_line: true,
		trim_trailing_whitespace_on_save: true,
		insert_final_newline_on_save: true,
		large_file_threshold_bytes: 2_000_000,
		scroll_past_end: true,
		smooth_scrolling: true,
		auto_reload: true,
	},
	terminal: {
		font_family: "SF Mono, Menlo, ui-monospace, monospace",
		font_thicken: false,
		font_size: 12.5,
		cursor_shape: "block",
		blinking: false,
		max_scroll_history_lines: 10_000,
		osc52: "ask",
		ligatures: true,
		minimum_contrast: 4.5,
		minimum_contrast_chosen: false,
		copy_on_select: true,
		option_as_alt: "both",
		click_to_move: true,
		padding_x: 10,
		padding_y: 6,
	},
	project_panel: { dock: "left", file_icons: true, folder_icons: true, indent_size: 12 },
	active_pane_modifiers: { border_size: 0, inactive_opacity: 1 },
	diff: { view: "unified" },
};

type Shape = Record<string, unknown>;
const isObj = (v: unknown): v is Shape => !!v && typeof v === "object" && !Array.isArray(v);

/** Same shape as `def`, taking values from `raw` only where the type matches. Array defaults are left to `sanitize*`. */
function fill<T>(def: T, raw: unknown, check: (key: string, v: unknown) => boolean = () => true): T {
	if (!raw || typeof raw !== "object") return structuredClone(def);
	const out = {} as Shape;
	for (const [k, d] of Object.entries(def as Shape)) {
		const v = (raw as Shape)[k];
		if (Array.isArray(d)) out[k] = structuredClone(d);
		else if (d && typeof d === "object") out[k] = fill(d, v, (sk, sv) => check(`${k}.${sk}`, sv));
		else out[k] = typeof v === typeof d && check(k, v) ? v : d;
	}
	return out as T;
}

const ENUMS: Record<string, readonly string[]> = {
	"editor.render_whitespace": ["none", "selection", "all"],
	"terminal.cursor_shape": ["block", "bar", "underline", "hollow"],
	"terminal.osc52": ["allow", "ask", "deny"],
	"terminal.option_as_alt": ["both", "left", "right", "none"],
	"project_panel.dock": ["left", "right"],
	"diff.view": ["unified", "split"],
};
const RANGES: Record<string, [number, number]> = {
	buffer_font_size: [6, 48],
	buffer_line_height: [1, 3],
	ui_font_size: [9, 24],
	zoom: [ZOOM_LEVELS[0], ZOOM_LEVELS[ZOOM_LEVELS.length - 1]],
	"terminal.font_size": [6, 48],
	"terminal.minimum_contrast": [1, 21],
	"terminal.padding_x": [0, 64],
	"terminal.padding_y": [0, 64],
	"terminal.max_scroll_history_lines": [0, 100_000],
	"project_panel.indent_size": [4, 40],
	"active_pane_modifiers.border_size": [0, 8],
	"active_pane_modifiers.inactive_opacity": [0.2, 1],
	"editor.tab_size": [1, 16],
	"editor.large_file_threshold_bytes": [100_000, 100_000_000],
};

/** Largest ruler column accepted; anything beyond is not a real line length. */
const MAX_RULER = 500;

/** Whole columns 1..500, in ascending order without duplicates; anything else is dropped. */
export function sanitizeRulers(raw: unknown): number[] {
	if (!Array.isArray(raw)) return [];
	const cols = raw.filter((n): n is number => Number.isInteger(n) && n >= 1 && n <= MAX_RULER);
	return [...new Set(cols)].sort((a, b) => a - b);
}

const isStringList = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === "string");

/**
 * One `lsp.<language>` entry. Fields of the wrong type are dropped, not the whole entry,
 * and an entry that ends up empty is omitted.
 */
function sanitizeLspLanguage(raw: unknown): LspLanguagePref | null {
	if (!isObj(raw)) return null;
	const out: LspLanguagePref = {};
	if (typeof raw.command === "string" && raw.command.trim() !== "") out.command = raw.command.trim();
	if (isStringList(raw.args)) out.args = raw.args;
	if (isObj(raw.initialization_options)) out.initialization_options = raw.initialization_options;
	if (isObj(raw.settings)) out.settings = raw.settings;
	if (typeof raw.disabled === "boolean") out.disabled = raw.disabled;
	return Object.keys(out).length > 0 ? out : null;
}

/** The `lsp` map: keys are LSP language ids; non-object entries and empty ones are dropped. */
export function sanitizeLsp(raw: unknown): Record<string, LspLanguagePref> {
	if (!isObj(raw)) return {};
	const out: Record<string, LspLanguagePref> = {};
	for (const [lang, entry] of Object.entries(raw)) {
		const e = sanitizeLspLanguage(entry);
		if (e) out[lang] = e;
	}
	return out;
}

export function resolvePrefs(settings: Record<string, unknown>): Prefs {
	const p = fill(DEFAULT_PREFS, settings, (k, v) => {
		if (ENUMS[k]) return ENUMS[k].includes(v as string);
		const r = RANGES[k];
		if (r) return Number.isFinite(v) && (v as number) >= r[0] && (v as number) <= r[1];
		return typeof v !== "string" || v.trim() !== "";
	});
	const raw = isObj(settings) ? settings : {};
	// the old default (1 = off) was written back whole by the Terminal tab: without the marker it is not a choice
	if (p.terminal.minimum_contrast === 1 && !p.terminal.minimum_contrast_chosen)
		p.terminal.minimum_contrast = DEFAULT_PREFS.terminal.minimum_contrast;
	p.lsp = sanitizeLsp(raw.lsp);
	p.editor.rulers = sanitizeRulers(isObj(raw.editor) ? raw.editor.rulers : undefined);
	return p;
}
