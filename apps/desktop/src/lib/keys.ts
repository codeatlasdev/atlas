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
	// Language server (the focused editor): F-keys as in Zed/VS Code
	f12: "editor::GoToDefinition",
	"ctrl--": "editor::GoBack",
	"ctrl-shift--": "editor::GoForward",
	"alt-f12": "editor::GoToTypeDefinition",
	"cmd-f12": "editor::GoToImplementation",
	"shift-f12": "editor::FindAllReferences",
	f2: "editor::Rename",
	"alt-shift-f": "editor::Format",
	"cmd-.": "editor::ToggleCodeActions",
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
	"cmd-shift-v": "atlas::PreviewHtml",
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

/**
 * macOS: ⌘ is the primary modifier. Elsewhere the Windows/Super key (Meta) belongs to the system, so Ctrl takes
 * ⌘'s place, as in VS Code and Zed: the keymap keeps its `cmd-…` spelling everywhere.
 */
export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/**
 * Off macOS ⌃ and ⌘ are both Ctrl, so a default written with ⌃ or ⌃⌘ that shares its key with a ⌘ one could never
 * run (⌃⌘S sidebar vs ⌘S save; ⌃1 space, ⌘1 tab, ⌃⌘1 recent worktree). Those move to Alt (⌃) and Alt+Shift (⌃⌘);
 * never to Ctrl+Alt, which is AltGr on Windows. The others keep their macOS spelling (`cmd` reads as Ctrl).
 */
const OFF_MAC: Record<string, string> = {
	"ctrl-cmd-s": "alt-shift-s",
	...Object.fromEntries([1, 2, 3, 4, 5, 6].map((n) => [`ctrl-cmd-${n}`, `alt-shift-${n}`])),
	...Object.fromEntries([1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => [`ctrl-${n}`, `alt-${n}`])),
};

/** The default keymap on this platform: [`DEFAULT_KEYMAP`] on macOS, with [`OFF_MAC`]'s moves elsewhere. */
export function defaultKeymap(mac = IS_MAC): Record<string, string> {
	if (mac) return DEFAULT_KEYMAP;
	return Object.fromEntries(Object.entries(DEFAULT_KEYMAP).map(([k, v]) => [OFF_MAC[k] ?? k, v]));
}

/** The modifier `cmd-…` bindings mean on this platform is held: ⌘ on macOS, Ctrl elsewhere. */
export const primaryKey = (e: { metaKey: boolean; ctrlKey: boolean }, mac = IS_MAC) =>
	mac ? e.metaKey : e.ctrlKey;

/** The other of ⌘/Ctrl: Ctrl on macOS, the Windows key elsewhere (for "no other modifier" checks). */
export const otherKey = (e: { metaKey: boolean; ctrlKey: boolean }, mac = IS_MAC) =>
	mac ? e.ctrlKey : e.metaKey;

/**
 * Off macOS the app's shortcuts are Ctrl chords, which a shell needs (^C, ^R, ^L, ^W …): in a focused terminal a
 * plain Ctrl+letter goes to the program. Ctrl+Shift chords (the palette, Ctrl+Shift+P) and Ctrl with a digit, Tab
 * or punctuation still reach the app. macOS shortcuts use ⌘, which no program reads.
 */
export function shellKeepsCtrl(
	e: { ctrlKey: boolean; shiftKey: boolean; altKey: boolean; code: string },
	mac = IS_MAC,
) {
	return !mac && e.ctrlKey && !e.shiftKey && !e.altKey && /^Key[A-Z]$/.test(e.code);
}

function keyOf(e: KeyboardEvent): string | undefined {
	const c = e.code;
	return /^Key[A-Z]$/.test(c)
		? c.slice(3).toLowerCase()
		: /^Digit\d$/.test(c)
			? c.slice(5)
			: /^F\d+$/.test(c)
				? c.toLowerCase()
				: CODE_KEYS[c];
}

/**
 * The physical key (layout-independent, like menu shortcuts) with modifiers. Off macOS Ctrl reads as `cmd` (see
 * [`IS_MAC`]) and a chord with the Windows key is the system's (`null`).
 */
export function strokeOf(e: KeyboardEvent, mac = IS_MAC): string | null {
	return strokesOf(e, mac)[0] ?? null;
}

/**
 * Every binding a key event can mean, in precedence order. macOS: the one stroke. Elsewhere Ctrl is ⌘ first
 * (`cmd-k`), then itself (`ctrl-tab`, `ctrl-\``), so the bindings written with ⌃ for macOS stay reachable.
 */
export function strokesOf(e: KeyboardEvent, mac = IS_MAC): string[] {
	const key = keyOf(e);
	if (!key) return [];
	const join = (mods: (string | false)[]) => [...mods.filter(Boolean), key].join("-");
	if (mac) return [join([e.ctrlKey && "ctrl", e.altKey && "alt", e.shiftKey && "shift", e.metaKey && "cmd"])];
	if (e.metaKey) return [];
	// AltGr arrives as Ctrl+Alt on Windows: a chord that types a character (ś, ², @) is text, not a shortcut.
	const typed = typeof e.key === "string" && e.key.length === 1 && e.key.toLowerCase() !== key;
	if (e.ctrlKey && e.altKey && /^(Key|Digit)/.test(e.code) && typed) return [];
	const primary = join([e.altKey && "alt", e.shiftKey && "shift", e.ctrlKey && "cmd"]);
	const literal = join([e.ctrlKey && "ctrl", e.altKey && "alt", e.shiftKey && "shift"]);
	return primary === literal ? [primary] : [primary, literal];
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

/** Key names off macOS (`Ctrl+Shift+P`, as Windows and Linux write them). */
const WORD: Record<string, string> = {
	escape: "Esc",
	enter: "Enter",
	tab: "Tab",
	backspace: "Backspace",
	delete: "Delete",
	space: "Space",
	up: "↑",
	down: "↓",
	left: "←",
	right: "→",
	home: "Home",
	end: "End",
	pageup: "PgUp",
	pagedown: "PgDn",
};

/** Label for a canonical keystroke: macOS glyphs (`⇧⌘P`), or `Ctrl+Shift+P` elsewhere (⌘ and ⌃ are both Ctrl). */
export function label(stroke: string, mac = IS_MAC): string {
	const parts = stroke.split("-");
	if (stroke.endsWith("--")) parts.splice(-2, 2, "-");
	if (mac) return parts.map((p) => GLYPH[p] ?? p.toUpperCase()).join("");
	const key = parts.pop() ?? "";
	const mods = [
		(parts.includes("ctrl") || parts.includes("cmd")) && "Ctrl",
		parts.includes("alt") && "Alt",
		parts.includes("shift") && "Shift",
	].filter(Boolean);
	return [...mods, WORD[key] ?? key.toUpperCase()].join("+");
}

/** A key named in UI text (`kbd("cmd-enter")`: ⌘↩ on macOS, Ctrl+Enter elsewhere), spelled like [`label`]. */
export function kbd(stroke: string, mac = IS_MAC): string {
	return label(normalize(stroke) ?? stroke, mac);
}

/** "⌘-click" on macOS, "Ctrl+click" elsewhere (link clicks read [`primaryKey`]). */
export const modClick = (mac = IS_MAC) => (mac ? "⌘-click" : "Ctrl+click");

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
