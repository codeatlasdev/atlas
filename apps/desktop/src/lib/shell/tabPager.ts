/**
 * Tab pager: the tab-group counterpart of `pager.ts`. The native engine reports engine frames and one
 * decision per generation for a tab group; the active pane and the tab it would land on move together, 1:1
 * with the fingers (`amount` = fraction of the body width, +1 = next). Panes stay laid out when hidden
 * (`visibility:hidden`), so the incoming one only needs to be shown. A neighbour that was never mounted (lazy
 * tabs) is simply not there: the active pane slides over the empty body.
 *
 * Same contract as the space pager: frames of an older generation are dropped, each decision applies once
 * (`activate` is only called for `seq` above the last applied one), and the end pose is cleared on `rest`.
 */
import type { CommitMsg, FrameMsg } from "#lib/shell/pager.ts";

export interface TabHost {
	pane(id: string): HTMLElement | undefined;
	strip(): HTMLElement | null;
	/** Body width in CSS px (one page). */
	width(): number;
	/** Tab ids in strip order and the active one. */
	order(): string[];
	active(): string | undefined;
	activate(id: string): void;
}

/** Pure: pane translations for an engine `amount` (the active pane + the neighbour in the direction of travel). */
export function placeTabs(order: string[], active: string, amount: number, w: number) {
	const out = [{ id: active, x: -amount * w }];
	if (amount === 0) return out;
	const dir = amount > 0 ? 1 : -1;
	const next = order[order.indexOf(active) + dir];
	if (next) out.push({ id: next, x: (dir - amount) * w });
	return out;
}

/**
 * Native views (terminal, browser) cannot follow a CSS transform and nothing crosses IPC per frame, so every pane in
 * flight carries `data-swiping` for the generation: `overlayOpen` parks the native view under it, and lifting the mark
 * makes `watchSlot` put the view back at the pane's untransformed rect.
 */
function release(el: HTMLElement) {
	el.style.transform = el.style.visibility = "";
	el.toggleAttribute("data-swiping", false);
}

/** The strip nudges a quarter of the drag, capped, so the finger reads as moving the whole group. */
const STRIP_NUDGE = 24;

interface GroupState {
	seq: number;
	applied: number;
	/** Active tab when this generation began: the origin its amounts are relative to. */
	origin: string | undefined;
	moved: Set<string>;
	active: boolean;
}

class TabPager {
	#groups = new Map<string, GroupState>();

	reset() {
		this.#groups.clear();
	}

	frame(group: string, h: TabHost, m: FrameMsg) {
		let g = this.#groups.get(group);
		if (!g)
			this.#groups.set(
				group,
				(g = { seq: 0, applied: 0, origin: undefined, moved: new Set(), active: false }),
			);
		if (m.seq < g.seq) return;
		if (m.seq > g.seq) {
			g.seq = m.seq;
			g.origin = h.active();
			this.#clear(g, h);
		}
		if (!g.origin) return;
		if (m.stage === "rest") {
			if (g.active) this.#clear(g, h);
			g.active = false;
			return;
		}
		const next = placeTabs(h.order(), g.origin, m.amount, h.width());
		const keep = new Set(next.map((p) => p.id));
		for (const id of g.moved) {
			if (keep.has(id)) continue;
			const el = h.pane(id);
			if (el) release(el);
		}
		g.moved = keep;
		for (const [i, p] of next.entries()) {
			const el = h.pane(p.id);
			if (!el) continue;
			el.style.transform = `translate3d(${p.x}px,0,0)`;
			el.style.visibility = i === 0 ? "" : "visible";
			el.toggleAttribute("data-swiping", true);
		}
		const strip = h.strip();
		if (strip)
			strip.style.transform = `translate3d(${-Math.max(-1, Math.min(1, m.amount)) * STRIP_NUDGE}px,0,0)`;
		g.active = true;
	}

	commit(group: string, h: TabHost, m: CommitMsg) {
		const g = this.#groups.get(group);
		if (!g || m.seq <= g.applied) return;
		g.applied = m.seq;
		if (m.dir === 0 || m.seq !== g.seq || !g.origin) return;
		const to = h.order()[h.order().indexOf(g.origin) + (m.dir > 0 ? 1 : -1)];
		if (to) h.activate(to);
	}

	#clear(g: GroupState, h: TabHost) {
		for (const id of g.moved) {
			const el = h.pane(id);
			if (el) release(el);
		}
		g.moved.clear();
		const strip = h.strip();
		if (strip) strip.style.transform = "";
	}
}

export const tabPager = new TabPager();
