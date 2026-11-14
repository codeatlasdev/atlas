/**
 * crewPalette.ts — pure logic for the crew command palette.
 * No Svelte imports; safe for unit tests.
 */

export type CrewmateRow = {
	kind: "crewmate";
	id: string;
	hostKey: string;
	name: string;
	/** Truncated role subtitle. */
	subtitle: string;
};

export type CommandRow = {
	kind: "command";
	id: string;
	name: string;
	subtitle?: string;
};

export type PaletteRow = CrewmateRow | CommandRow;

// ─── normalise ────────────────────────────────────────────────────────────────

/** Extra transliterations for chars that don't decompose via NFD (ø, ð, etc.). */
const TRANSLIT: Record<string, string> = {
	ø: "o",
	Ø: "o",
	ð: "d",
	Ð: "d",
	þ: "th",
	Þ: "th",
	ß: "ss",
	æ: "ae",
	Æ: "ae",
	œ: "oe",
	Œ: "oe",
	ł: "l",
	Ł: "l",
};
const TRANSLIT_RE = new RegExp(Object.keys(TRANSLIT).join("|"), "g");

/** Strip accents and transliterate special chars, then lower-case. */
function norm(s: string): string {
	return s
		.replace(TRANSLIT_RE, (c) => TRANSLIT[c] ?? c)
		.normalize("NFD")
		.replace(/[\u0300-\u036f]/g, "")
		.toLowerCase();
}

// ─── filter ──────────────────────────────────────────────────────────────────

/**
 * Filter `rows` by `query` over title + subtitle.
 * Empty / whitespace-only query returns all rows unchanged.
 */
export function filterRows(rows: PaletteRow[], query: string): PaletteRow[] {
	const q = norm(query.trim());
	if (!q) return rows;
	return rows.filter((row) => {
		const title = row.kind === "crewmate" ? row.name : row.name;
		const sub = row.kind === "crewmate" ? row.subtitle : (row.subtitle ?? "");
		return norm(title).includes(q) || norm(sub).includes(q);
	});
}

// ─── ⌘1–⌘9 mapping ───────────────────────────────────────────────────────────

/**
 * Map a keyboard digit key ("1"–"9") to the zero-based index it should activate.
 * Returns -1 for unmapped keys.
 */
export function cmdKeyIndex(key: string): number {
	const n = parseInt(key, 10);
	if (Number.isNaN(n) || n < 1 || n > 9) return -1;
	return n - 1;
}

/**
 * The ⌘N label for a row at position `index` (0-based).
 * Returns "" for positions ≥ 9 (no keycap shown).
 */
export function keycapLabel(index: number): string {
	if (index < 0 || index > 8) return "";
	return String(index + 1);
}
