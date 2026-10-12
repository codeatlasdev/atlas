/** Spaces: one per repo. Deterministic accent per id, ordering and switching, pure for tests. */

/** FNV-1a, 32 bit: stable across sessions and platforms. */
export function hash(s: string): number {
	let h = 0x811c9dc5;
	for (let i = 0; i < s.length; i++) {
		h ^= s.charCodeAt(i);
		h = Math.imul(h, 0x01000193);
	}
	return h >>> 0;
}

/** Hue (0–359) of a space, derived from its id. */
export const hueOf = (id: string): number => hash(id) % 360;

/** Lightness/chroma tuned so the accent reads on both the light and the dark surface. */
const TONE = { light: { l: 0.62, c: 0.17 }, dark: { l: 0.74, c: 0.14 } } as const;

/** OKLCH accent for a space id. `hue` overrides the derived one (user choice later). */
export function accentOf(id: string, dark: boolean, hue?: number): string {
	const { l, c } = dark ? TONE.dark : TONE.light;
	return `oklch(${l} ${c} ${hue ?? hueOf(id)})`;
}

/** Soft background tint of the accent, for large surfaces. */
export function tintOf(id: string, dark: boolean, hue?: number): string {
	return `oklch(${dark ? 0.74 : 0.62} ${dark ? 0.14 : 0.17} ${hue ?? hueOf(id)} / ${dark ? 0.1 : 0.07})`;
}

/** Smallest angular distance between two hues. */
const arc = (a: number, b: number) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));

/**
 * Spread hues so two spaces never land on near-identical colors: starting from the
 * derived hue, step forward until it is at least `gap` degrees from every earlier space.
 * With more spaces than the circle fits, the hue farthest from the others wins.
 * Deterministic for a given ordered id list.
 */
export function spreadHues(ids: string[], gap = 24): Record<string, number> {
	const out: Record<string, number> = {};
	const used: number[] = [];
	for (const id of ids) {
		const start = hueOf(id);
		let best = start;
		let bestDist = -1;
		for (let step = 0; step < 360; step++) {
			const h = (start + step) % 360;
			const d = used.length ? Math.min(...used.map((u) => arc(u, h))) : 360;
			if (d >= gap) {
				best = h;
				break;
			}
			if (d > bestDist) {
				best = h;
				bestDist = d;
			}
		}
		used.push(best);
		out[id] = best;
	}
	return out;
}

/** The neighbouring space id in `delta` direction, wrapping. `null` when there is nowhere to go. */
export function neighbour(ids: string[], current: string | null, delta: 1 | -1): string | null {
	if (ids.length < 2) return null;
	const i = current ? ids.indexOf(current) : -1;
	if (i < 0) return ids[delta === 1 ? 0 : ids.length - 1];
	return ids[(i + delta + ids.length) % ids.length];
}

/** Direction of a switch from `from` to `to` (1 = content enters from the right). */
export function directionOf(ids: string[], from: string | null, to: string | null): 1 | -1 {
	const a = from ? ids.indexOf(from) : -1;
	const b = to ? ids.indexOf(to) : -1;
	return b >= a ? 1 : -1;
}
