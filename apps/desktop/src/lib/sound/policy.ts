/**
 * When a sound may play. Pure: no Web Audio, no Tauri, no clock of its own, so every rule is testable.
 *
 * The rules come from the research in studio notes (Apple HIG "Playing audio": people who silence sounds
 * do not want unexpected ones; the system volume governs; never sound as the only channel):
 *
 *  1. Master switch / volume 0 silence everything, including attention sounds.
 *  2. macOS "Play user interface sound effects" off silences everything the user did not ask for.
 *  3. A user-initiated preview (Settings → Sounds → Play) ignores 2 and cooldowns, never 1's volume 0.
 *  4. In the background only notification categories play (the window is not what the user is looking at).
 *  5. "Reduce sounds" keeps attention categories only, quieter.
 *  6. Per-category switch, then a per-category cooldown so bursts collapse into one sound.
 */

/** One earcon family member per meaningful event. Ids are also the asset file names. */
export const CATEGORIES = [
	"boot",
	"discovery-done",
	"stack-up",
	"service-failed",
	"agent-needs-you",
	"agent-done",
	"update",
	"error",
	"success",
	"toggle",
] as const;

export type Category = (typeof CATEGORIES)[number];

interface Spec {
	/** Mix level relative to full scale after the assets were normalised (louder = more urgent). */
	level: number;
	/** Minimum ms between two plays of the category. */
	cooldownMs: number;
	/** Plays while the window is in the background (a notification, not ambient feedback). */
	background: boolean;
	/** Still plays under "Reduce sounds". */
	attention: boolean;
	/** Short label for Settings. */
	label: string;
	description: string;
}

const MIN = 60_000;

export const SPECS: Record<Category, Spec> = {
	boot: {
		level: 0.85,
		cooldownMs: 60 * MIN,
		background: false,
		attention: false,
		label: "App opens",
		description: "A short signature when Atlas starts, at most once an hour.",
	},
	"discovery-done": {
		level: 0.8,
		cooldownMs: 2_000,
		background: false,
		attention: false,
		label: "Projects found",
		description: "When “Find my projects” finishes.",
	},
	"stack-up": {
		level: 0.85,
		cooldownMs: 3_000,
		background: false,
		attention: false,
		label: "Stack is up",
		description: "When every service of a stack is healthy.",
	},
	"service-failed": {
		level: 1,
		cooldownMs: 8_000,
		background: true,
		attention: true,
		label: "Service failed",
		description: "A service crashed or could not start.",
	},
	"agent-needs-you": {
		level: 0.95,
		cooldownMs: 4_000,
		background: true,
		attention: true,
		label: "Agent needs you",
		description: "An agent is waiting for an answer or approval.",
	},
	"agent-done": {
		level: 0.85,
		cooldownMs: 4_000,
		background: true,
		attention: true,
		label: "Agent is done",
		description: "An agent finished its task.",
	},
	update: {
		level: 0.8,
		cooldownMs: 10 * MIN,
		background: true,
		attention: true,
		label: "Update ready",
		description: "A new Atlas version is available.",
	},
	error: {
		level: 0.7,
		cooldownMs: 3_000,
		background: false,
		attention: false,
		label: "Errors",
		description: "A soft tick when an action fails.",
	},
	success: {
		level: 0.55,
		cooldownMs: 1_500,
		background: false,
		attention: false,
		label: "Copied and saved",
		description: "A very quiet confirmation.",
	},
	toggle: {
		level: 0.5,
		cooldownMs: 120,
		background: false,
		attention: false,
		label: "Switches",
		description: "When you turn a meaningful switch on or off.",
	},
};

export interface SoundPrefs {
	enabled: boolean;
	/** 0..1, applied on top of the system volume (which always governs the final output). */
	volume: number;
	/** Only attention categories, quieter. */
	reduce: boolean;
	categories: Record<Category, boolean>;
}

export const DEFAULT_SOUND_PREFS: SoundPrefs = {
	enabled: true,
	volume: 0.4,
	reduce: false,
	categories: Object.fromEntries(CATEGORIES.map((c) => [c, true])) as Record<Category, boolean>,
};

