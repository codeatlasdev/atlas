/**
 * Capture (`shots`) types: the JSON Rust (`crates/atlas-desktop/src/shots`) exchanges with the shots pages, camelCase.
 * Coordinates are global logical points (origin top-left of the primary display, y down) unless a page says it
 * stores display-local ones. Types only: no logic lives here.
 */

export type Kind = "screenshot" | "recording" | "gif";

export interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

export interface Display {
	id: number;
	frame: Rect;
	/** Backing scale (1 or 2 on a Mac): points to pixels. */
	scale: number;
}

export interface Entry {
	path: string;
	kind: Kind;
	/** Epoch ms. */
	at: number;
	width: number;
	height: number;
	bytes: number;
	/** Duration of a recording / GIF, `null` for a still. */
	secs: number | null;
}

export interface History {
	entries: Entry[];
}

export interface Keys {
	allInOne: string;
	region: string;
	previous: string;
	window: string;
	fullscreen: string;
	ocr: string;
	record: string;
	gif: string;
}

export interface After {
	copy: boolean;
	quickAccess: boolean;
	dismissSecs: number;
	sound: boolean;
}

export type LastMode = "region" | "window" | "screen" | "record" | "gif" | "text";

/** Screenshot options (`CapturePrefs.still`). */
export interface StillPrefs {
	lastMode: LastMode;
	/** 0 (off), 3, 5 or 10. */
	timerSecs: number;
	windowShadow: boolean;
	windowPadding: boolean;
	/** `native` = retina pixels, `1x` = one pixel per point. */
	scale: "native" | "1x";
	hideIcons: boolean;
	/** A region commits the moment the mouse is released; off: it stays held until Return. */
	captureOnRelease: boolean;
}

export type Corner = "bottomRight" | "bottomLeft" | "topRight" | "topLeft";

/** Quick-access stack options (`CapturePrefs.quick`). */
export interface QuickPrefs {
	position: Corner;
	stack: number;
}

export type VideoQuality = "low" | "medium" | "high";

/** Recording options (`CapturePrefs.recording`). */
export interface RecordingPrefs {
	keystrokes: boolean;
	clicks: boolean;
	microphone: boolean;
	systemAudio: boolean;
	webcam: boolean;
	doNotDisturb: boolean;
	quality: VideoQuality;
}

/** What this Mac can record besides the picture (`shots_record_capabilities`); `reason` explains whichever is off. */
export interface RecordCapabilities {
	systemAudio: boolean;
	microphone: boolean;
	reason: string | null;
}

export interface CapturePrefs {
	folder: string;
	keys: Keys;
	after: After;
	still: StillPrefs;
	quick: QuickPrefs;
	recording: RecordingPrefs;
	highlightCursor: boolean;
	enabled: boolean;
	history: number;
	/** The person turned macOS's own ⇧⌘3–6 screenshot shortcuts off and said so. */
	systemKeysFree: boolean;
}

export interface Preset {
	id: string;
	label: string;
	keys: Keys;
	note: string | null;
	/** Uses macOS's own screenshot keys: unavailable until `systemKeysFree`. */
	needsSystemKeys: boolean;
}

export type Verdict = { kind: "ok" } | { kind: "warn"; reason: string } | { kind: "refused"; reason: string };

/**
 * The modes the overlay opens in. `record`, `gif` and `text` select a region like `region` does; `all` is the
 * All-in-One HUD (the page picks its sub-mode and sends the action with the commit).
 */
export type Mode = "region" | "window" | "record" | "gif" | "text" | "all";

/** What a committed selection becomes. Plain overlays ignore it (Rust takes the action from the mode). */
export type CommitAction = "capture" | "record" | "gif" | "text";

/** A window the overlay can snap to, front-to-back, on the overlay's own display. */
export interface OverlayWindow {
	id: number;
	frame: Rect;
	title: string;
}

/** What the All-in-One HUD starts from (`OverlayInit.hud`). */
export interface HudInit {
	lastMode: LastMode;
	timerSecs: number;
	/** "Capture previous area" has an area to use. */
	hasPrevious: boolean;
	/** `still.captureOnRelease`: a dragged region is captured on release. */
	captureOnRelease: boolean;
}

/** `shots_overlay_init()`. */
export interface OverlayInit {
	display: Display;
	mode: Mode;
	windows: OverlayWindow[];
	/** Global points. */
	cursor: { x: number; y: number };
	hud: HudInit;
}

/** `shots_overlay_commit` arguments. `action` defaults to `capture`. */
export type OverlayCommit =
	| { kind: "region"; rect: Rect; display: number; action?: CommitAction }
	| { kind: "window"; windowId: number; action?: CommitAction }
	| { kind: "screen"; display: number; action?: CommitAction };

/** Event `shots://quick` / `shots_quick_state()`. */
export interface QuickState {
	entry: Entry;
	/** Data URL. */
	thumb: string;
	dismissSecs: number;
	copied: boolean;
}

export type QuickAction = "copy" | "save" | "annotate" | "send" | "reveal" | "close" | "discard";

/** Event `shots://text` (after "capture text"). */
export interface TextResult {
	text: string;
	preview: string;
	lines: number;
	chars: number;
	empty: boolean;
}

/** Event `shots://record` / `shots_record_state()`. */
export interface RecordState {
	phase: "idle" | "countdown" | "recording" | "paused" | "saving" | "done" | "error";
	elapsedMs: number;
	maxMs: number;
	kind: "recording" | "gif";
	message?: string;
	entry?: Entry;
}

/** `shots_gif_presets()` item. */
export interface GifPreset {
	id: string;
	label: string;
	fps: number;
	maxSide: number;
}

/** `shots_gif_estimate()`. */
export interface GifEstimate {
	width: number;
	height: number;
	fps: number;
	bytes: number;
}

/** Event `shots://gif-progress`. */
export interface GifProgress {
	done: number;
	total: number;
}

/** `shots_editor_load()`. */
export interface EditorDoc {
	path: string;
	width: number;
	height: number;
	/** Data URL or asset path. */
	src: string;
}
