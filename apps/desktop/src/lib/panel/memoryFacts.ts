/**
 * What the runner says about a service's memory (#131): `memory_rss` (bytes of the process tree),
 * its `memory_trend` (`rising` | `flat` | `falling`) in `memory_trend_mb_min`, `budget_state`
 * (`ok` | `warn` | `over` against `memory_warn` / `max_memory`) and a suspected `memory_leak`.
 * The keys are the ones the Rust producer (`memory_facts_json`) serializes, pinned by
 * `__fixtures__/service-memory.json`.
 */
import { m } from "#lib/i18n/m.ts";

export interface MemoryRow {
	memory_rss?: number | null;
	memory_trend?: string | null;
	memory_trend_mb_min?: number | null;
	budget_state?: string | null;
	memory_leak?: string | null;
}

/** Value compare for the row diff: `state` events repeat on every health tick. */
export function sameMemory(a: MemoryRow, b: MemoryRow): boolean {
	return (
		a.memory_rss === b.memory_rss &&
		a.memory_trend === b.memory_trend &&
		a.memory_trend_mb_min === b.memory_trend_mb_min &&
		a.budget_state === b.budget_state &&
		a.memory_leak === b.memory_leak
	);
}

export type MemoryTone = "warn" | "error";

export interface MemoryNote {
	tone: MemoryTone;
	/** One short line for the card. */
	text: string;
	/** The full sentence, for the tooltip. */
	title: string;
}

const MB = 1 << 20;

/** `+42 MB/min` / `-8 MB/min`. */
const rate = (mb: number): string => `${mb > 0 ? "+" : ""}${Math.round(mb)} MB/min`;

/**
 * The line a card adds under its memory sparkline, or null when there is nothing to say: a leak
 * suspicion first (it is the actionable one), then a budget overrun, then a climbing trend. A flat
 * or falling service in budget stays quiet.
 */
export function memoryNote(s: MemoryRow): MemoryNote | null {
	if (s.memory_leak) return { tone: "warn", text: s.memory_leak, title: s.memory_leak };
	if (s.budget_state === "over")
		return {
			tone: "error",
			text: m.panel_memory_over_max(),
			title: m.panel_memory_over_max_title(),
		};
	if (s.budget_state === "warn")
		return {
			tone: "warn",
			text: m.panel_memory_over_warn(),
			title: m.panel_memory_over_warn_title(),
		};
	if (s.memory_trend === "rising" && s.memory_trend_mb_min != null) {
		const trend = rate(s.memory_trend_mb_min);
		return {
			tone: "warn",
			text: m.panel_memory_rising({ rate: trend }),
			title:
				s.memory_rss != null
					? m.panel_memory_rising_title_size({ rate: trend, size: (s.memory_rss / MB).toFixed(0) })
					: m.panel_memory_rising_title({ rate: trend }),
		};
	}
	return null;
}