/** Reduced sounds play at this fraction of the normal level. */
const REDUCE_FACTOR = 0.5;

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);

/** Whatever settings.json holds under `sounds`, as valid prefs (bad values fall back per field). */
export function parseSoundPrefs(raw: unknown): SoundPrefs {
	const d = DEFAULT_SOUND_PREFS;
	if (!isObj(raw)) return structuredClone(d);
	const cats = isObj(raw.categories) ? raw.categories : {};
	return {
		enabled: typeof raw.enabled === "boolean" ? raw.enabled : d.enabled,
		volume:
			typeof raw.volume === "number" && Number.isFinite(raw.volume)
				? Math.min(1, Math.max(0, raw.volume))
				: d.volume,
		reduce: typeof raw.reduce === "boolean" ? raw.reduce : d.reduce,
		categories: Object.fromEntries(
			CATEGORIES.map((c) => [c, typeof cats[c] === "boolean" ? cats[c] : d.categories[c]]),
		) as Record<Category, boolean>,
	};
}

/** Everything `decide` needs from the outside world. */
export interface Ctx {
	now: number;
	/** The window is focused/visible. */
	foreground: boolean;
	/** macOS "Play user interface sound effects" (true when the preference is absent). */
	systemUiSounds: boolean;
	prefs: SoundPrefs;
	/** `prefers-reduced-motion`; never silences sound on its own, kept so callers pass one context. */
	reducedMotion: boolean;
}

/** When each category last played (ms). Mutated by `record`, read by `decide`. */
export interface Memory {
	last: Partial<Record<Category, number>>;
}

export const newMemory = (): Memory => ({ last: {} });

/** Categories whose cooldown outlives the process: only the boot sound, its window is an hour. */
export const PERSISTED: readonly Category[] = ["boot"];

/** Memory restored from what the app persisted (`{ boot: <ms> }`); unknown keys and non-numbers are dropped. */
export function seedMemory(raw: Record<string, unknown>): Memory {
	const m = newMemory();
	for (const c of PERSISTED) {
		const v = raw[c];
		if (typeof v === "number" && Number.isFinite(v) && v > 0) m.last[c] = v;
	}
	return m;
}

export type Why =
	| "ok"
	| "preview"
	| "disabled"
	| "muted"
	| "system-ui-sounds-off"
	| "category-off"
	| "background"
	| "reduced"
	| "cooldown";

export interface Decision {
	play: boolean;
	why: Why;
}

/** Linear gain for `category` under `prefs` (volume × mix level, halved when reduced). */
export function gain(category: Category, prefs: SoundPrefs): number {
	const g = prefs.volume * SPECS[category].level * (prefs.reduce ? REDUCE_FACTOR : 1);
	return Math.min(1, Math.max(0, g));
}

/** Should `category` sound now? `preview` is a user pressing "Play" in Settings. */
export function decide(
	category: Category,
	ctx: Ctx,
	memory: Memory,
	opts: { preview?: boolean } = {},
): Decision {
	const no = (why: Why): Decision => ({ play: false, why });
	const { prefs } = ctx;
	if (opts.preview) return prefs.volume > 0 ? { play: true, why: "preview" } : no("muted");
	if (!prefs.enabled) return no("disabled");
	if (prefs.volume <= 0) return no("muted");
	if (!ctx.systemUiSounds) return no("system-ui-sounds-off");
	if (!prefs.categories[category]) return no("category-off");
	const spec = SPECS[category];
	if (!ctx.foreground && !spec.background) return no("background");
	if (prefs.reduce && !spec.attention) return no("reduced");
	const last = memory.last[category];
	// A clock that went backwards (`since < 0`) must not lock the category until it catches up.
	if (last !== undefined) {
		const since = ctx.now - last;
		if (since >= 0 && since < spec.cooldownMs) return no("cooldown");
	}
	return { play: true, why: "ok" };
}

/** Note that `category` played at `now` (call only for sounds that actually played). */
export function record(category: Category, now: number, memory: Memory): void {
	memory.last[category] = now;
}
