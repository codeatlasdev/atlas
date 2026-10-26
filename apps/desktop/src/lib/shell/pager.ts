/**
 * Space pager: places the space layers for the frames of the native swipe engine (`swipe_engine.rs`).
 *
 * The engine is the only source of truth for motion and decisions (claim, 1:1 tracking, rubber band, release
 * decision, critically damped settle, programmatic `go`). It stamps every frame and decision with a generation
 * (`seq`, one per gesture) and this module only
 *
 *  - writes `transform` on the mounted layers for each frame (no Svelte state, no animations, no timers),
 *  - applies each commit exactly once (`seq` guard + `shell.commitSpace` is itself idempotent),
 *  - lets Svelte apply the committed selection before dropping the end pose at rest.
 *
 *   amount  fraction of the content width the *origin* layer has moved toward `dir` (+1 = next). The origin
 *           sits at `-amount·W`, the neighbour in the direction of travel at `(dir - amount)·W`. A gesture that
 *           preempts a settle is rebased by the engine on the page being settled to: the origin is simply
 *           whatever is selected when the new generation begins.
 */

export type Dir = 1 | -1;

export interface FrameMsg {
	zone: string;
	id?: string;
	seq: number;
	amount: number;
	stage: "track" | "settle" | "rest";
}

export interface CommitMsg {
	zone: string;
	id?: string;
	seq: number;
	dir: number;
}

export interface PagerHost {
	/** Layer elements keyed by space id (current + neighbours that are mounted). */
	layer(id: string): HTMLElement | undefined;
	/** Width of the pager viewport in CSS px. */
	width(): number;
	/** Space ids by sidebar order, and the active one. */
	ids(): string[];
	active(): string | null;
	/** Switch the selection to `id` right now (no animation of its own). */
	commit(id: string): void;
	/** Flip per-gesture state: natives parked, `keep` layers mounted and unhidden. Never called per frame. */
	moving(on: boolean, keep?: string[]): void;
	/** Resolves once Svelte has applied the pending state (after `commit`). */
	flush(): Promise<void>;
	/** The engine's own programmatic switch (`swipe_go`): its spring and clock. False = refused (fingers down). */
	engineGo(dir: Dir): Promise<boolean>;
}

export interface Placement {
	id: string;
	/** Translation in CSS px. */
	x: number;
}

/** Neighbour of `id` in the sidebar order, wrapping; null with fewer than two spaces. */
function around(ids: string[], id: string, dir: Dir): string | null {
	const i = ids.indexOf(id);
	return i < 0 || ids.length < 2 ? null : (ids[(i + dir + ids.length) % ids.length] ?? null);
}

/**
 * Where each layer goes for an engine `amount` relative to `origin`: the origin at `-amount·W`, the incoming
 * layer (`target`, else the neighbour in the direction of `amount`) at `(dir - amount)·W`. At rest only the
 * origin. Pure.
 */
export function place(
	origin: string,
	amount: number,
	ids: string[],
	w: number,
	target?: string,
): Placement[] {
	const out: Placement[] = [{ id: origin, x: -amount * w }];
	if (amount === 0) return out;
	const dir: Dir = amount > 0 ? 1 : -1;
	const inc = target ?? around(ids, origin, dir);
	if (inc && inc !== origin) out.push({ id: inc, x: (dir - amount) * w });
	return out;
}

class Pager {
	#host: PagerHost | null = null;
	/** Generation being drawn, and the layers it moved (to release the ones a heading flip leaves). */
	#seq = 0;
	#origin: string | null = null;
	/** An explicit incoming layer: a jump to a non-adjacent space (⌘3, click, history). */
	#target: string | null = null;
	#moved = new Set<string>();
	#active = false;
	/** What the host was last told it must keep mounted (`moving(true, …)`): told again only when it changes. */
	#told = "";
	/** Highest generation whose decision was applied. */
	#applied = 0;
	/** The state change was already made by the caller (`go` selects first). */
	#selfCommitted = false;
	/** A programmatic switch waiting for its generation: its target and origin. */
	#pending: { target: string; origin: string; selfCommitted: boolean } | null = null;

	get active() {
		return this.#active;
	}

	/** Layers the current gesture keeps on screen (empty at rest). */
	get kept(): string[] {
		return this.#active && this.#told ? this.#told.split("\u0000") : [];
	}

