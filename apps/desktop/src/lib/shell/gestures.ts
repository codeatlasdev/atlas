/**
 * App-level trackpad gestures (Arc), built on `swipe()`:
 *
 *  - space:    two-finger swipe over the sidebar → previous / next space. The content follows
 *              the finger (rubber-banded) and the next space's accent shows as an edge hint.
 *  - tabs:     swipe over the tab strip → previous / next tab (lighter travel).
 *  - history:  swipe over the content where nothing can scroll → Back / Forward, with an arrow
 *              chip that fills as you drag.
 *
 * Live values are written inline (`transform` on the moving elements, `--hint-accent` on the
 * edge hint); nothing here touches Svelte state until a gesture commits.
 */
import { shell } from "#lib/shell/shell.svelte.ts";
import { neighbour } from "#lib/shell/space.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import type { GestureView } from "#lib/shell/gesture.ts";
import type { SwipeOptions } from "#lib/shell/swipe.ts";

const content = () => document.querySelector<HTMLElement>("main.content");

/**
 * Mirror a live view. Per event only `transform` (and the chip fill) is written, straight on the
 * elements that move: a transform is compositor-only, while a custom property on the content
 * root would restyle every descendant (logs, editors, the panel) on each wheel event. The root's
 * `data-<name>` attribute, which switches the CSS to tracking mode, changes only with the direction.
 */
function mirror(name: "space" | "nav", v: GestureView, maxPx: number) {
	const el = content();
	if (!el) return;
	const view = el.querySelector<HTMLElement>(".space-view");
	const chips = el.querySelectorAll<HTMLElement>(".nav-chip");
	if (v.phase === "idle") {
		if (el.dataset[name] !== undefined) delete el.dataset[name];
		if (view) view.style.transform = "";
		if (name === "nav") for (const c of chips) ((c.style.transform = ""), (c.style.background = ""));
		return;
	}
	const state = v.dir > 0 ? "next" : v.dir < 0 ? "prev" : "";
	if (el.dataset[name] !== state) el.dataset[name] = state;
	const x = Math.max(-maxPx, Math.min(maxPx, v.offset));
	if (view) view.style.transform = `translate3d(${name === "nav" ? x * 0.35 : x}px,0,0)`;
	if (name === "nav")
		for (const c of chips) {
			const nudge = c.classList.contains("nav-chip-prev") ? -4 : 4;
			c.style.transform = `translate3d(${x * 0.5 + nudge}px,0,0)`;
			c.style.background = `color-mix(in oklab, var(--space-accent) ${Math.round(v.progress * 70)}%, var(--ca-surface-raised))`;
		}
}

/** Accent of the space a swipe would land on; shown as an edge hint while dragging. */
let hintDir: 1 | -1 | 0 = 0;
function hintAccent(dir: 1 | -1 | 0) {
	if (dir === hintDir) return; // only on a direction change, not per frame
	hintDir = dir;
	const el = content();
	if (!el) return;
	const to = dir ? neighbour(shell.spaceIds, shell.activeId, dir) : null;
	const edge = el.querySelector<HTMLElement>(".space-edge") ?? el;
	if (to) edge.style.setProperty("--hint-accent", shell.accent(to));
	else edge.style.removeProperty("--hint-accent");
}

/** Space switching: two-finger swipe over the sidebar. */
export function spaceSwipe(): SwipeOptions {
	return {
		commitFrac: 0.3,
		can: (d) => neighbour(shell.spaceIds, shell.activeId, d) !== null,
		onView: (v) => {
			mirror("space", v, 120);
			hintAccent(v.phase === "idle" ? 0 : v.dir);
		},
		onEnd: (end) => end.kind === "commit" && shell.cycleSpace(end.dir),
	};
}

/** Tab switching: two-finger swipe over a tab strip. `ids` are the group's tabs in order. */
export function tabSwipe(ids: () => string[], active: () => string | undefined): SwipeOptions {
	const target = (d: 1 | -1) => {
		const all = ids();
		const i = all.indexOf(active() ?? "");
		return i < 0 ? undefined : all[i + d];
	};
	return {
		commitFrac: 0.18,
		respectScrollers: true,
		flickSpeed: 0.7,
		can: (d) => target(d) !== undefined,
		onView: (v, el) => {
			// The strip nudges with the fingers; nothing else moves.
			el.style.transform =
				v.phase === "idle" ? "" : `translate3d(${Math.max(-48, Math.min(48, v.offset * 0.25))}px,0,0)`;
		},
		onEnd: (end) => {
			const id = end.kind === "commit" ? target(end.dir) : undefined;
			if (id) ws.activate(id);
		},
	};
}

/** Back / Forward: swipe over the content where nothing can scroll horizontally (Safari's gesture). */
export function historySwipe(): SwipeOptions {
	return {
		commitFrac: 0.22,
		respectScrollers: true,
		can: (d) => shell.canGo(d),
		onView: (v) => mirror("nav", v, 72),
		onEnd: (end) => end.kind === "commit" && shell.go(end.dir),
	};
}
