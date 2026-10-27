/**
 * The quick-access stack (`/shots/quick`): several cards in the corner, each a capture (thumbnail, a few actions) or
 * a text read from an image, each closing itself after its own countdown. Rust owns the window, the files, the
 * clipboard and the list of cards (`shots://quick-stack`, `shots_quick_stack`); this module is the pure state
 * machine `reduce(state, event) → {state, effects}` the page drives (a `tick` every ~100 ms, pointer events, Rust
 * events), plus the format helpers. `quick.test.ts` pins the rules.
 *
 * Timer rules, PER CARD: the dismiss countdown only runs while the pointer is off THAT card, no action of that card
 * is in flight, its GIF sheet is closed and no error is waiting to be read; leaving the card resumes with the
 * REMAINING time. The "copied" badge and the discard confirmation are their own countdowns and run regardless of
 * those pauses. A card Rust re-sends (same `id`) keeps its local state; a card Rust no longer lists is gone.
 */

import type { Corner, Entry, Kind, QuickAction, QuickState } from "./types.ts";

/** How long the "Copied" feedback stays. */
export const COPIED_MS = 1500;
/** How long a first click on Discard waits for the second one. */
export const CONFIRM_MS = 3000;
/** The exact message `shots_gif_export` rejects with after `shots_gif_cancel` (`record_mac.rs`). */
export const GIF_CANCELLED = "GIF export cancelled.";

/** What a capture card sends to `shots_quick_action` (`close` has its own effect). */
export type InvokedAction = Exclude<QuickAction, "close"> | "pin" | "ocr";
/** Every name a capture card's buttons can dispatch. */
export type CaptureAction = QuickAction | "pin" | "ocr";
/** What a text card's buttons can dispatch (`shots_quick_text`). */
export type TextAction = "copy" | "send" | "close";

/** A capture card as Rust serializes it (`CaptureCard`). `id` is the file path. */
export interface CaptureCardIn extends QuickState {
	kind: "capture";
	id: string;
}

/** A text card as Rust serializes it (`TextCard`); the full text stays in Rust. */
export interface TextCardIn {
	kind: "text";
	id: string;
	preview: string;
	lines: number;
	chars: number;
	/** Nothing was recognized: the card says so and offers no copy / send. */
	empty: boolean;
	dismissSecs: number;
}

export type CardIn = CaptureCardIn | TextCardIn;

/** `shots://quick-stack` / `shots_quick_stack()`: the cards oldest first, and the corner they stack in. */
export interface StackIn {
	position: Corner;
	cards: CardIn[];
}

/** What every card keeps locally, whatever it shows. */
interface Local {
	id: string;
	/** Rust's setting at show time, `0` = never auto-dismiss. */
	dismissSecs: number;
	remainingMs: number;
	/** The pointer is over the card. */
	paused: boolean;
	copied: boolean;
	/** Time left of the "copied" feedback (`0` when not copied). */
	copiedMs: number;
	busy: boolean;
	error: string | null;
	/** An error stops the countdown until the pointer leaves the card. */
	errorHeld: boolean;
}

export interface CaptureView extends Local {
	kind: "capture";
	entry: Entry;
	/** Data URL. */
	thumb: string;
	/** The action Rust is working on (`null` = none); while set, the countdown is stopped. */
	pending: InvokedAction | null;
	sheet: null | "gif";
	/** GIF export progress, `null` until the first event. */
	progress: { done: number; total: number } | null;
	/** `shots_gif_export` is running for this card; Esc cancels it before closing the sheet. */
	exporting: boolean;
	/** First click on Discard happened, the second one deletes. */
	confirmDiscard: boolean;
	confirmMs: number;
}

export interface TextView extends Local {
	kind: "text";
	preview: string;
	lines: number;
	chars: number;
	empty: boolean;
	pending: "copy" | "send" | null;
}

export type CardView = CaptureView | TextView;

export interface QuickView {
	position: Corner;
	/** Oldest first, as Rust lists them; [`layout`] orders them for the corner. */
	cards: CardView[];
}

