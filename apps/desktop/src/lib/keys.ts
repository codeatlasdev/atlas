/** Keystroke parsing and labels (Zed syntax), pure for tests. */

export const DEFAULT_KEYMAP: Record<string, string> = {
	// Files: ⌘S saves the focused editor (autosave is also on), ⌥⌘S every open one
	"cmd-s": "workspace::Save",
	"alt-cmd-s": "workspace::SaveAll",
	// Shell and docks
	"ctrl-cmd-s": "shell::ToggleSidebar",
	"cmd-b": "workspace::ToggleLeftDock",
	"cmd-r": "workspace::ToggleRightDock",
	"cmd-j": "workspace::ToggleBottomDock",
	"alt-cmd-y": "workspace::ToggleAllDocks",
	"shift-escape": "workspace::ToggleZoom",
	// Go anywhere
	"cmd-shift-p": "command_palette::Toggle",
	"cmd-k": "atlas::Jump",
	"cmd-p": "file_finder::Toggle",
	"cmd-shift-o": "outline::Toggle",
	"cmd-shift-f": "search::Toggle",
	"cmd-shift-e": "project_panel::ToggleFocus",
	"ctrl-shift-g": "git_panel::ToggleFocus",
	"ctrl-`": "terminal_panel::ToggleFocus",
	"cmd-?": "agent::Toggle",
	// Create
	"cmd-n": "atlas::NewWorktree",
	"cmd-t": "workspace::NewTerminal",
	"cmd-shift-a": "atlas::NewAgentSession",
	// Tabs: close, reopen, switch (⌘1…8 by position, ⌘9 last; ⌃Tab most recent)
	"cmd-w": "pane::CloseActiveItem",
	"alt-cmd-w": "pane::CloseOtherItems",
	"cmd-shift-w": "pane::CloseAllItems",
	"cmd-shift-t": "pane::ReopenClosedItem",
	"cmd-shift-]": "pane::ActivateNextItem",
	"cmd-shift-[": "pane::ActivatePreviousItem",
	"ctrl-tab": "pane::ActivateRecentItem",
	"ctrl-shift-tab": "pane::ActivateOlderRecentItem",
	"cmd-1": "pane::ActivateItem1",
	"cmd-2": "pane::ActivateItem2",
	"cmd-3": "pane::ActivateItem3",
	"cmd-4": "pane::ActivateItem4",
	"cmd-5": "pane::ActivateItem5",
	"cmd-6": "pane::ActivateItem6",
	"cmd-7": "pane::ActivateItem7",
	"cmd-8": "pane::ActivateItem8",
	"cmd-9": "pane::ActivateLastItem",
	"cmd-\\": "pane::SplitRight",
	"cmd-shift-\\": "pane::SplitDown",
	// Project
	"cmd-,": "zed::OpenSettings",
	"cmd-l": "atlas::Logs",
	"cmd-e": "atlas::Config",
	"cmd-shift-d": "git::Diff",
	"cmd-shift-r": "atlas::RestartService",
	"cmd-/": "atlas::Help",
	// Worktrees and spaces
	"ctrl-cmd-1": "atlas::RecentWorktree1",
	"ctrl-cmd-2": "atlas::RecentWorktree2",
	"ctrl-cmd-3": "atlas::RecentWorktree3",
	"ctrl-cmd-4": "atlas::RecentWorktree4",
	"ctrl-cmd-5": "atlas::RecentWorktree5",
	"ctrl-cmd-6": "atlas::RecentWorktree6",
	"cmd-[": "shell::Back",
	"cmd-]": "shell::Forward",
	"alt-cmd-left": "shell::PreviousSpace",
	"alt-cmd-right": "shell::NextSpace",
	"ctrl-1": "shell::Space1",
	"ctrl-2": "shell::Space2",
	"ctrl-3": "shell::Space3",
	"ctrl-4": "shell::Space4",
	"ctrl-5": "shell::Space5",
	"ctrl-6": "shell::Space6",
	"ctrl-7": "shell::Space7",
	"ctrl-8": "shell::Space8",
	"ctrl-9": "shell::Space9",
};

const MODS = ["ctrl", "alt", "shift", "cmd"] as const;
/** Characters typed with shift, written unshifted (`cmd-?` = `shift-cmd-/`). */
const SHIFTED: Record<string, string> = {
	"?": "/",
	"~": "`",
	"{": "[",
	"}": "]",
	"|": "\\",
	"<": ",",
	">": ".",
	_: "-",
	"+": "=",
	":": ";",
	'"': "'",
	"!": "1",
	"@": "2",
	"#": "3",
};
const ALIASES: Record<string, string> = {
	esc: "escape",
	return: "enter",
	option: "alt",
	super: "cmd",
	meta: "cmd",
};

