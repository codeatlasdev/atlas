/**
 * Settings ▸ Capture, pure logic. Rust (`shots_prefs_set`) stays the authority on every rule here; the client-side
 * checks only give instant feedback before the round trip.
 */
import { dockKeyFrom, type KeyRecording } from "#lib/dockKey.ts";
import { IS_MAC, kbd, normalize } from "#lib/keys.ts";
import type { CapturePrefs, Entry, Keys, Preset, Verdict } from "./types.ts";

/** Every capture action with a hotkey, in the order the page lists them (and `clashes` reports the first duplicate). */
export const KEY_FIELDS: readonly { field: keyof Keys; label: string; description: string }[] = [
	{
		field: "allInOne",
		label: "All-in-One",
		description: "One toolbar over the screen: pick region, window, screen, record, GIF or text.",
	},
	{
		field: "region",
		label: "Region",
		description: "Drag to pick an area, or press Space to switch to picking a window.",
	},
	{
		field: "previous",
		label: "Previous area",
		description: "Capture the same area as your last region capture, right now.",
	},
	{ field: "window", label: "Window", description: "Click a window to capture it." },
	{
		field: "fullscreen",
		label: "Screen",
		description: "Capture the whole screen under the pointer.",
	},
	{
		field: "ocr",
		label: "Text",
		description: "Drag over text on screen and copy it, read on this Mac.",
	},
	{
		field: "record",
		label: "Record",
		description: "Pick an area and record a video. Press the key again to stop.",
	},
	{ field: "gif", label: "GIF", description: "Pick an area and record an animated GIF." },
];

/** A key press while a row is recording: a stroke Rust accepts, a cancel, a wait, or a refusal with the reason. */
export function recordingResult(e: KeyboardEvent, mac: boolean): KeyRecording {
	return dockKeyFrom(e, mac);
}

/** `⇧⌘S` (macOS) or `Ctrl+Shift+S`; an empty stroke reads "Not set". */
export function keyLabel(stroke: string, mac = IS_MAC): string {
	return stroke.trim() ? kbd(stroke.trim(), mac) : "Not set";
}

/**
 * The same hotkey, whatever the order, case or alias of its modifiers (`shift-cmd-s` == `cmd-shift-s`).
 * An empty stroke is "not set" and equals nothing, not even another empty one.
 */
export function sameStroke(a: string, b: string): boolean {
	if (!a.trim() || !b.trim()) return false;
	const na = normalize(a);
	const nb = normalize(b);
	return na !== null && nb !== null ? na === nb : a.trim().toLowerCase() === b.trim().toLowerCase();
}

/** The self-timer choices; Rust accepts exactly these seconds (`prefs::TIMER_VALUES`). */
export const TIMER_OPTIONS = [
	{ value: "0", label: "Off" },
	{ value: "3", label: "3 seconds" },
	{ value: "5", label: "5 seconds" },
	{ value: "10", label: "10 seconds" },
] as const;

export const SCALE_OPTIONS = [
	{ value: "native", label: "Retina (2x on a Retina display)" },
	{ value: "1x", label: "Standard (1x)" },
] as const;

export const CORNER_OPTIONS = [
	{ value: "bottomRight", label: "Bottom right" },
	{ value: "bottomLeft", label: "Bottom left" },
	{ value: "topRight", label: "Top right" },
	{ value: "topLeft", label: "Top left" },
] as const;

/** Recording quality: Rust's `VideoQuality` words (`prefs.rs`). Low and Medium cap the pixels, Low also the frame rate. */
export const QUALITY_OPTIONS = [
	{ value: "high", label: "High (the display's own pixels)" },
	{ value: "medium", label: "Medium (up to 1920 px)" },
	{ value: "low", label: "Low (up to 1280 px, 24 fps)" },
] as const;

/** The four strokes macOS keeps for its own screenshots, as the page shows them. */
const SYSTEM_STROKES: readonly [string, string][] = [
	["cmd-shift-3", "⇧⌘3"],
	["cmd-shift-4", "⇧⌘4"],
	["cmd-shift-5", "⇧⌘5"],
	["cmd-shift-6", "⇧⌘6"],
];

/** Which of macOS's own screenshot keys (⇧⌘3–6) `keys` uses, as symbols. */
export function systemKeysUsed(keys: Keys): string[] {
	return SYSTEM_STROKES.filter(([stroke]) =>
		KEY_FIELDS.some(({ field }) => sameStroke(keys[field], stroke)),
	).map(([, label]) => label);
}

