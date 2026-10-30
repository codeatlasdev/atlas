/**
 * The clock of a deck scene: a handful of discrete beats per loop (a line typed, a service up, a branch merged), each
 * held for its duration, then back to 0. Elements switch on `n >= k` and CSS transitions do the motion, so the page
 * changes state a few times a second at most and the compositor animates the rest. Reduced motion: the last beat, still.
 */
export class Beats {
	n = $state(0);
	readonly last: number;
	#timer: ReturnType<typeof setTimeout> | undefined;
	#hold: readonly number[];

	/** `hold[k]` = ms beat k lasts before the next; after the last one the loop starts over at 0. */
	constructor(hold: readonly number[]) {
		this.#hold = hold;
		this.last = hold.length - 1;
	}

	start(reduced = matchMedia("(prefers-reduced-motion: reduce)").matches): () => void {
		if (reduced) {
			this.n = this.last;
			return () => {};
		}
		const step = () => {
			this.#timer = setTimeout(() => {
				this.n = this.n === this.last ? 0 : this.n + 1;
				step();
			}, this.#hold[this.n]);
		};
		this.n = 0;
		step();
		return () => clearTimeout(this.#timer);
	}
}
