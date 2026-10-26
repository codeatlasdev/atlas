/**
 * Variable-height windowing for `VirtualList`: item heights (an estimate until measured), their prefix sums, and
 * the slice of items that intersects a viewport. Pure, so the arithmetic is unit-tested without a DOM.
 *
 * Measured heights are remembered per key (when the list gives keys), so items prepended to, trimmed from or
 * re-ordered in the list keep their measure instead of snapping back to the estimate.
 */

/** Sub-pixel noise from a ResizeObserver is not a layout change. */
const EPS = 0.5;

export class Heights {
	readonly estimate: number;
	#n = 0;
	#keys: readonly string[] | undefined;
	/** Measured height by key (keyed lists) or by index (unkeyed). */
	#byKey = new Map<string | number, number>();
	/** `#tops[i]` = top of item i; `#tops[n]` = total. Valid for indexes below `#valid`. */
	#tops: number[] = [0];
	#valid = 0;

	constructor(estimate: number) {
		this.estimate = estimate;
	}

	get length() {
		return this.#n;
	}

	get total(): number {
		this.#fill(this.#n);
		return this.#tops[this.#n];
	}

	#id(i: number): string | number {
		return this.#keys ? this.#keys[i] : i;
	}

	#height(i: number): number {
		return this.#byKey.get(this.#id(i)) ?? this.estimate;
	}

	#fill(upTo: number) {
		const t = this.#tops;
		for (let i = this.#valid; i < upTo; i++) t[i + 1] = t[i] + this.#height(i);
		if (upTo > this.#valid) this.#valid = upTo;
	}

	/** The list now has `n` items (with these keys). Forgets measures of keys that left. */
	resize(n: number, keys?: readonly string[]) {
		// Called from a derived that also re-runs on every scroll: the same list must cost nothing.
		if (n === this.#n && keys === this.#keys) return;
		if (keys) {
			const alive = new Set(keys);
			for (const k of this.#byKey.keys()) if (!alive.has(k as string)) this.#byKey.delete(k);
		} else if (n < this.#n) {
			for (let i = n; i < this.#n; i++) this.#byKey.delete(i);
		}
		this.#n = n;
		this.#keys = keys;
		// Keyed lists may have changed anywhere; unkeyed ones only past the old end.
		this.#valid = keys ? 0 : Math.min(this.#valid, n);
		this.#tops.length = Math.max(1, Math.min(this.#tops.length, this.#valid + 1));
	}

	/** Record the measured height of item `i`. Returns true when the layout below it moved. */
	set(i: number, h: number): boolean {
		if (i < 0 || i >= this.#n) return false;
		if (Math.abs(this.#height(i) - h) < EPS) return false;
		this.#byKey.set(this.#id(i), h);
		if (i < this.#valid) {
			this.#valid = i;
			this.#tops.length = i + 1;
		}
		return true;
	}

	top(i: number): number {
		const k = Math.max(0, Math.min(i, this.#n));
		this.#fill(k);
		return this.#tops[k];
	}

	/** Index of the item that contains offset `y` (the last item for `y` past the end). */
	at(y: number): number {
		if (this.#n === 0) return 0;
		this.#fill(this.#n);
		let lo = 0;
		let hi = this.#n - 1;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (this.#tops[mid + 1] <= y) lo = mid + 1;
			else hi = mid;
		}
		return lo;
	}
}

/** Items `[first, last)` to render for a viewport at `scrollTop` of height `viewH`, with `overscan` extra items each side. */
export function windowFor(h: Heights, scrollTop: number, viewH: number, overscan: number) {
	if (h.length === 0) return { first: 0, last: 0 };
	const first = Math.max(0, h.at(scrollTop) - overscan);
	const last = Math.min(h.length, h.at(scrollTop + viewH) + 1 + overscan);
	return { first, last };
}
