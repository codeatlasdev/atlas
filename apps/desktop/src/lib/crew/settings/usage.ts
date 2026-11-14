/**
 * Pure token-usage maths for the crewmate settings dialog: 30 daily buckets for the sparkline, today / 7d / 30d
 * totals for the stat line, and the compact token formatter.
 */
import type { CrewmateUsageRecord } from "#lib/crew.ts";

const DAY_MS = 24 * 60 * 60 * 1000;
export const USAGE_DAYS = 30;

export interface UsageTotals {
	today: number;
	week: number;
	month: number;
}

export const EMPTY_USAGE_TOTALS: UsageTotals = { today: 0, week: 0, month: 0 };

const tokensOf = (r: CrewmateUsageRecord) => (r.input_tokens ?? 0) + (r.output_tokens ?? 0);

/** Tokens per day for the last 30 days, oldest first (the last bucket is "now" back to 24 h ago). */
export function usageBuckets(records: CrewmateUsageRecord[], now: number): number[] {
	const buckets = new Array<number>(USAGE_DAYS).fill(0);
	for (const rec of records) {
		const dayIdx = Math.floor((now - rec.ts * 1000) / DAY_MS);
		if (dayIdx >= 0 && dayIdx < USAGE_DAYS) buckets[USAGE_DAYS - 1 - dayIdx] += tokensOf(rec);
	}
	return buckets;
}

/** Tokens since local midnight, since 7 days ago and since 30 days ago. */
export function usageTotals(records: CrewmateUsageRecord[], now: number): UsageTotals {
	const todayStart = new Date(now);
	todayStart.setHours(0, 0, 0, 0);
	const since = (from: number) =>
		records.filter((r) => r.ts * 1000 >= from).reduce((sum, r) => sum + tokensOf(r), 0);
	return {
		today: since(todayStart.getTime()),
		week: since(now - 7 * DAY_MS),
		month: since(now - USAGE_DAYS * DAY_MS),
	};
}

/** 1234 → "1.2k"; below 1000 the plain number. */
export function fmtTokens(n: number): string {
	return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}