export type QuickEvent =
	/** Rust's list of cards (`shots://quick-stack`, or `shots_quick_stack` on mount). */
	| { type: "stack"; stack: StackIn }
	| { type: "tick"; dtMs: number }
	| { type: "hover"; id: string; on: boolean }
	| { type: "action"; id: string; name: CaptureAction }
	| { type: "textAction"; id: string; name: TextAction }
	| { type: "actionOk"; id: string }
	| { type: "actionErr"; id: string; message: string }
	| { type: "openGifSheet"; id: string }
	| { type: "closeSheet"; id: string }
	/** The page started `shots_gif_export` for this card's sheet. */
	| { type: "exportStart"; id: string }
	/** The export finished (saved or failed with an error the sheet shows). */
	| { type: "exportEnd"; id: string }
	/** The export rejected with [`GIF_CANCELLED`]: the sheet closes quietly. */
	| { type: "exportCancelled"; id: string }
	/** `shots://gif-progress` carries no path: it belongs to the card whose GIF sheet is open. */
	| { type: "gifProgress"; done: number; total: number }
	/** Esc: cancels what is pending in a card (sheet, discard confirmation), then closes the newest card. */
	| { type: "escape" };

export type QuickEffect =
	/** Ask Rust to remove the card (`shots_quick_action` `close` for a capture, `shots_quick_text` for a text). */
	| { type: "close"; id: string; card: "capture" | "text" }
	/** `shots_quick_hover`. */
	| { type: "hover"; hovering: boolean }
	/** `shots_quick_action` for everything but `close`; the page answers with `actionOk`/`actionErr`. */
	| { type: "invoke"; action: InvokedAction; id: string; path: string }
	/** `shots_quick_text` copy / send; the page answers with `actionOk`/`actionErr`. */
	| { type: "invokeText"; action: Exclude<TextAction, "close">; id: string }
	/** `shots_gif_cancel`: stop the running GIF encode (it stops within one frame). */
	| { type: "cancelExport" };

export interface Reduced {
	state: QuickView;
	effects: QuickEffect[];
}

export function initialState(): QuickView {
	return { position: "bottomRight", cards: [] };
}

/** Whether a card's dismiss countdown is running. */
export function counting(c: CardView): boolean {
	const sheetOpen = c.kind === "capture" && c.sheet !== null;
	return c.dismissSecs > 0 && !c.paused && !c.busy && !sheetOpen && !c.errorHeld;
}

/**
 * The cards in the order they are drawn, top to bottom: the newest nearest the corner, so bottom corners list the
 * oldest first (newest last = lowest) and top corners the newest first.
 */
export function layout(s: QuickView): CardView[] {
	return s.position === "topLeft" || s.position === "topRight" ? [...s.cards].reverse() : s.cards;
}

const out = (state: QuickView, ...effects: QuickEffect[]): Reduced => ({ state, effects });

function fresh(c: CardIn, keep: CardView | undefined): CardView {
	// The pointer may already be over the card, and Rust only reports a hover change, so `paused` carries over.
	const paused = keep?.paused ?? false;
	const base = {
		id: c.id,
		dismissSecs: c.dismissSecs,
		remainingMs: Math.max(0, c.dismissSecs) * 1000,
		paused,
		busy: false,
		error: null,
		errorHeld: false,
	};
	if (c.kind === "text") {
		return {
			...base,
			kind: "text",
			preview: c.preview,
			lines: c.lines,
			chars: c.chars,
			empty: c.empty,
			copied: false,
			copiedMs: 0,
			pending: null,
		};
	}
	return {
		...base,
		kind: "capture",
		entry: c.entry,
		thumb: c.thumb,
		copied: c.copied,
		copiedMs: c.copied ? COPIED_MS : 0,
		pending: null,
		sheet: null,
		progress: null,
		exporting: false,
		confirmDiscard: false,
		confirmMs: 0,
	};
}

/**
 * Merge Rust's list into the local cards. A card already shown keeps ALL its local state (countdown, hover, a
 * confirmation in progress): Rust re-sends the whole list whenever any card comes or goes, and that must not reset
 * the timers of the others. The exception is a capture Rust re-pushed with the same id (the same file captured
 * again): it is a new capture, so it starts over.
 */
function merge(s: QuickView, stack: StackIn): QuickView {
	const have = new Map(s.cards.map((c) => [c.id, c]));
	const cards = stack.cards.map((c) => {
		const old = have.get(c.id);
		if (!old) return fresh(c, undefined);
		const sameCapture = c.kind === "capture" && old.kind === "capture" && c.entry.at === old.entry.at;
		const sameText = c.kind === "text" && old.kind === "text";
		return sameCapture || sameText ? old : fresh(c, old);
	});
	return { position: stack.position, cards };
}

