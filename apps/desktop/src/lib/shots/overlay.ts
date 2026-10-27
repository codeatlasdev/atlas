/**
 * The capture overlay (`/shots/overlay`): a transparent window per display where the user drags a region or clicks a
 * window. Like `lib/dock.ts` everything here is pure (no Svelte, no IPC): the state machine
 * `reduce(state, event) → {state, effects}` plus the geometry it needs, ported from
 * `crates/atlas-desktop/src/shots/geom.rs` (`drag_rect`, `square`, `window_at`, `size_label`, `loupe_origin`).
 *
 * The reducer stores display-local page points (the page is exactly the display's size). It converts to the global
 * points Rust expects only in the `commit` effect payload.
 */
import type {
	CommitAction,
	Display,
	LastMode,
	Mode,
	OverlayCommit,
	OverlayInit,
	OverlayWindow,
	Rect,
} from "./types.ts";

/** The smallest region a drag may produce (`geom::MIN_REGION`). Below it a release is a no-op. */
export const MIN_REGION = 4;

export interface Point {
	x: number;
	y: number;
}

export interface Size {
	w: number;
	h: number;
}

/** What a drag in progress remembers. `start` is the anchor: the press point, moved along by Space-drag. */
export interface Drag {
	start: Point;
	last: Point;
	shift: boolean;
	alt: boolean;
	/** Space is held: the pointer moves the whole rectangle instead of resizing it. */
	moving: boolean;
}

/** The eight resize handles of a held selection, by the edge(s) each one moves. */
export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export const HANDLES: readonly Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

/** A move/resize of the held selection in progress. `origin` is the selection when the gesture began. */
export interface Edit {
	kind: "move" | "resize";
	handle: Handle | null;
	grab: Point;
	origin: Rect;
	/** Shift: the resize keeps `origin`'s aspect ratio (fixed when the resize began, not per event). */
	lock: boolean;
}

/** The All-in-One HUD: which mode the next gesture means. */
export interface Hud {
	mode: LastMode;
	timerSecs: number;
	hasPrevious: boolean;
}

export interface OverlayState {
	display: Display | null;
	windows: OverlayWindow[];
	/** What a click does now: drag a region, or pick the hovered window. */
	pick: "region" | "window";
	pointer: Point | null;
	drag: Drag | null;
	/** The live rectangle while dragging (display-local), `null` otherwise or while below MIN_REGION. */
	rect: Rect | null;
	/** A released drag: it stays, can be moved/resized/nudged, and commits on Return or double click. */
	held: Rect | null;
	/** Moving or resizing `held` with the pointer. */
	edit: Edit | null;
	/** Window mode: the front-most window under the pointer. */
	hover: OverlayWindow | null;
	/** `null` in a plain overlay; the All-in-One HUD otherwise. */
	hud: Hud | null;
	/** A press landed on this overlay in window/screen pick and has not been released: the release is a click. */
	pressed: boolean;
	/** After a commit or cancel the overlay is ending: nothing else is accepted. */
	done: boolean;
	/** `still.captureOnRelease`: a valid drag commits the moment the button is released instead of being held. */
	captureOnRelease: boolean;
}

export type OverlayEvent =
	| { type: "init"; init: OverlayInit }
	| { type: "move"; x: number; y: number; shift?: boolean; alt?: boolean }
	| { type: "down"; x: number; y: number; button: number; shift: boolean; alt: boolean; clicks?: number }
	| { type: "up"; x: number; y: number }
	/** `up: true` is a keyup (Space ends a Space-drag). `shift`/`alt` are the modifiers held with the key. */
	| { type: "key"; key: string; up?: boolean; shift?: boolean; alt?: boolean }
	| { type: "windows"; windows: OverlayWindow[] }
	/** The HUD toolbar: pick a mode (a button, 1..6 or Tab). */
	| { type: "mode"; mode: LastMode }
	/** The HUD toolbar's "Previous area". */
	| { type: "previous" }
	/** Rust answered the timer button with the seconds now in force (0, 3, 5 or 10). */
	| { type: "timer"; secs: number };

export type OverlayEffect =
	| { type: "commit"; payload: OverlayCommit }
	| { type: "cancel" }
	/** The HUD asked for the previous area: the page calls `shots_overlay_previous`. */
	| { type: "previous" };

