// The torn-frame oracle of `scripts/term-flood-bench.ts`, pure so it is tested (`term-flood-judge.test.ts`).
// `scripts/term-flood.ts` paints one block of boxes whose every row carries the same frame number.

export type Sample = "whole" | "torn" | "erased" | "before-first-frame";

/** What the page records per read of the screen: the frame numbers it shows and on how many rows (see `SIGNATURE_JS`). */
export interface Signature {
	/** Distinct frame numbers on screen. */
	ids: string[];
	/** Rows that carry one. */
	rows: number;
}

/**
 * Page-side snippet that reads the VT screen in a loop and stores one `Signature` per read in `window.__sigs`.
 * Plain JS with no backslash escapes, so it survives being a template string.
 */
export const sigLoop = (tab: string, everyMs = 2) => `
	const S = (window.__sigs = { list: [], stop: false }), T = window.__TAURI_INTERNALS__;
	(async () => { while (!S.stop) {
		const text = await T.invoke("term_text", { tab: ${JSON.stringify(tab)} });
		const ids = new Set(); let rows = 0;
		for (const l of text.split(String.fromCharCode(10))) { const i = l.indexOf("frame "); if (i >= 0) { ids.add(l.slice(i + 6, i + 12)); rows++; } }
		S.list.push({ ids: [...ids], rows });
		await new Promise((r) => setTimeout(r, ${everyMs})); } })(); return 1`;

/**
 * Classify one read of the screen. `whole`: every block row of one frame. `torn`: rows of two frames (a draw landed
 * between two writes of a repaint). `erased`: some rows are gone (the erase half of erase-then-draw arrived and the
 * draw did not), including an empty screen once a frame was seen. `inner` is the number of rows that carry the frame
 * number (the block minus its two border rows). `seenFrame`: a whole frame was already observed in this run (before
 * it, an empty screen is just startup).
 */
export function classify(
	sig: Signature,
	inner: number,
	seenFrame: boolean,
): Sample {
	if (sig.ids.length > 1) return "torn";
	if (sig.ids.length === 1 && sig.rows < inner) return "erased";
	if (sig.ids.length === 0) return seenFrame ? "erased" : "before-first-frame";
	return "whole";
}

/** The rows a whole frame carries: the most any read showed (the block's size depends on the terminal's). */
export const innerRows = (sigs: Signature[]) =>
	sigs.reduce((m, s) => (s.ids.length === 1 ? Math.max(m, s.rows) : m), 0);

/** Count a run of signatures. */
export function tally(
	sigs: Signature[],
	inner = innerRows(sigs),
): Record<Sample, number> {
	const out: Record<Sample, number> = {
		whole: 0,
		torn: 0,
		erased: 0,
		"before-first-frame": 0,
	};
	let seen = false;
	for (const s of sigs) {
		const k = classify(s, inner, seen);
		if (k === "whole") seen = true;
		out[k]++;
	}
	return out;
}