/**
 * A preset can be applied when it does not use macOS's screenshot keys, or when the person said they turned those
 * off (Atlas never changes that system setting, so it cannot know on its own).
 */
export function presetAvailable(preset: Preset, prefs: Pick<CapturePrefs, "systemKeysFree">): boolean {
	return !preset.needsSystemKeys || prefs.systemKeysFree;
}

/** The prefs with the preset's keys and nothing else changed. */
export function applyPreset(prefs: CapturePrefs, preset: Preset): CapturePrefs {
	return { ...prefs, keys: { ...preset.keys } };
}

/** The preset whose keys equal `keys` (order-insensitive per stroke), or `null` for a custom set. */
export function activePresetId(keys: Keys, presets: readonly Preset[]): string | null {
	if (KEY_FIELDS.every(({ field }) => !keys[field].trim())) return null;
	return (
		presets.find((p) =>
			KEY_FIELDS.every(({ field }) =>
				keys[field].trim() === "" ? p.keys[field].trim() === "" : sameStroke(keys[field], p.keys[field]),
			),
		)?.id ?? null
	);
}

/** The line shown under a hotkey for a `hotkey_check` verdict; empty when there is nothing to say. */
export function verdictText(v: Verdict | null): string {
	return v && v.kind !== "ok" ? v.reason : "";
}

/**
 * The sentence for the first two capture keys that are the same stroke, or a capture key that is also one of the
 * dock's; `null` when there is none. Empty strokes never clash.
 */
export function clashes(keys: Keys, dockKeys: readonly string[]): string | null {
	for (const [i, a] of KEY_FIELDS.entries()) {
		for (const b of KEY_FIELDS.slice(i + 1))
			if (sameStroke(keys[a.field], keys[b.field]))
				return `${a.label} and ${b.label} use the same key (${keyLabel(keys[a.field])}). Give one of them another.`;
		if (dockKeys.some((d) => sameStroke(keys[a.field], d)))
			return `${a.label} uses ${keyLabel(keys[a.field])}, which the agent dock already uses. Pick another key for one of them.`;
	}
	return null;
}

export interface HistoryGroup {
	title: "Today" | "Yesterday" | "This week" | "Earlier";
	entries: Entry[];
}

/**
 * History grouped by LOCAL calendar days: Today, Yesterday, This week (the seven days before today) and Earlier,
 * newest first inside each, empty groups omitted. Boundaries come from `new Date(y, m, d - n)`, never from
 * "now - 24 h", so a 23 h or 25 h day (DST) cannot shift an entry into the wrong group. A timestamp in the future
 * (clock skew) counts as Today.
 */
export function historyGroups(entries: readonly Entry[], now: number): HistoryGroup[] {
	const d = new Date(Number.isFinite(now) ? now : Date.now());
	const day = (back: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() - back).getTime();
	const [today, yesterday, week] = [day(0), day(1), day(7)];
	const groups: HistoryGroup[] = [
		{ title: "Today", entries: [] },
		{ title: "Yesterday", entries: [] },
		{ title: "This week", entries: [] },
		{ title: "Earlier", entries: [] },
	];
	for (const e of [...entries].sort((a, b) => b.at - a.at)) {
		const i = !(e.at < today) ? 0 : e.at >= yesterday ? 1 : e.at >= week ? 2 : 3;
		groups[i].entries.push(e);
	}
	return groups.filter((g) => g.entries.length);
}

const KIND_LABEL: Record<Entry["kind"], string> = {
	screenshot: "Screenshot",
	recording: "Recording",
	gif: "GIF",
};

function sizeText(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
	return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function durationText(secs: number): string {
	const s = Math.max(0, Math.round(secs));
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** What a history row shows: the file name, the kind, the size and a one-line detail (pixels, duration, size). */
export function historyLabel(e: Entry): { name: string; kind: string; size: string; detail: string } {
	const size = sizeText(e.bytes);
	return {
		name: e.path.split(/[\\/]/).pop() ?? e.path,
		kind: KIND_LABEL[e.kind],
		size,
		detail: [`${e.width} × ${e.height}`, e.secs === null ? null : durationText(e.secs), size]
			.filter(Boolean)
			.join(" · "),
	};
}