export interface Step {
	state: OverlayState;
	effects: OverlayEffect[];
}

export const initialState = (): OverlayState => ({
	display: null,
	windows: [],
	pick: "region",
	pointer: null,
	drag: null,
	rect: null,
	held: null,
	edit: null,
	hover: null,
	hud: null,
	pressed: false,
	captureOnRelease: false,
	done: false,
});

/** Window mode snaps to windows; every other mode (region, record, gif, text, screen, all) works on a region. */
const pickOf = (mode: Mode | LastMode): OverlayState["pick"] => (mode === "window" ? "window" : "region");

/** The HUD's modes in toolbar order: keys 1..6 pick them, Tab cycles through them. */
export const HUD_MODES: readonly LastMode[] = ["region", "window", "screen", "record", "gif", "text"];

/** The action a HUD mode commits with (`screen` and `region` capture). */
export const actionOf = (mode: LastMode): CommitAction =>
	mode === "record" || mode === "gif" || mode === "text" ? mode : "capture";

/** The mode after `mode` in the toolbar, wrapping (`step` -1 goes back). */
export function cycleMode(mode: LastMode, step = 1): LastMode {
	const i = HUD_MODES.indexOf(mode);
	return HUD_MODES[(i + step + HUD_MODES.length) % HUD_MODES.length]!;
}

/** What the user did to produce a commit: dragged/kept a region, clicked a window, clicked/Returned for the screen. */
export type Gesture =
	| { kind: "region"; rect: Rect; display: number }
	| { kind: "window"; window: OverlayWindow; display: number }
	| { kind: "screen"; display: number };

/**
 * The commit a `gesture` makes in HUD `mode` (`rect` and window frames are already global points). Plain overlays
 * (no HUD) pass `mode: null`: the payload then carries no `action` and a window stays a window.
 *
 * - region/window/screen gestures keep their kind in capture modes;
 * - in record/gif/text a window click sends the window's frame as a region (the Rust machine refuses window commits
 *   for those) and a screen gesture stays a screen (Rust has no text-of-screen commit, so text of a screen is the
 *   whole display's frame as a region).
 */
