/** ⌃Tab switcher: a frozen MRU snapshot walked while ⌃ is held, committed on release. */

export interface Switcher {
	ids: string[];
	index: number;
}

/** Start at the entry after the current tab (`step` 1) or at the oldest (`step` -1). */
export function openSwitcher(mru: string[], step: 1 | -1): Switcher | null {
	if (mru.length < 2) return null;
	return { ids: mru, index: step === 1 ? 1 : mru.length - 1 };
}

/** Move the highlight, wrapping. */
export function stepSwitcher(s: Switcher, step: 1 | -1): Switcher {
	return { ids: s.ids, index: (s.index + step + s.ids.length) % s.ids.length };
}

/** The tab to activate when ⌃ is released. */
export const pickSwitcher = (s: Switcher): string | undefined => s.ids[s.index];