function patch(s: QuickView, id: string, f: (c: CardView) => CardView): QuickView {
	return { ...s, cards: s.cards.map((c) => (c.id === id ? f(c) : c)) };
}

export function reduce(state: QuickView, ev: QuickEvent): Reduced {
	switch (ev.type) {
		case "stack":
			return out(merge(state, ev.stack));
		case "tick":
			return tick(state, Math.max(0, ev.dtMs));
		case "escape":
			return escape(state);
		case "gifProgress": {
			if (!(ev.total > 0)) return out(state);
			const progress = { done: Math.min(Math.max(0, ev.done), ev.total), total: ev.total };
			return out({
				...state,
				cards: state.cards.map((c) => (c.kind === "capture" && c.sheet === "gif" ? { ...c, progress } : c)),
			});
		}
	}
	const c = state.cards.find((x) => x.id === ev.id);
	if (!c) return out(state);
	switch (ev.type) {
		case "hover":
			// Leaving the card releases the error stop; a hover-in alone never does.
			return out(
				patch(state, c.id, (x) => ({ ...x, paused: ev.on, errorHeld: ev.on ? x.errorHeld : false })),
				{ type: "hover", hovering: ev.on },
			);
		case "action":
			return c.kind === "capture" ? act(state, c, ev.name) : out(state);
		case "textAction":
			return c.kind === "text" ? actText(state, c, ev.name) : out(state);
		case "actionOk":
			return out(
				patch(state, c.id, (x) => {
					const done = { ...x, busy: false, pending: null };
					// Only a copy flashes the badge, and it does not touch the dismiss timer.
					return x.pending === "copy" ? { ...done, copied: true, copiedMs: COPIED_MS } : done;
				}),
			);
		case "actionErr":
			return out(
				patch(state, c.id, (x) => {
					const failed = { ...x, busy: false, pending: null, error: ev.message, errorHeld: true };
					return x.kind === "capture" ? { ...failed, confirmDiscard: false, confirmMs: 0 } : failed;
				}),
			);
		case "openGifSheet":
			return c.kind === "capture" && c.entry.kind === "recording"
				? out(patch(state, c.id, (x) => ({ ...x, sheet: "gif", error: null })))
				: out(state);
		case "closeSheet":
			return out(
				patch(state, c.id, (x) =>
					x.kind === "capture" ? { ...x, sheet: null, progress: null, exporting: false } : x,
				),
			);
		case "exportStart":
			return c.kind === "capture" && c.sheet === "gif"
				? out(
						patch(state, c.id, (x) => (x.kind === "capture" ? { ...x, exporting: true, progress: null } : x)),
					)
				: out(state);
		case "exportEnd":
			return out(patch(state, c.id, (x) => (x.kind === "capture" ? { ...x, exporting: false } : x)));
		case "exportCancelled":
			// Rust says "GIF export cancelled.": the person asked for it, so the sheet closes and no error shows.
			return out(
				patch(state, c.id, (x) =>
					x.kind === "capture" ? { ...x, exporting: false, sheet: null, progress: null } : x,
				),
			);
	}
}

function tick(s: QuickView, dt: number): Reduced {
	const effects: QuickEffect[] = [];
	const cards = s.cards.map((c): CardView => {
		const next = { ...c };
		if (c.copied) {
			next.copiedMs = Math.max(0, c.copiedMs - dt);
			next.copied = next.copiedMs > 0;
		}
		if (c.kind === "capture" && next.kind === "capture" && c.confirmDiscard) {
			next.confirmMs = Math.max(0, c.confirmMs - dt);
			next.confirmDiscard = next.confirmMs > 0;
		}
		if (!counting(c) || c.remainingMs === 0) return next;
		next.remainingMs = Math.max(0, c.remainingMs - dt);
		if (next.remainingMs === 0) effects.push({ type: "close", id: c.id, card: c.kind });
		return next;
	});
	return { state: { ...s, cards }, effects };
}