export function resolveCommit(mode: LastMode | null, g: Gesture, display?: Display): OverlayCommit | null {
	const action = mode === null ? undefined : actionOf(mode);
	const withAction = <T extends OverlayCommit>(c: T): T => (action ? { ...c, action } : c);
	const capture = action === undefined || action === "capture";
	switch (g.kind) {
		case "region":
			return withAction({ kind: "region", rect: g.rect, display: g.display });
		case "window":
			if (capture) return withAction({ kind: "window", windowId: g.window.id });
			return withAction({ kind: "region", rect: g.window.frame, display: g.display });
		case "screen":
			if (action === "text") {
				if (!display) return null;
				return withAction({ kind: "region", rect: display.frame, display: display.id });
			}
			return withAction({ kind: "screen", display: g.display });
	}
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

const contains = (r: Rect, p: Point) => p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h;

/** The front-most window under the pointer (`windows` is ordered front to back). */
export function windowAt(windows: readonly OverlayWindow[], p: Point): OverlayWindow | null {
	return windows.find((w) => contains(w.frame, p)) ?? null;
}

/** `Math.sign` that treats 0 as `zero` (`geom::SignumOr`). */
const signOr = (v: number, zero: number) => (v === 0 ? zero : Math.sign(v));

/**
 * Shift: the pointer becomes the corner of a square on the longer side, press point fixed. When the edge cuts one
 * axis the side shrinks to what fits on the tighter one, so the result stays a square (`geom::square`).
 * `room` is the free space per axis in the drag's direction, supplied by the caller (it differs for Alt).
 */
function squareOf(dx: number, dy: number, room: Size): Point {
	const side = Math.min(Math.max(Math.abs(dx), Math.abs(dy)), room.w, room.h);
	return { x: side * signOr(dx, 1), y: side * signOr(dy, 1) };
}

/**
 * The rectangle a drag describes, clamped to a display of `bounds` size at the origin (`geom::drag_rect` without the
 * MIN_REGION cut, which the caller applies). Alt grows it out from the press point; Shift makes it a square.
 */
export function dragRect(start: Point, now: Point, bounds: Size, shift: boolean, alt: boolean): Rect {
	const sx = clamp(start.x, 0, bounds.w);
	const sy = clamp(start.y, 0, bounds.h);
	const nx = clamp(now.x, 0, bounds.w);
	const ny = clamp(now.y, 0, bounds.h);
	let dx = nx - sx;
	let dy = ny - sy;
	// free space from the press point toward the pointer, per axis (Alt: the tighter of both sides)
	const toward = (d: number, at: number, max: number) => (d >= 0 ? max - at : at);
	const away = (d: number, at: number, max: number) => (d >= 0 ? at : max - at);
	const room = (d: number, at: number, max: number) =>
		alt ? Math.min(toward(d, at, max), away(d, at, max)) : toward(d, at, max);
	if (alt) {
		// the half-extent is capped by what fits on both sides, per axis
		dx = signOr(dx, 1) * Math.min(Math.abs(dx), room(dx, sx, bounds.w));
		dy = signOr(dy, 1) * Math.min(Math.abs(dy), room(dy, sy, bounds.h));
		if (nx === sx) dx = 0;
		if (ny === sy) dy = 0;
	}
	if (shift) {
		const sq = squareOf(dx, dy, { w: room(dx, sx, bounds.w), h: room(dy, sy, bounds.h) });
		dx = sq.x;
		dy = sq.y;
	}
	if (alt) return { x: sx - Math.abs(dx), y: sy - Math.abs(dy), w: Math.abs(dx) * 2, h: Math.abs(dy) * 2 };
	return { x: Math.min(sx, sx + dx), y: Math.min(sy, sy + dy), w: Math.abs(dx), h: Math.abs(dy) };
}

/** HUD modes where a click (no drag) over a window picks that window, like window mode does. */
const CLICK_WINDOW_MODES: readonly LastMode[] = ["record", "gif", "text"];

/** How far (points) from a handle's centre a press still grabs it. */
export const HANDLE_REACH = 6;

const big = (r: Rect) => r.w >= MIN_REGION && r.h >= MIN_REGION;

/** The live rectangle of a drag: `null` while it is below MIN_REGION on either side. */
function liveRect(d: Drag, bounds: Size, current: Rect | null): Rect | null {
	if (d.moving) return current;
	const r = dragRect(d.start, d.last, bounds, d.shift, d.alt);
	return big(r) ? r : null;
}

const sizeOf = (d: Display): Size => ({ w: d.frame.w, h: d.frame.h });

/** A display-local rectangle as the global points Rust captures. */
const toGlobal = (r: Rect, d: Display): Rect => ({ x: r.x + d.frame.x, y: r.y + d.frame.y, w: r.w, h: r.h });

/** Which handle's square (of `reach` points around it) holds `p`, or `null`. Corners win over edges. */
export function handleAt(r: Rect, p: Point, reach: number): Handle | null {
	const xs = { w: r.x, e: r.x + r.w, c: r.x + r.w / 2 };
	const ys = { n: r.y, s: r.y + r.h, c: r.y + r.h / 2 };
	const at: Record<Handle, Point> = {
		nw: { x: xs.w, y: ys.n },
		n: { x: xs.c, y: ys.n },
		ne: { x: xs.e, y: ys.n },
		e: { x: xs.e, y: ys.c },
		se: { x: xs.e, y: ys.s },
		s: { x: xs.c, y: ys.s },
		sw: { x: xs.w, y: ys.s },
		w: { x: xs.w, y: ys.c },
	};
	return HANDLES.find((h) => Math.abs(p.x - at[h].x) <= reach && Math.abs(p.y - at[h].y) <= reach) ?? null;
}

/** `r` translated by (`dx`, `dy`), kept inside a display of `bounds` size. */
export function moveRect(r: Rect, dx: number, dy: number, bounds: Size): Rect {
	return { ...r, x: clamp(r.x + dx, 0, bounds.w - r.w), y: clamp(r.y + dy, 0, bounds.h - r.h) };
}

/**
 * `origin` resized by dragging `handle` by (`dx`, `dy`): the edges the handle owns follow the pointer, the others stay.
 * The result stays inside the display and never drops below MIN_REGION (an edge stops MIN_REGION short of the
 * opposite one, it never flips). `lock` keeps `origin`'s aspect ratio: corners scale from the opposite corner on the
 * axis the pointer moved further, edges grow the other axis around the origin's centre.
 */
export function resizeRect(
	origin: Rect,
	handle: Handle,
	dx: number,
	dy: number,
	bounds: Size,
	lock: boolean,
): Rect {
	const moves = {
		w: handle.includes("w"),
		e: handle.includes("e"),
		n: handle.includes("n"),
		s: handle.includes("s"),
	};
	let l = origin.x;
	let t = origin.y;
	let r = origin.x + origin.w;
	let b = origin.y + origin.h;
	if (moves.w) l = clamp(l + dx, 0, r - MIN_REGION);
	if (moves.e) r = clamp(r + dx, l + MIN_REGION, bounds.w);
	if (moves.n) t = clamp(t + dy, 0, b - MIN_REGION);
	if (moves.s) b = clamp(b + dy, t + MIN_REGION, bounds.h);
	if (!lock || origin.w <= 0 || origin.h <= 0) return { x: l, y: t, w: r - l, h: b - t };

	const ratio = origin.w / origin.h;
	const horizontal = moves.w || moves.e;
	const vertical = moves.n || moves.s;
	// the anchor is the opposite corner/edge; `room` is how far the rect can grow from it on each axis
	const ax = moves.w ? origin.x + origin.w : origin.x;
	const ay = moves.n ? origin.y + origin.h : origin.y;
	let w = r - l;
	let h = b - t;
	if (horizontal && vertical) {
		// the axis the pointer pulled further (relative to the origin size) decides
		const byW = Math.abs(w - origin.w) / origin.w >= Math.abs(h - origin.h) / origin.h;
		w = byW ? w : h * ratio;
		h = w / ratio;
		const roomW = moves.w ? ax : bounds.w - ax;
		const roomH = moves.n ? ay : bounds.h - ay;
		const scale = Math.min(1, roomW / w, roomH / h);
		w *= scale;
		h *= scale;
		// never below MIN_REGION on the short side
		const grow = Math.max(1, MIN_REGION / Math.min(w, h));
		w *= grow;
		h *= grow;
		return { x: moves.w ? ax - w : ax, y: moves.n ? ay - h : ay, w, h };
	}
	// an edge handle: the dragged axis decides, the other follows around the centre
	const cx = origin.x + origin.w / 2;
	const cy = origin.y + origin.h / 2;
	if (horizontal) {
		h = w / ratio;
		const maxH = 2 * Math.min(cy, bounds.h - cy);
		if (h > maxH) {
			h = maxH;
			w = h * ratio;
		}
		const x = moves.w ? ax - w : ax;
		return { x, y: cy - h / 2, w, h };
	}
	w = h * ratio;
	const maxW = 2 * Math.min(cx, bounds.w - cx);
	if (w > maxW) {
		w = maxW;
		h = w / ratio;
	}
	const y = moves.n ? ay - h : ay;
	return { x: cx - w / 2, y, w, h };
}

const ARROWS: Record<string, Point> = {
	ArrowLeft: { x: -1, y: 0 },
	ArrowRight: { x: 1, y: 0 },
	ArrowUp: { x: 0, y: -1 },
	ArrowDown: { x: 0, y: 1 },
};

/**
 * An arrow key on the held selection: it moves by 1 point (10 with Shift); with Alt it resizes from the bottom-right
 * edge instead (the top-left stays). Always inside the display, never below MIN_REGION. `null` for other keys.
 */
export function nudgeRect(r: Rect, key: string, shift: boolean, alt: boolean, bounds: Size): Rect | null {
	const dir = ARROWS[key];
	if (!dir) return null;
	const step = shift ? 10 : 1;
	if (!alt) return moveRect(r, dir.x * step, dir.y * step, bounds);
	return resizeRect(r, "se", dir.x * step, dir.y * step, bounds, false);
}

const cancel = (s: OverlayState): Step => ({
	state: { ...s, drag: null, rect: null, held: null, edit: null, done: true },
	effects: [{ type: "cancel" }],
});

const idle = (s: OverlayState): Step => ({ state: s, effects: [] });

function withHover(s: OverlayState): OverlayState {
	const hover = s.pick === "window" && s.pointer ? windowAt(s.windows, s.pointer) : null;
	return { ...s, hover };
}

/** Switch the HUD to `mode`: the selection in progress is dropped, the hover follows the new pick. */
function setMode(s: OverlayState, mode: LastMode): OverlayState {
	if (!s.hud) return s;
	return withHover({
		...s,
		hud: { ...s.hud, mode },
		pick: pickOf(mode),
		drag: null,
		rect: null,
		held: null,
		edit: null,
		pressed: false,
	});
}

/** The commit of `gesture` in the state's mode, ending the overlay. */
function commitStep(s: OverlayState, g: Gesture): Step {
	const d = s.display;
	// a screen selection as text is the whole display's frame (global points) sent as a region
	const payload = resolveCommit(s.hud?.mode ?? null, g, d ?? undefined);
	if (!payload) return idle(s);
	const next = { ...s, drag: null, rect: null, held: null, edit: null, done: true };
	return { state: next, effects: [{ type: "commit", payload }] };
}

/** Return, or a double click, on the held selection: commits only a valid (>= MIN_REGION) one. */
function commitHeld(s: OverlayState): Step {
	const display = s.display;
	if (!display || !s.held || !big(s.held)) return idle(s);
	return commitStep(s, { kind: "region", rect: toGlobal(s.held, display), display: display.id });
}

/**
 * Return with nothing held: window mode takes the hovered window; screen, record, gif and text take the whole
 * display (text of a display is its frame as a region). Region mode and plain overlays do nothing.
 */
function commitReturn(s: OverlayState): Step {
	const display = s.display;
	if (!display) return idle(s);
	if (s.held) return commitHeld(s);
	if (s.pick === "window" && s.hover) {
		const window = { ...s.hover, frame: toGlobal(s.hover.frame, display) };
		return commitStep(s, { kind: "window", window, display: display.id });
	}
	if (s.hud && s.hud.mode !== "region" && s.pick !== "window") {
		return commitStep(s, { kind: "screen", display: display.id });
	}
	return idle(s);
}

/** The pure transition function: what happens to the overlay on `ev`, and which side effects the page must run. */
export function reduce(s: OverlayState, ev: OverlayEvent): Step {
	if (s.done) return idle(s);
	switch (ev.type) {
		case "init": {
			const hud = ev.init.mode === "all" && ev.init.hud ? ev.init.hud : null;
			const mode = hud ? (HUD_MODES.includes(hud.lastMode) ? hud.lastMode : "region") : null;
			return {
				state: withHover({
					...s,
					display: ev.init.display,
					windows: ev.init.windows,
					hud: hud && mode ? { mode, timerSecs: hud.timerSecs, hasPrevious: hud.hasPrevious } : null,
					captureOnRelease: ev.init.hud?.captureOnRelease === true,
					pick: mode ? pickOf(mode) : pickOf(ev.init.mode),
					pointer: {
						x: ev.init.cursor.x - ev.init.display.frame.x,
						y: ev.init.cursor.y - ev.init.display.frame.y,
					},
				}),
				effects: [],
			};
		}

		case "windows":
			return { state: withHover({ ...s, windows: ev.windows }), effects: [] };

		case "move":
			return { state: onMove(s, ev), effects: [] };

		case "down":
			return onDown(s, ev);

		case "up":
			return onUp(s, ev);

		case "mode":
			return { state: setMode(s, ev.mode), effects: [] };

		case "previous":
			if (!s.hud?.hasPrevious) return idle(s);
			return { state: { ...s, done: true }, effects: [{ type: "previous" }] };

		case "timer":
			// Only the values Rust accepts (`prefs::TIMER_VALUES`); anything else leaves the HUD as it was.
			if (!s.hud || ![0, 3, 5, 10].includes(ev.secs)) return idle(s);
			return { state: { ...s, hud: { ...s.hud, timerSecs: ev.secs } }, effects: [] };

		case "key":
			return onKey(s, ev);
	}
}

function onDown(s: OverlayState, ev: Extract<OverlayEvent, { type: "down" }>): Step {
	if (ev.button === 2) return cancel(s);
	const display = s.display;
	if (ev.button !== 0 || !display) return idle(s);
	const at = { x: ev.x, y: ev.y };
	// window mode and screen mode act on the release, never start a drag
	if (s.pick === "window" || s.hud?.mode === "screen") {
		return { state: { ...s, pointer: at, pressed: true }, effects: [] };
	}
	const held = s.held;
	if (held) {
		if ((ev.clicks ?? 1) >= 2 && contains(held, at)) return commitHeld(s);
		const handle = handleAt(held, at, HANDLE_REACH);
		if (handle) {
			const edit: Edit = { kind: "resize", handle, grab: at, origin: held, lock: ev.shift };
			return { state: { ...s, pointer: at, edit }, effects: [] };
		}
		if (contains(held, at)) {
			const edit: Edit = { kind: "move", handle: null, grab: at, origin: held, lock: false };
			return { state: { ...s, pointer: at, edit }, effects: [] };
		}
	}
	const drag: Drag = { start: at, last: at, shift: ev.shift, alt: ev.alt, moving: false };
	return { state: { ...s, pointer: at, drag, rect: null, held: null, edit: null }, effects: [] };
}

function onMove(s: OverlayState, ev: Extract<OverlayEvent, { type: "move" }>): OverlayState {
	const pointer = { x: ev.x, y: ev.y };
	const e = s.edit;
	if (e && s.held && s.display) {
		const b = sizeOf(s.display);
		const dx = ev.x - e.grab.x;
		const dy = ev.y - e.grab.y;
		const held =
			e.kind === "move" || !e.handle
				? moveRect(e.origin, dx, dy, b)
				: resizeRect(e.origin, e.handle, dx, dy, b, ev.shift ?? e.lock);
		return { ...s, pointer, held, edit: { ...e, lock: ev.shift ?? e.lock } };
	}
	const d = s.drag;
	if (!d || !s.display) return withHover({ ...s, pointer });
	const shift = ev.shift ?? d.shift;
	const alt = ev.alt ?? d.alt;
	if (d.moving && s.rect) {
		// Space: translate the rectangle by the pointer's delta and carry the anchor with it
		const b = sizeOf(s.display);
		const rect = moveRect(s.rect, ev.x - d.last.x, ev.y - d.last.y, b);
		const moved = { x: rect.x - s.rect.x, y: rect.y - s.rect.y };
		const next = {
			...d,
			last: pointer,
			shift,
			alt,
			start: { x: d.start.x + moved.x, y: d.start.y + moved.y },
		};
		return { ...s, pointer, drag: next, rect };
	}
	const next = { ...d, last: pointer, shift, alt };
	return { ...s, pointer, drag: next, rect: liveRect(next, sizeOf(s.display), s.rect) };
}

function onUp(s: OverlayState, ev: Extract<OverlayEvent, { type: "up" }>): Step {
	const display = s.display;
	if (!display) return idle(s);
	const at = { x: ev.x, y: ev.y };
	if (s.edit) {
		// the release is the last move of the gesture; the selection stays held
		return { state: { ...onMove(s, { type: "move", x: ev.x, y: ev.y }), edit: null }, effects: [] };
	}
	// window and screen picks act on a click, which needs its press on this overlay
	const pressed = s.pressed;
	const released = { ...s, pressed: false };
	if (s.pick === "window") {
		const w = pressed ? windowAt(s.windows, at) : null;
		if (!w) return idle(released);
		const window = { ...w, frame: toGlobal(w.frame, display) };
		return commitStep(released, { kind: "window", window, display: display.id });
	}
	if (s.hud?.mode === "screen") {
		if (!pressed) return idle(released);
		return commitStep(released, { kind: "screen", display: display.id });
	}
	const d = s.drag;
	if (!d) return idle(released);
	// the release point counts as the last move (a fast flick may release before the last move event arrives)
	const settled = onMove(s, { type: "move", x: ev.x, y: ev.y });
	const rect = settled.rect;
	if (!rect || !big(rect)) {
		// a click (no drag) in record/gif/text over a window records/captures that window's frame as a region
		const hudMode = s.hud?.mode;
		const w = hudMode && CLICK_WINDOW_MODES.includes(hudMode) ? windowAt(s.windows, d.start) : null;
		if (w) {
			const window = { ...w, frame: toGlobal(w.frame, display) };
			return commitStep({ ...settled, drag: null }, { kind: "window", window, display: display.id });
		}
		return { state: { ...settled, drag: null, rect: null }, effects: [] };
	}
	// the selection stays (Return or a double click commits it), unless the pref commits it on release
	if (s.captureOnRelease) {
		return commitStep(
			{ ...settled, drag: null, rect: null },
			{ kind: "region", rect: toGlobal(rect, display), display: display.id },
		);
	}
	return { state: { ...settled, drag: null, rect: null, held: rect }, effects: [] };
}

function onKey(s: OverlayState, ev: Extract<OverlayEvent, { type: "key" }>): Step {
	if (ev.key === "Escape" && !ev.up) return cancel(s);
	if (ev.key === " ") return onSpace(s, ev.up === true);
	if (ev.up) return idle(s);
	const display = s.display;
	if (ev.key === "Enter") return commitReturn(s);
	if (s.hud && !s.drag && !s.edit) {
		const digit = Number(ev.key);
		if (Number.isInteger(digit) && digit >= 1 && digit <= HUD_MODES.length) {
			return { state: setMode(s, HUD_MODES[digit - 1]!), effects: [] };
		}
		if (ev.key === "Tab") return { state: setMode(s, cycleMode(s.hud.mode, ev.shift ? -1 : 1)), effects: [] };
	}
	if (s.held && display && !s.drag && !s.edit) {
		const held = nudgeRect(s.held, ev.key, ev.shift === true, ev.alt === true, sizeOf(display));
		if (held) return { state: { ...s, held }, effects: [] };
	}
	return idle(s);
}

function onSpace(s: OverlayState, released: boolean): Step {
	const d = s.drag;
	if (d) {
		// Space only means "move" once there is a rectangle to move
		if (released) return { state: { ...s, drag: { ...d, moving: false } }, effects: [] };
		if (!s.rect || d.moving) return idle(s);
		return { state: { ...s, drag: { ...d, moving: true } }, effects: [] };
	}
	// a held selection or the HUD (its modes are the toolbar's) keeps Space for nothing: no accidental mode flip
	if (released || s.held || s.edit || s.hud) return idle(s);
	const pick = s.pick === "region" ? "window" : "region";
	return { state: withHover({ ...s, pick }), effects: [] };
}

/** The label the overlay shows while dragging: pixel size, not points (what the file will be). */
export function sizeLabel(r: Rect, scale: number): string {
	return `${Math.round(r.w * scale)} × ${Math.round(r.h * scale)}`;
}

/** The selection's top-left in pixels from the display's own corner, for the readout beside the size. */
export function positionLabel(r: Rect, scale: number): string {
	return `${Math.round(r.x * scale)}, ${Math.round(r.y * scale)}`;
}

/** The accessible name of the held selection. */
export function selectionAria(r: Rect, scale: number): string {
	return `Selection ${sizeLabel(r, scale)} pixels at ${positionLabel(r, scale)}`;
}

const LABEL_GAP = 8;

/**
 * Where the size pill sits for `rect`: below it, flipped above when it would leave the display, inside (bottom-aligned)
 * when neither fits. Horizontally centred on the rect and kept inside `bounds`. Top-left of the pill.
 */
export function labelPlacement(
	rect: Rect,
	label: Size,
	bounds: Rect,
): { x: number; y: number; where: "below" | "above" | "inside" } {
	const x = clamp(rect.x + rect.w / 2 - label.w / 2, bounds.x, bounds.x + bounds.w - label.w);
	const below = rect.y + rect.h + LABEL_GAP;
	if (below + label.h <= bounds.y + bounds.h) return { x, y: below, where: "below" };
	const above = rect.y - LABEL_GAP - label.h;
	if (above >= bounds.y) return { x, y: above, where: "above" };
	return { x, y: rect.y + rect.h - LABEL_GAP - label.h, where: "inside" };
}

const LOUPE_OFFSET = 24;

/**
 * The magnifier's top-left: offset from the pointer, flipped to the other side at the right/bottom edge
 * (`geom::loupe_origin`).
 */
export function loupeOrigin(pointer: Point, loupe: number, bounds: Rect): Point {
	let x = pointer.x + LOUPE_OFFSET;
	let y = pointer.y + LOUPE_OFFSET;
	if (x + loupe > bounds.x + bounds.w) x = pointer.x - LOUPE_OFFSET - loupe;
	if (y + loupe > bounds.y + bounds.h) y = pointer.y - LOUPE_OFFSET - loupe;
	return { x: Math.max(x, bounds.x), y: Math.max(y, bounds.y) };
}