	attach(host: PagerHost) {
		this.#host = host;
		// A fresh mount (or HMR) starts clean; the engine's generations only ever grow, so 0 accepts all.
		this.#seq = this.#applied = 0;
		this.#moved.clear();
		return () => {
			this.#abort();
			this.#host = null;
		};
	}

	/** One engine frame of the space zone. Frames of an older generation are dropped. */
	frame(m: FrameMsg) {
		const h = this.#host;
		if (!h || m.seq < this.#seq) return;
		if (m.seq > this.#seq) this.#begin(m.seq);
		if (!this.#origin) return;
		if (m.stage === "rest") return void this.#rest(m.seq);
		const ids = h.ids();
		const next = place(this.#origin, m.amount, ids, h.width(), this.#target ?? undefined);
		const keep = new Set(next.map((p) => p.id));
		for (const id of this.#moved) if (!keep.has(id)) this.#release(id);
		for (const p of next) {
			const el = h.layer(p.id);
			if (!el) continue;
			el.style.transform = `translate3d(${p.x}px,0,0)`;
			this.#moved.add(p.id);
		}
		const told = [...keep].join("\u0000");
		if (!this.#active || told !== this.#told) {
			this.#told = told;
			h.moving(true, [...keep]);
		}
		this.#active = true;
	}

	/** The release decision: once per generation. `dir` 0 is a cancel. */
	commit(m: CommitMsg) {
		const h = this.#host;
		if (!h || m.seq <= this.#applied) return;
		this.#applied = m.seq;
		if (m.dir === 0 || !this.#origin || m.seq !== this.#seq) return;
		const to = this.#target ?? around(h.ids(), this.#origin, m.dir > 0 ? 1 : -1);
		if (to && !this.#selfCommitted) h.commit(to);
	}

	/**
	 * Keyboard / click / history / ctl: switch to `id`. The selection moves now (keyboard consumers see it at
	 * once) and the engine animates it with its own spring. With `origin` the selection already moved (history
	 * picks its own worktree): only animate. Returns whether a switch happened.
	 */
	go(id: string, origin?: string) {
		const h = this.#host;
		const from = origin ?? h?.active();
		if (!h || !from || from === id) return false;
		const ids = h.ids();
		if (ids.indexOf(id) < 0 || ids.indexOf(from) < 0) return false;
		const dir: Dir = ids.indexOf(id) >= ids.indexOf(from) ? 1 : -1;
		this.#pending = { target: id, origin: from, selfCommitted: !origin };
		if (!origin) h.commit(id);
		void h.engineGo(dir).then((took) => {
			// The engine refused (fingers on the pad): nothing animates, the selection already moved.
			if (!took && this.#pending?.target === id) this.#pending = null;
		});
		return true;
	}

	#begin(seq: number) {
		const h = this.#host!;
		// A new generation while one still holds its end pose: its transforms are replaced below, so just
		// forget them (Svelte applied the commit when the new origin was selected).
		this.#seq = seq;
		const p = this.#pending;
		this.#pending = null;
		this.#target = p?.target ?? null;
		this.#origin = p?.origin ?? h.active();
		this.#selfCommitted = p?.selfCommitted ?? false;
		for (const id of this.#moved) this.#release(id);
		this.#moved.clear();
	}

	#release(id: string) {
		this.#host?.layer(id)?.style.removeProperty("transform");
		this.#moved.delete(id);
	}

	/** Rest: let Svelte apply the committed selection, drop the transforms, then put natives back. */
	async #rest(seq: number) {
		const h = this.#host;
		if (!h || !this.#active) return;
		this.#active = false;
		this.#told = "";
		// The end pose is held while Svelte applies the new roles; once the rest layout is in the DOM it is
		// dropped, so no frame shows an in-between state.
		await h.flush();
		if (seq !== this.#seq || this.#active) return; // a newer generation took over meanwhile
		for (const id of [...this.#moved]) this.#release(id);
		h.moving(false);
	}

	/** Abort without animation (detach). */
	#abort() {
		for (const id of [...this.#moved]) this.#release(id);
		if (this.#active) this.#host?.moving(false);
		this.#active = false;
		this.#told = "";
		this.#origin = this.#target = this.#pending = null;
	}
}

export const spacePager = new Pager();
