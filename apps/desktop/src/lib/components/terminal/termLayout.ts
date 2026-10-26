/**
 * One owner of native terminal geometry per frame.
 *
 * Every GPU terminal registers its slot here. Any trigger (a slot resize, a window resize, a pane or tab
 * change) only calls `request()`; at most one `term_layout` invoke is in flight and at most one waits behind
 * it. When it runs it measures EVERY registered slot in one pass (all reads, no writes: one layout, however
 * many terminals are open) and sends all the rects in a single call. The backend applies them in one Core
 * Animation transaction after the page has presented the DOM change, then answers per tab whether the native
 * view is on screen under its slot; only then does the slot open its hole (`onPresented`).
 *
 * Nothing here knows about overlays: a menu, dialog or the sidebar peek is drawn over the terminal by the
 * page, so it is never a reason to move or hide a native view. A view is hidden only when its tab is not on
 * screen (`visible: false`).
 *
 * No DOM or Tauri imports: `invoke` and `now` are injected, so the batching rules are unit-tested.
 */

export interface SlotRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

export interface TermSlot {
	tab: string;
	/** The slot's rect in window points, or null when it is not laid out. */
	measure: () => SlotRect | null;
	/** Is this tab on screen (the active tab of a visible space)? Never about overlays. */
	visible: () => boolean;
	/** The native view is on screen under the slot (true) or no longer is (false). */
	onPresented: (presented: boolean) => void;
	/** The backend lost this terminal's view: open it again. */
	onDropped: () => void;
}

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface Placed {
	tab: string;
	presented: boolean;
	dropped: boolean;
}

/** A rect that can hold a terminal: finite, at least 1 point each way. */
export const usable = (r: SlotRect | null): r is SlotRect =>
	!!r && [r.x, r.y, r.w, r.h].every(Number.isFinite) && r.w >= 1 && r.h >= 1;

export class TermLayout {
	#slots = new Map<string, TermSlot>();
	#running = false;
	#queued = false;
	/** What each tab was last told, so an unchanged rect is not sent again. */
	#sent = new Map<string, string>();
	/** What the backend last said about each tab's view being on screen. */
	#presented = new Map<string, boolean>();
	#invoke: Invoke;

	constructor(invoke: Invoke) {
		this.#invoke = invoke;
	}

	/** Register a slot; returns the unregister function. A remounted tab replaces its old slot. */
	add(slot: TermSlot): () => void {
		this.#slots.set(slot.tab, slot);
		this.#sent.delete(slot.tab);
		return () => {
			if (this.#slots.get(slot.tab) === slot) {
				this.#slots.delete(slot.tab);
				this.#sent.delete(slot.tab);
				this.#presented.delete(slot.tab);
			}
		};
	}

	/** The backend says `tab`'s view appeared or went (`term://presented`): remember it for unchanged slots. */
	presented(tab: string, presented: boolean) {
		this.#presented.set(tab, presented);
	}

	/** Ask for a layout. Coalesces: one runs, one waits. */
	request(): Promise<void> {
		if (this.#running) {
			this.#queued = true;
			return this.#done;
		}
		return this.#run();
	}

	#done: Promise<void> = Promise.resolve();

	#run(): Promise<void> {
		this.#running = true;
		this.#done = (async () => {
			try {
				await this.#once();
			} finally {
				this.#running = false;
			}
			if (this.#queued) {
				this.#queued = false;
				await this.#run();
			}
		})();
		return this.#done;
	}

	async #once() {
		const batch: Array<{ slot: TermSlot; key: string; body: Record<string, unknown> }> = [];
		const unchanged: TermSlot[] = [];
		// One read pass over every slot: no DOM write happens between two measurements.
		for (const slot of this.#slots.values()) {
			const r = slot.measure();
			const visible = slot.visible() && usable(r);
			// An unusable rect keeps the last good one on the backend (`Placement`); it only parks when hidden.
			const body = { tab: slot.tab, x: r?.x ?? 0, y: r?.y ?? 0, w: r?.w ?? 0, h: r?.h ?? 0, visible };
			const key = `${body.x}|${body.y}|${body.w}|${body.h}|${visible}`;
			if (this.#sent.get(slot.tab) === key) {
				// Nothing to move, but the slot may be waiting on an answer (a swipe lifted over an unchanged
				// rect): repeat the last one, or it would paint over its view forever.
				unchanged.push(slot);
				continue;
			}
			batch.push({ slot, key, body });
		}
		for (const slot of unchanged) slot.onPresented(this.#presented.get(slot.tab) ?? false);
		if (!batch.length) return;
		let placed: Placed[];
		try {
			placed = (await this.#invoke("term_layout", { slots: batch.map((b) => b.body) })) as Placed[];
		} catch (e) {
			// A failed batch must not stick: the next request sends everything again.
			for (const b of batch) this.#sent.delete(b.slot.tab);
			throw e;
		}
		const byTab = new Map(placed.map((p) => [p.tab, p]));
		for (const b of batch) {
			const p = byTab.get(b.slot.tab);
			if (!p || p.dropped) {
				// Never remember a rect for a view that is gone: the reopen must send it.
				this.#sent.delete(b.slot.tab);
				this.#presented.set(b.slot.tab, false);
				b.slot.onPresented(false);
				if (p?.dropped) b.slot.onDropped();
				continue;
			}
			this.#sent.set(b.slot.tab, b.key);
			this.#presented.set(b.slot.tab, p.presented);
			b.slot.onPresented(p.presented);
		}
	}
}
