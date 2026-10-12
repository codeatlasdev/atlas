/**
 * Editor/terminal/panel preferences under Zed's setting names in settings.json
 * (`buffer_font_*`, `ui_font_*`, `terminal.*`, `project_panel.*`,
 * `active_pane_modifiers`). Pure: `resolvePrefs` fills defaults and drops bad values.
 */

export interface Prefs {
	buffer_font_family: string;
	buffer_font_size: number;
	buffer_line_height: number;
	ui_font_family: string;
	ui_font_size: number;
	terminal: {
		/** xterm.js only; the GPU terminal uses its bundled JetBrains Mono. */
		font_family: string;
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
		/** GPU terminal: copy the selection to the clipboard on release (Ghostty `copy-on-select`). */
		copy_on_select: boolean;
		/** GPU terminal: ⌥ acts as Alt (⌥B = ESC b) instead of typing macOS characters (Ghostty `macos-option-as-alt`). */
		option_as_alt: "both" | "left" | "right" | "none";
		/** GPU terminal: a click on the prompt moves the cursor there (needs shell integration; Ghostty `cursor-click-to-move`). */
		click_to_move: boolean;
	};
	project_panel: {
		dock: "left" | "right";
		file_icons: boolean;
		folder_icons: boolean;
		indent_size: number;
	};
	active_pane_modifiers: { border_size: number; inactive_opacity: number };
}

export const DEFAULT_PREFS: Prefs = {
	buffer_font_family: 'ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace',
	buffer_font_size: 12.5,
	buffer_line_height: 1.55,
	ui_font_family: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
	ui_font_size: 13,
	terminal: {
		font_family: "SF Mono, Menlo, ui-monospace, monospace",
		font_size: 12.5,
		cursor_shape: "block",
		blinking: false,
		max_scroll_history_lines: 10_000,
		osc52: "ask",
		ligatures: true,
		minimum_contrast: 1,
		copy_on_select: true,
		option_as_alt: "both",
		click_to_move: true,
	},
	project_panel: { dock: "left", file_icons: true, folder_icons: true, indent_size: 12 },
	active_pane_modifiers: { border_size: 0, inactive_opacity: 1 },
};

type Shape = Record<string, unknown>;

/** Same shape as `def`, taking values from `raw` only where the type matches. */
function fill<T>(def: T, raw: unknown, check: (key: string, v: unknown) => boolean = () => true): T {
	if (!raw || typeof raw !== "object") return structuredClone(def);
	const out = {} as Shape;
	for (const [k, d] of Object.entries(def as Shape)) {
		const v = (raw as Shape)[k];
		if (d && typeof d === "object") out[k] = fill(d, v, (sk, sv) => check(`${k}.${sk}`, sv));
		else out[k] = typeof v === typeof d && check(k, v) ? v : d;
	}
	return out as T;
}

const ENUMS: Record<string, readonly string[]> = {
	"terminal.cursor_shape": ["block", "bar", "underline", "hollow"],
	"terminal.osc52": ["allow", "ask", "deny"],
	"terminal.option_as_alt": ["both", "left", "right", "none"],
	"project_panel.dock": ["left", "right"],
};
const RANGES: Record<string, [number, number]> = {
	buffer_font_size: [6, 48],
	buffer_line_height: [1, 3],
	ui_font_size: [9, 24],
	"terminal.font_size": [6, 48],
	"terminal.minimum_contrast": [1, 21],
	"terminal.max_scroll_history_lines": [0, 100_000],
	"project_panel.indent_size": [4, 40],
	"active_pane_modifiers.border_size": [0, 8],
	"active_pane_modifiers.inactive_opacity": [0.2, 1],
};

export function resolvePrefs(settings: Record<string, unknown>): Prefs {
	return fill(DEFAULT_PREFS, settings, (k, v) => {
		if (ENUMS[k]) return ENUMS[k].includes(v as string);
		const r = RANGES[k];
		if (r) return Number.isFinite(v) && (v as number) >= r[0] && (v as number) <= r[1];
		return typeof v !== "string" || v.trim() !== "";
	});
}
