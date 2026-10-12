/**
 * DOM side of the trackpad gestures: one non-passive `wheel` listener per surface feeds a
 * `Gesture` and reports live views through `onView`, so a swipe costs no Svelte reactivity per
 * event (consumers write CSS variables straight to elements). Only the end calls `onEnd`.
 *
 * With `prefers-reduced-motion` nothing tracks: `onView` is never called, the decision still
 * happens on release and the switch is instant.
 */
import {
	createGesture,
	IDLE,
	type Gesture,
	type GestureEnd,
	type GestureOptions,
	type GestureView,
} from "#lib/shell/gesture.ts";
import { scrollsHorizontally } from "#lib/shell/scrollable.ts";

export interface SwipeOptions extends GestureOptions {
	/** Once per gesture end. `dir` 1 = next (fingers moved left), -1 = previous. */
	onEnd: (end: Exclude<GestureEnd, null>, el: HTMLElement) => void;
	/** Every visible change while tracking, then once with an idle view at the end. Keep it cheap. */
	onView?: (v: GestureView, el: HTMLElement) => void;
	/** When set, a gesture only starts if nothing between the target and the surface can scroll horizontally. */
	respectScrollers?: boolean;
}

const zoneOf = (t: EventTarget | null) => (t instanceof Element ? t.closest("[data-gesture-zone]") : null);

/** Svelte action: `use:swipe={options}`. */
export function swipe(el: HTMLElement, options: SwipeOptions) {
	let o = options;
	// Options are read through `o` per call so an update never resets a swipe in flight.
	const g: Gesture = createGesture({ ...options, can: (d) => o.can?.(d) ?? true });
	const motion = window.matchMedia("(prefers-reduced-motion: no-preference)");
	let raf = 0;
	/** Latest view not yet painted: wheel events can outnumber frames, paint once per frame. */
	let pending: GestureView | null = null;

	function frame(now: number) {
		raf = 0;
		if (pending && motion.matches) o.onView?.(pending, el);
		pending = null;
		const end = g.poll(now);
		if (end) {
			if (motion.matches) o.onView?.(IDLE, el);
			o.onEnd(end, el);
		} else if (g.active) raf = requestAnimationFrame(frame);
	}

	function onwheel(e: WheelEvent) {
		if (e.ctrlKey) return; // pinch-zoom arrives as ctrl+wheel: never a swipe
		const mine = g.active;
		if (!mine && Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return; // vertical scroll: not ours
		if (o.respectScrollers && !mine && scrollsHorizontally(e.target, el, e.deltaX)) return;
		// Nested surfaces (tab strip inside the content) own their own events: the nearest zone must be this one.
		if (!mine && zoneOf(e.target) !== el.closest("[data-gesture-zone]")) return;
		const v = g.feed(e.deltaX, e.deltaY, e.timeStamp, el.clientWidth);
		if (g.active) {
			e.preventDefault(); // no overscroll or native history swipe underneath
			e.stopPropagation(); // and no ancestor surface (history swipe) tracks the same fingers
		}
		if (v) pending = v;
		if (!g.active && !v) return;
		if (!raf) raf = requestAnimationFrame(frame);
	}

	el.addEventListener("wheel", onwheel, { passive: false });
	return {
		update(next: SwipeOptions) {
			o = next;
		},
		destroy() {
			el.removeEventListener("wheel", onwheel);
			cancelAnimationFrame(raf);
			g.cancel();
			o.onView?.(IDLE, el);
		},
	};
}