/** Canonical form: modifiers in `ctrl-alt-shift-cmd` order, then the unshifted key. */
export function normalize(stroke: string): string | null {
	const parts = stroke.trim().toLowerCase().split("-");
	// `cmd--` binds the minus key.
	if (parts.length > 1 && parts[parts.length - 1] === "") parts.splice(-2, 2, "-");
	let key = parts.pop();
	if (!key) return null;
	const mods = new Set<string>();
	for (const p of parts) {
		const m = ALIASES[p] ?? p;
		if (!(MODS as readonly string[]).includes(m)) return null;
		mods.add(m);
	}
	key = ALIASES[key] ?? key;
	if (SHIFTED[key]) {
		mods.add("shift");
		key = SHIFTED[key];
	}
	return [...MODS.filter((m) => mods.has(m)), key].join("-");
}

const CODE_KEYS: Record<string, string> = {
	Backquote: "`",
	Backslash: "\\",
	BracketLeft: "[",
	BracketRight: "]",
	Comma: ",",
	Period: ".",
	Slash: "/",
	Minus: "-",
	Equal: "=",
	Semicolon: ";",
	Quote: "'",
	Escape: "escape",
	Enter: "enter",
	Tab: "tab",
	Backspace: "backspace",
	Delete: "delete",
	Space: "space",
	ArrowUp: "up",
	ArrowDown: "down",
	ArrowLeft: "left",
	ArrowRight: "right",
	Home: "home",
	End: "end",
	PageUp: "pageup",
	PageDown: "pagedown",
};

/** The physical key (layout-independent, like menu shortcuts) with modifiers. */
export function strokeOf(e: KeyboardEvent): string | null {
	const c = e.code;
	const key = /^Key[A-Z]$/.test(c)
		? c.slice(3).toLowerCase()
		: /^Digit\d$/.test(c)
			? c.slice(5)
			: /^F\d+$/.test(c)
				? c.toLowerCase()
				: CODE_KEYS[c];
	if (!key) return null;
	const mods = [e.ctrlKey && "ctrl", e.altKey && "alt", e.shiftKey && "shift", e.metaKey && "cmd"].filter(
		Boolean,
	);
	return [...mods, key].join("-");
}

const GLYPH: Record<string, string> = {
	ctrl: "⌃",
	alt: "⌥",
	shift: "⇧",
	cmd: "⌘",
	escape: "⎋",
	enter: "↩",
	tab: "⇥",
	backspace: "⌫",
	up: "↑",
	down: "↓",
	left: "←",
	right: "→",
	space: "␣",
};

/** macOS-style label for a canonical keystroke (`⌘⇧P`). */
export function label(stroke: string): string {
	const parts = stroke.split("-");
	if (stroke.endsWith("--")) parts.splice(-2, 2, "-");
	return parts.map((p) => GLYPH[p] ?? p.toUpperCase()).join("");
}

/** Effective keystroke → command: defaults, then user overrides (`null` unbinds). */
export function merge(
	defaults: Record<string, string>,
	user: Record<string, string | null>,
): Map<string, string> {
	const m = new Map<string, string>();
	for (const [k, v] of Object.entries(defaults)) {
		const n = normalize(k);
		if (n) m.set(n, v);
	}
	for (const [k, v] of Object.entries(user)) {
		const n = normalize(k);
		if (!n) continue;
		if (v) m.set(n, v);
		else m.delete(n);
	}
	return m;
}

/** Every stroke bound to `id` in the effective keymap, as labels (`⌘B`). */
export function bindingsOf(id: string, effective: Map<string, string>): string[] {
	return [...effective].filter(([, v]) => v === id).map(([k]) => k);
}

/**
 * New user `keymap` after binding `stroke` to `id`: the stroke's previous owner loses
 * it (`null`), and `id`'s other bindings are dropped so a command has one key.
 * Returns the patch to store and the command that was displaced (if any).
 */
export function rebind(
	defaults: Record<string, string>,
	user: Record<string, string | null>,
	id: string,
	stroke: string,
): { keymap: Record<string, string | null>; displaced?: string } | null {
	const key = normalize(stroke);
	if (!key) return null;
	const effective = merge(defaults, user);
	const displaced = effective.get(key);
	const next = withoutStrokes(user, [key, ...bindingsOf(id, effective)]);
	for (const old of bindingsOf(id, effective)) if (old !== key) next[old] = null;
	next[key] = id;
	return { keymap: next, displaced: displaced && displaced !== id ? displaced : undefined };
}

/** New user `keymap` with every binding of `id` removed (`null` overrides for defaults). */
export function unbind(
	defaults: Record<string, string>,
	user: Record<string, string | null>,
	id: string,
): Record<string, string | null> {
	const olds = bindingsOf(id, merge(defaults, user));
	const next = withoutStrokes(user, olds);
	for (const old of olds) next[old] = null;
	return next;
}

/** `user` minus every entry whose stroke normalizes to one of `strokes`. */
function withoutStrokes(
	user: Record<string, string | null>,
	strokes: string[],
): Record<string, string | null> {
	const out: Record<string, string | null> = {};
	for (const [k, v] of Object.entries(user)) {
		const n = normalize(k);
		if (n && strokes.includes(n)) continue;
		out[k] = v;
	}
	return out;
}
