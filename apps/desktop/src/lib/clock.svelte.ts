/**
 * One shared, visibility-gated clock. Every relative-time label ("3m ago") and every poller used to own a
 * `setInterval` (Panel, ActivityDrawer, SetupRow, tray, ActionOutputDrawer, SourceControl, PrTab, ChatTab ...), so an idle
 * app with a hidden window still woke JS many times a minute per project.
 *
 * Here there is at most ONE timer, at the finest interval anyone asked for, and none while nobody can see the
 * page: the document is hidden, or the window has been blurred longer than `blurGraceMs` (a quick alt-tab keeps
 * the labels live; a window left behind for minutes sleeps). Coming back ticks every subscriber once at once, since
 * their data is stale by then.
 */

import { onDestroy } from "svelte";

export interface ClockEnv {
	now(): number;
	setInterval(fn: () => void, ms: number): unknown;
	clearInterval(id: unknown): void;
	setTimeout(fn: () => void, ms: number): unknown;
	clearTimeout(id: unknown): void;
	visible(): boolean;
	focused(): boolean;
	onVisibility(fn: () => void): () => void;
	onFocus(fn: () => void): () => void;
	onBlur(fn: () => void): () => void;
	blurGraceMs: number;
}

interface Sub {
	fn: () => void;
	ms: number;
	/** Ticks of the shared timer since this subscriber last ran. */
	due: number;
}

export function createClock(env: ClockEnv) {
	const subs = new Set<Sub>();
	let timer: unknown;
	let timerMs = 0;
	let grace: unknown;
	/** Blurred long enough to sleep. */
	let asleep = false;
	let stamp = $state(env.now());

	const awake = () => env.visible() && !asleep;
	const step = () => Math.min(...[...subs].map((s) => s.ms));

	function run(s: Sub) {
		try {
			s.fn();
		} catch (e) {
			console.error("clock subscriber failed", e);
		}
	}

	function tick() {
		stamp = env.now();
		for (const s of subs) {
			s.due += timerMs;
			if (s.due >= s.ms) {
				s.due = 0;
				run(s);
			}
		}
	}

	/** Make the timer match the subscribers and the visibility: none, or one at the finest interval. */
	function sync() {
		const want = subs.size && awake() ? step() : 0;
		if (want === timerMs) return;
		if (timer !== undefined) env.clearInterval(timer);
		timer = want ? env.setInterval(tick, want) : undefined;
		timerMs = want;
	}

	function wake() {
		asleep = false;
		stamp = env.now();
		sync();
		for (const s of subs) {
			s.due = 0;
			run(s);
		}
	}

	env.onVisibility(() => {
		if (env.visible()) wake();
		else sync();
	});
	env.onBlur(() => {
		if (grace !== undefined) env.clearTimeout(grace);
		grace = env.setTimeout(() => {
			grace = undefined;
			if (env.focused()) return;
			asleep = true;
			sync();
		}, env.blurGraceMs);
	});
	env.onFocus(() => {
		if (grace !== undefined) env.clearTimeout(grace);
		grace = undefined;
		if (asleep) wake();
	});

	return {
		/** Reactive "now", refreshed on every tick while someone is subscribed and visible. */
		get now() {
			return stamp;
		},
		/** Re-read "now" immediately (a window re-shown without a visibility change). */
		refresh() {
			stamp = env.now();
		},
		/** Call `fn` about every `ms` while the page is visible. Returns the unsubscribe function. */
		every(ms: number, fn: () => void): () => void {
			const s: Sub = { fn, ms, due: 0 };
			subs.add(s);
			sync();
			return () => {
				subs.delete(s);
				sync();
			};
		},
	};
}

/** The clock's public surface, so a poller can be driven by a test clock. */
type Clock = ReturnType<typeof createClock>;

/**
 * A poll that runs on the shared clock only while its gate is open (its tab is the visible one), and polls once
 * the moment the gate opens: whatever it shows is stale by then. `stop()` ends it for good (component teardown).
 */
export function createPoller(c: Clock, ms: number, fn: () => void) {
	let off: (() => void) | undefined;
	return {
		gate(open: boolean) {
			if (open === !!off) return;
			if (open) {
				off = c.every(ms, fn);
				fn();
			} else {
				off?.();
				off = undefined;
			}
		},
		stop() {
			off?.();
			off = undefined;
		},
	};
}

/**
 * Poll `fn` every `ms` while `open()` is true and the page is visible. Call during component init; `open` is read
 * reactively (e.g. `() => visible`), and the poll ends with the component.
 */
export function usePoll(ms: number, open: () => boolean, fn: () => void, c: Clock = clock) {
	const p = createPoller(c, ms, fn);
	$effect(() => p.gate(open()));
	// Teardown only: an effect cleanup would also run on every change of `open()`.
	onDestroy(p.stop);
}

/**
 * `Date.now()` that stays fresh while this component lives, refreshed about every `ms` and only while the page is
 * visible. Call during component init: `const t = useNow(15_000)`, then read `t.now`.
 */
export function useNow(ms: number, c = clock) {
	$effect(() => c.every(ms, () => {}));
	return c;
}

/** The app's clock, bound to the real document and window. */
export const clock = createClock({
	now: () => Date.now(),
	setInterval: (fn, ms) => window.setInterval(fn, ms),
	clearInterval: (id) => window.clearInterval(id as number),
	setTimeout: (fn, ms) => window.setTimeout(fn, ms),
	clearTimeout: (id) => window.clearTimeout(id as number),
	visible: () => document.visibilityState !== "hidden",
	focused: () => document.hasFocus(),
	onVisibility: (fn) => (
		document.addEventListener("visibilitychange", fn),
		() => document.removeEventListener("visibilitychange", fn)
	),
	onFocus: (fn) => (window.addEventListener("focus", fn), () => window.removeEventListener("focus", fn)),
	onBlur: (fn) => (window.addEventListener("blur", fn), () => window.removeEventListener("blur", fn)),
	blurGraceMs: 60_000,
});
