/**
 * Quick Open's file index: the ranking of `fuzzy(query, name, dir)` over a whole worktree's file list, built once per
 * list instead of once per keystroke.
 *
 * What the palette used to do on every key: build an item (with its icon markup) for each of the 100k files, run `fuzzy`
 * on each (two `toLowerCase` and a template string per call), sort the matches and keep 80. Here the lowercased
 * strings are made once, a query that extends the previous one only looks at what the previous one matched (a
 * subsequence match of "abc" is a match of "ab"), and only the best `limit` are kept while scanning. The score and the
 * order (score, then list order) are those of `fuzzy`, which `quickOpen.test.ts` checks against it.
 */

/** `[\s/\-_.]` of fuzzy.ts, as char codes. */
function isSep(c: number): boolean {
	if (c === 47 || c === 45 || c === 95 || c === 46 || c <= 32) return true;
	return c >= 0x80 && /\s/.test(String.fromCharCode(c));
}

/** The subsequence score of fuzzy.ts for `s` from offset `from` (0 = no match): one pass over char codes. */
function score(q: string, s: string, from: number): number {
	const n = s.length;
	const m = q.length;
	let total = 0;
	let run = 0;
	let j = 0;
	let want = q.charCodeAt(0);
	for (let i = from; i < n; i++) {
		const c = s.charCodeAt(i);
		if (c !== want) {
			run = 0;
			continue;
		}
		run++;
		total += 1 + run * 2 + (i === from || isSep(s.charCodeAt(i - 1)) ? 5 : 0);
		if (++j === m) return total;
		want = q.charCodeAt(j);
	}
	return 0;
}

/** The best score a path-only match of `m` characters can reach: every hit extends the run (`1 + 2 * run + 5` each). */
const maxPathScore = (m: number) => 6 * m + m * (m + 1);

/** Characters present, as a bit set (`code & 31`: letters exactly, others may collide, which only costs a real check). */
function maskOf(s: string, from = 0): number {
	let mask = 0;
	for (let c = from; c < s.length; c++) mask |= 1 << (s.charCodeAt(c) & 31);
	return mask;
}

export class FileIndex {
	/** `dir/name` lowercased (`/name` for a file at the root, as `fuzzy` sees it with no directory). */
	readonly #all: string[];
	/** Where the name starts in `#all[i]`. */
	readonly #nameAt: Int32Array;
	/** `dir.length + name.length` of the original strings: the tie-break `fuzzy` subtracts. */
	readonly #size: Int32Array;
	/** `name.length`, for the bonus a name match gets. */
	readonly #nameLen: Int32Array;
	/** Characters in the path / in the name: a query with a character a file lacks cannot match there. */
	readonly #mask: Int32Array;
	readonly #nameMask: Int32Array;
	/**
	 * The last query and what a longer one can start from (list order). `complete`: every file the query matches
	 * anywhere; else only the ones it matches in the name, which is all a longer query needs while the best matches
	 * are name matches (see `top`).
	 */
	#last: { q: string; hits: Int32Array; complete: boolean } | undefined;

	constructor(readonly files: readonly string[]) {
		const n = files.length;
		this.#all = new Array<string>(n);
		this.#nameAt = new Int32Array(n);
		this.#size = new Int32Array(n);
		this.#nameLen = new Int32Array(n);
		this.#mask = new Int32Array(n);
		this.#nameMask = new Int32Array(n);
		for (let i = 0; i < n; i++) {
			const f = files[i];
			const slash = f.lastIndexOf("/");
			const nameLen = f.length - slash - 1;
			const all = (slash > 0 ? f : `/${f.slice(slash + 1)}`).toLowerCase();
			this.#all[i] = all;
			this.#nameAt[i] = all.length - nameLen;
			this.#mask[i] = maskOf(all);
			this.#nameMask[i] = maskOf(all, all.length - nameLen);
			this.#nameLen[i] = nameLen;
			this.#size[i] = (slash > 0 ? slash : 0) + nameLen;
		}
	}

	/** Indexes into `files` of the best `limit` matches of `query`, best first; the first `limit` files for an empty query. */
	top(query: string, limit: number): number[] {
		const q = query.toLowerCase().replace(/\s+/g, "");
		if (!q) return Array.from({ length: Math.min(limit, this.files.length) }, (_, i) => i);
		const qMask = maskOf(q);
		const prev = this.#last;
		const from = prev && q.startsWith(prev.q) ? prev : undefined;

		// A match in the name outranks any match in the path (1000 + ...): when enough names match, the path-only
		// matches cannot make the cut and only the names are looked at.
		const named = this.#rank(q, qMask, limit, from?.hits, true);
		if (named.best.length >= limit && named.scores[limit - 1] > maxPathScore(q.length)) {
			this.#last = { q, hits: named.hits, complete: false };
			return named.best;
		}
		const all = this.#rank(q, qMask, limit, from?.complete ? from.hits : undefined, false);
		this.#last = { q, hits: all.hits, complete: true };
		return all.best;
	}

	/** Scan `source` (default: every file) for matches of `q`: names only, or names and paths. */
	#rank(q: string, qMask: number, limit: number, source: Int32Array | undefined, namesOnly: boolean) {
		const count = source ? source.length : this.files.length;
		const hits = new Int32Array(count);
		let nHits = 0;
		const best: number[] = [];
		const scores: number[] = [];
		for (let k = 0; k < count; k++) {
			const i = source ? source[k] : k;
			const mask = namesOnly ? this.#nameMask[i] : this.#mask[i];
			if (qMask & ~mask) continue;
			const all = this.#all[i];
			// The name is the tail of `all`: a name match is also a path match, so the (longer) path is only scanned
			// when the name did not match.
			const inName = score(q, all, this.#nameAt[i]);
			const inAll = inName || namesOnly ? 1 : score(q, all, 0);
			if (!inAll || (namesOnly && !inName)) continue;
			hits[nHits++] = i;
			const s = inName ? 1000 + inName - this.#nameLen[i] / 100 : inAll - this.#size[i] / 100;
			if (s <= 0 || (best.length === limit && s <= scores[limit - 1])) continue;
			// Equal scores keep list order: this file is later than every one already kept, so it goes after them.
			let lo = 0;
			let hi = best.length;
			while (lo < hi) {
				const mid = (lo + hi) >> 1;
				if (scores[mid] >= s) lo = mid + 1;
				else hi = mid;
			}
			best.splice(lo, 0, i);
			scores.splice(lo, 0, s);
			if (best.length > limit) (best.pop(), scores.pop());
		}
		return { best, scores, hits: hits.subarray(0, nHits) };
	}
}

let current: FileIndex | undefined;

/** The index of `files`: built once per list (the same array gives the same index), rebuilt when the list is a new one. */
export function indexFor(files: readonly string[]): FileIndex {
	if (current?.files !== files) current = new FileIndex(files);
	return current;
}
