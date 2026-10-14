/**
 * Named commands + a configurable keymap (Zed keystroke syntax: `cmd-shift-p`,
 * `alt-cmd-y`, `ctrl-\``). Defaults follow Zed; `keymap` in settings.json overrides
 * them per keystroke (`null` unbinds). The command palette lists every command with
 * its current binding.
 */
import { api } from "#lib/api.ts";
import { bindingsOf, defaultKeymap, label, merge, rebind, strokesOf, unbind } from "#lib/keys.ts";

export interface Command {
	id: string;
	label: string;
	run: () => void;
	/** Hidden from the palette and inert while false. */
	when?: () => boolean;
}

const commands = new Map<string, Command>();

/** Register commands; returns an unregister function. */
export function register(list: Command[]): () => void {
	for (const c of list) commands.set(c.id, c);
	return () => list.forEach((c) => commands.get(c.id) === c && commands.delete(c.id));
}

/** Every registered command, including those whose `when` is currently false. */
export function registeredCommands(): Command[] {
	return [...commands.values()];
}

export function allCommands(): Command[] {
	return [...commands.values()].filter((c) => c.when?.() ?? true);
}

const RECENT_KEY = "atlas.recentCommands";
const RECENT_MAX = 5;

/** Most recently run command ids (palette shows them first), persisted per app. */
export const recent = $state<{ ids: string[] }>({
	ids: (() => {
		try {
			const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
			return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, RECENT_MAX) : [];
		} catch {
			return [];
		}
	})(),
});

/** Remember `id` as the latest command run from the palette or a key. */
export function noteRun(id: string) {
	recent.ids = [id, ...recent.ids.filter((x) => x !== id)].slice(0, RECENT_MAX);
	localStorage.setItem(RECENT_KEY, JSON.stringify(recent.ids));
}

/** Run a registered command by id (no-op when unknown or its `when` is false). */
export function runCommand(id: string) {
	const c = commands.get(id);
	if (c && (c.when?.() ?? true)) (noteRun(id), c.run());
}

/** User overrides from settings.json `keymap`. */
export const keymap = $state<{ user: Record<string, string | null> }>({ user: {} });

/** Adopt the `keymap` object of a settings document (anything else = no overrides). */
function adopt(settings: { keymap?: unknown }) {
	const k = settings.keymap;
	keymap.user = k && typeof k === "object" ? (k as Record<string, string | null>) : {};
}

export async function loadKeymap() {
	adopt(await api.settings());
}

async function saveKeymap(next: Record<string, string | null>) {
	adopt(await api.settingsPatch({ keymap: next }));
}

/** Bind `stroke` to `id`; resolves to the command that lost the key, if any. `null`: invalid stroke. */
export async function bindKey(id: string, stroke: string): Promise<{ displaced?: string } | null> {
	const r = rebind(defaultKeymap(), keymap.user, id, stroke);
	if (!r) return null;
	await saveKeymap(r.keymap);
	return { displaced: r.displaced };
}

export const unbindKey = (id: string) => saveKeymap(unbind(defaultKeymap(), keymap.user, id));

/** Drop every override: back to the default keymap. */
export const resetKeymap = () => saveKeymap({});

/** Stroke labels bound to a command (`["⌘B"]`). */
export function bindingLabels(id: string): string[] {
	return bindingsOf(id, merge(defaultKeymap(), keymap.user)).map((k) => label(k));
}

/** First binding of a command, as a label. */
export function shortcutFor(id: string): string | undefined {
	for (const [k, v] of merge(defaultKeymap(), keymap.user)) if (v === id) return label(k);
}

/**
 * Run the command bound to this key event. `true` = handled. `ownedByFocus` lets the focused
 * surface keep a chord (a GPU terminal claims ⇧⌘D for split-down like Ghostty, not "Changes").
 */
export function dispatch(e: KeyboardEvent, ownedByFocus?: (e: KeyboardEvent) => boolean): boolean {
	if (ownedByFocus?.(e)) return false;
	const effective = merge(defaultKeymap(), keymap.user);
	// Off macOS a Ctrl chord can mean a ⌘ binding or a ⌃ one (see keys.ts `strokesOf`): first that runs wins.
	for (const s of strokesOf(e)) {
		const id = effective.get(s);
		const c = id ? commands.get(id) : undefined;
		if (!c || !(c.when?.() ?? true)) continue;
		noteRun(c.id);
		c.run();
		return true;
	}
	return false;
}