function escape(s: QuickView): Reduced {
	// Newest first: the card nearest the corner is the one the person is looking at.
	const newest = [...s.cards].reverse();
	const sheet = newest.find((c) => c.kind === "capture" && c.sheet !== null);
	if (sheet?.kind === "capture") {
		const closed = patch(s, sheet.id, (x) =>
			x.kind === "capture" ? { ...x, sheet: null, progress: null, exporting: false } : x,
		);
		// An encode in flight is stopped BEFORE the sheet goes away, so closing never leaves it running unseen.
		return sheet.exporting ? out(closed, { type: "cancelExport" }) : out(closed);
	}
	const armed = newest.find((c) => c.kind === "capture" && c.confirmDiscard);
	if (armed)
		return out(
			patch(s, armed.id, (x) => (x.kind === "capture" ? { ...x, confirmDiscard: false, confirmMs: 0 } : x)),
		);
	const top = newest[0];
	return top ? out(s, { type: "close", id: top.id, card: top.kind }) : out(s);
}

function act(s: QuickView, c: CaptureView, name: CaptureAction): Reduced {
	if (name === "close") return out(s, { type: "close", id: c.id, card: "capture" });
	if (c.busy) return out(s);
	// Recordings have no editor, no pin and no text to read.
	if (c.entry.kind === "recording" && (name === "annotate" || name === "pin" || name === "ocr"))
		return out(s);
	if (c.entry.kind === "gif" && (name === "pin" || name === "ocr" || name === "annotate")) return out(s);
	if (name === "discard" && !c.confirmDiscard) {
		// Deleting a file: arm first, the second click within CONFIRM_MS does it.
		return out(
			patch(s, c.id, (x) =>
				x.kind === "capture" ? { ...x, confirmDiscard: true, confirmMs: CONFIRM_MS, error: null } : x,
			),
		);
	}
	return out(
		patch(s, c.id, (x) =>
			x.kind === "capture"
				? {
						...x,
						busy: true,
						pending: name,
						error: null,
						errorHeld: false,
						confirmDiscard: false,
						confirmMs: 0,
					}
				: x,
		),
		{ type: "invoke", action: name, id: c.id, path: c.entry.path },
	);
}

function actText(s: QuickView, c: TextView, name: TextAction): Reduced {
	if (name === "close") return out(s, { type: "close", id: c.id, card: "text" });
	// An empty read has nothing to copy or send.
	if (c.busy || c.empty) return out(s);
	return out(
		patch(s, c.id, (x) =>
			x.kind === "text" ? { ...x, busy: true, pending: name, error: null, errorHeld: false } : x,
		),
		{ type: "invokeText", action: name, id: c.id },
	);
}

// --- format helpers -------------------------------------------------------------------------------------------

/** `812 B`, `1.4 KB`, `12.3 MB`, `1.02 GB` (binary units, one decimal up to MB, two for GB). */
export function humanBytes(n: number): string {
	if (!Number.isFinite(n) || n <= 0) return "0 B";
	if (n < 1024) return `${Math.floor(n)} B`;
	if (n < 1024 ** 2) return `${fixedKB(n / 1024)} KB`;
	if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
	return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

/** One decimal, but never `1024.0 KB`: a value that rounds up to the next unit's edge prints as whole KB. */
function fixedKB(kb: number): string {
	const s = kb.toFixed(1);
	return Number(s) >= 1024 ? String(Math.round(kb)) : s;
}

/** `0:07`, `1:05`, `1:02:03`. */
export function duration(secs: number): string {
	const t = Number.isFinite(secs) && secs > 0 ? Math.floor(secs) : 0;
	const h = Math.floor(t / 3600);
	const m = Math.floor((t % 3600) / 60);
	const s = t % 60;
	const ss = String(s).padStart(2, "0");
	return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** The card's title line. */
export function title(e: Entry): string {
	if (e.kind === "screenshot") return `Screenshot ${e.width} × ${e.height}`;
	if (e.kind === "recording") return e.secs == null ? "Recording" : `Recording ${duration(e.secs)}`;
	return e.secs == null ? `GIF · ${humanBytes(e.bytes)}` : `GIF ${duration(e.secs)} · ${humanBytes(e.bytes)}`;
}

/** A lucide icon name (kebab-case) per kind; the page maps it to the component. */
export function kindIcon(kind: Kind): string {
	return kind === "screenshot" ? "image" : kind === "recording" ? "video" : "film";
}

/** The text card's title: what was read, or that nothing was. */
export function textTitle(c: Pick<TextView, "empty" | "lines" | "chars">): string {
	if (c.empty) return "No text found";
	const lines = c.lines === 1 ? "1 line" : `${c.lines} lines`;
	return `Text copied · ${lines} · ${c.chars} characters`;
}
