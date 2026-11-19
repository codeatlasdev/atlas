/**
 * Settings ▸ Capture, pure logic. Rust (`shots_prefs_set`) stays the authority on every rule here; the client-side
 * checks only give instant feedback before the round trip.
 */
import { dockKeyFrom, type KeyRecording } from "#lib/dockKey.ts";
import { formatNumber, list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { IS_MAC, kbd, normalize } from "#lib/keys.ts";
import type { CapturePrefs, Entry, Keys, Preset, Verdict } from "./types.ts";

/**
 * Every capture action with a hotkey, in the order the page lists them (and `clashes` reports the first duplicate).
 * `label` and `description` are getters: a read gives the text in the language of the moment.
 */
export const KEY_FIELDS: readonly {
	readonly field: keyof Keys;
	readonly label: string;
	readonly description: string;
}[] = [
	{
		field: "allInOne",
		get label() {
			return m.settings_capture_key_all_in_one();
		},
		get description() {
			return m.settings_capture_key_all_in_one_description();
		},
	},
	{
		field: "region",
		get label() {
			return m.capture_overlay_page_mode_region();
		},
		get description() {
			return m.settings_capture_key_region_description();
		},
	},
	{
		field: "previous",
		get label() {
			return m.capture_overlay_page_previous_area();
		},
		get description() {
			return m.settings_capture_key_previous_description();
		},
	},
	{
		field: "window",
		get label() {
			return m.capture_overlay_page_mode_window();
		},
		get description() {
			return m.settings_capture_key_window_description();
		},
	},
	{
		field: "fullscreen",
		get label() {
			return m.capture_overlay_page_mode_screen();
		},
		get description() {
			return m.settings_capture_key_fullscreen_description();
		},
	},
	{
		field: "ocr",
		get label() {
			return m.capture_quick_page_ocr();
		},
		get description() {
			return m.settings_capture_key_ocr_description();
		},
	},
	{
		field: "record",
		get label() {
			return m.capture_overlay_page_mode_record();
		},
		get description() {
			return m.settings_capture_key_record_description();
		},
	},
	{
		field: "gif",
		get label() {
			return m.settings_capture_key_gif();
		},
		get description() {
			return m.settings_capture_key_gif_description();
		},
	},
];

/** A key press while a row is recording: a stroke Rust accepts, a cancel, a wait, or a refusal with the reason. */
export function recordingResult(e: KeyboardEvent, mac: boolean): KeyRecording {
	return dockKeyFrom(e, mac);
}

/** `⇧⌘S` (macOS) or `Ctrl+Shift+S`; an empty stroke reads "Not set". */
export function keyLabel(stroke: string, mac = IS_MAC): string {
	return stroke.trim() ? kbd(stroke.trim(), mac) : m.settings_capture_key_not_set();
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
export const timerOptions = () => [
	{ value: "0", label: m.settings_capture_timer_off() },
	{ value: "3", label: m.settings_capture_timer_seconds({ seconds: 3 }) },
	{ value: "5", label: m.settings_capture_timer_seconds({ seconds: 5 }) },
	{ value: "10", label: m.settings_capture_timer_seconds({ seconds: 10 }) },
];

export const scaleOptions = () => [
	{ value: "native", label: m.settings_capture_scale_retina() },
	{ value: "1x", label: m.settings_capture_scale_standard() },
];

export const cornerOptions = () => [
	{ value: "bottomRight", label: m.settings_capture_corner_bottom_right() },
	{ value: "bottomLeft", label: m.settings_capture_corner_bottom_left() },
	{ value: "topRight", label: m.settings_capture_corner_top_right() },
	{ value: "topLeft", label: m.settings_capture_corner_top_left() },
];

/** Recording quality: Rust's `VideoQuality` words (`prefs.rs`). Low and Medium cap the pixels, Low also the frame rate. */
export const qualityOptions = () => [
	{ value: "high", label: m.settings_capture_quality_high() },
	{ value: "medium", label: m.settings_capture_quality_medium() },
	{ value: "low", label: m.settings_capture_quality_low() },
];

/** The four strokes macOS keeps for its own screenshots. */
const SYSTEM_STROKES = ["cmd-shift-3", "cmd-shift-4", "cmd-shift-5", "cmd-shift-6"] as const;

/** Which of macOS's own screenshot keys (the four of `SYSTEM_STROKES`) `keys` uses, as the keyboard prints them. */
export function systemKeysUsed(keys: Keys, mac = IS_MAC): string[] {
	return SYSTEM_STROKES.filter((stroke) =>
		KEY_FIELDS.some(({ field }) => sameStroke(keys[field], stroke)),
	).map((stroke) => kbd(stroke, mac));
}

/** macOS's four screenshot keys and their range, as the messages that name them take them (`{screen}`, `{range}`). */
export function systemKeyArgs(mac = IS_MAC) {
	const [screen, area, toolbar, touchBar] = SYSTEM_STROKES.map((stroke) => kbd(stroke, mac));
	return { screen, area, toolbar, touchBar, range: `${screen}–6` };
}

/** The warning for the macOS screenshot keys a key set uses, naming them in the language's list style. */
export function systemKeysClash(used: readonly string[]): string {
	return m.settings_capture_system_keys_clash({ count: used.length, keys: list(used) });
}

/**
 * The name of a preset. Rust sends its own English `label`; the page shows the message for the preset's stable `id`
 * and, for an id it does not know (a newer Rust), Rust's label as sent. `capture-prefs.json` (written by Rust) is the
 * contract: `settings.test.ts` fails when a real preset has no message or Rust's English moved.
 */
export function presetLabel(preset: Pick<Preset, "id" | "label">, mac = IS_MAC): string {
	switch (preset.id) {
		case "default":
			return m.settings_capture_preset_default_label({ keys: `${kbd("ctrl-alt-1", mac)}–8` });
		case "cleanshot":
			return m.settings_capture_preset_cleanshot_label({ first: kbd("cmd-shift-3", mac) });
		case "ctrl-shift":
			return m.settings_capture_preset_ctrl_shift_label({
				keys: `${kbd("ctrl-shift-3", mac)}–9`,
				cmd: kbd("cmd", mac),
			});
		case "shift-cmd-s":
			return m.settings_capture_preset_shift_cmd_s_label({ key: kbd("shift-cmd-s", mac) });
		default:
			return preset.label;
	}
}

/** The note under a picked preset (`null`: nothing to say), by the same rule as [`presetLabel`]. */
export function presetNote(preset: Pick<Preset, "id" | "note">, mac = IS_MAC): string | null {
	switch (preset.id) {
		case "default":
			return null;
		case "cleanshot":
			return m.settings_capture_preset_cleanshot_note(systemKeyArgs(mac));
		case "ctrl-shift":
			return m.settings_capture_preset_ctrl_shift_note({
				example: kbd("ctrl-shift-4", mac),
				modifiers: kbd("ctrl-shift", mac),
			});
		case "shift-cmd-s": {
			const { screen, area, toolbar } = systemKeyArgs(mac);
			return m.settings_capture_preset_shift_cmd_s_note({
				key: kbd("shift-cmd-s", mac),
				screen,
				area,
				toolbar,
			});
		}
		default:
			return preset.note;
	}
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
				return m.settings_capture_clash_same_key({
					first: a.label,
					second: b.label,
					key: keyLabel(keys[a.field]),
				});
		if (dockKeys.some((d) => sameStroke(keys[a.field], d)))
			return m.settings_capture_clash_dock({ action: a.label, key: keyLabel(keys[a.field]) });
	}
	return null;
}

export interface HistoryGroup {
	/** "Today", "Yesterday", "This week" or "Earlier", in the display language. */
	title: string;
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
		{ title: m.settings_capture_group_today(), entries: [] },
		{ title: m.settings_capture_group_yesterday(), entries: [] },
		{ title: m.settings_capture_group_this_week(), entries: [] },
		{ title: m.settings_capture_group_earlier(), entries: [] },
	];
	for (const e of [...entries].sort((a, b) => b.at - a.at)) {
		const i = !(e.at < today) ? 0 : e.at >= yesterday ? 1 : e.at >= week ? 2 : 3;
		groups[i].entries.push(e);
	}
	return groups.filter((g) => g.entries.length);
}

function kindLabel(kind: Entry["kind"]): string {
	switch (kind) {
		case "screenshot":
			return m.capture_editor_page_screenshot();
		case "recording":
			return m.capture_quick_title_recording();
		case "gif":
			return m.settings_capture_key_gif();
	}
}

/** `512 B`, `20 KB`, `2.4 MB`: the unit symbols are the same in every language, the decimal mark is not. */
function sizeText(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
	return `${formatNumber(bytes / 1024 / 1024, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MB`;
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
		kind: kindLabel(e.kind),
		size,
		detail: [`${e.width} × ${e.height}`, e.secs === null ? null : durationText(e.secs), size]
			.filter(Boolean)
			.join(" · "),
	};
}
