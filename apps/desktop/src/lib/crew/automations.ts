/**
 * Pure utilities for crewmate automations (trigger descriptions, cron ↔ builder conversion).
 * No side-effects; safe to import in tests with --conditions browser.
 */
import type { Trigger } from "#lib/crew.ts";

export interface ScheduleSpec {
	/** Day-of-week indices: 0 = Sun … 6 = Sat (matches cron DOW). */
	days: number[];
	hour: number;
	minute: number;
}

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;

// ─── cron ↔ builder ──────────────────────────────────────────────────────────

/**
 * Parse a "m h * * dow" cron into a ScheduleSpec, or null for unsupported patterns
 * (non-standard fields, step expressions, multiple ranges, etc.).
 */
export function scheduleFromCron(cron: string): ScheduleSpec | null {
	const parts = cron.trim().split(/\s+/);
	if (parts.length !== 5) return null;
	const [min, hour, dom, month, dow] = parts;
	if (dom !== "*" || month !== "*") return null;

	// minute + hour must be simple integers
	if (!/^\d+$/.test(min) || !/^\d+$/.test(hour)) return null;
	const minuteN = parseInt(min, 10);
	const hourN = parseInt(hour, 10);
	if (hourN < 0 || hourN > 23 || minuteN < 0 || minuteN > 59) return null;

	let days: number[];
	if (dow === "*") {
		days = [0, 1, 2, 3, 4, 5, 6];
	} else if (/^\d+$/.test(dow)) {
		const d = parseInt(dow, 10);
		if (d < 0 || d > 6) return null;
		days = [d];
	} else if (/^\d+-\d+$/.test(dow)) {
		const [a, b] = dow.split("-").map(Number);
		if (a > b || a < 0 || b > 6) return null;
		days = Array.from({ length: b - a + 1 }, (_, i) => a + i);
	} else if (/^[\d,]+$/.test(dow)) {
		const ds = dow.split(",").map(Number);
		if (ds.some((d) => isNaN(d) || d < 0 || d > 6)) return null;
		days = ds;
	} else {
		return null;
	}

	return { days, hour: hourN, minute: minuteN };
}

/** Produce a canonical "m h * * dow" cron string from a ScheduleSpec. */
export function cronFromSchedule({ days, hour, minute }: ScheduleSpec): string {
	const sorted = [...days].sort((a, b) => a - b);
	let dow: string;
	if (sorted.length === 7) {
		dow = "*";
	} else if (sorted.length === 1) {
		dow = String(sorted[0]);
	} else {
		// Prefer compact range when contiguous
		const contiguous = sorted.every((d, i) => i === 0 || d === sorted[i - 1] + 1);
		dow = contiguous ? `${sorted[0]}-${sorted[sorted.length - 1]}` : sorted.join(",");
	}
	return `${minute} ${hour} * * ${dow}`;
}

// ─── interval ↔ builder ──────────────────────────────────────────────────────

export type IntervalUnit = "minutes" | "hours" | "days";

const UNIT_SECS: Record<IntervalUnit, number> = { minutes: 60, hours: 3600, days: 86400 };

/** The builder's amount + unit for an interval in seconds: the biggest unit that divides it evenly, else minutes. */
export function intervalFromSecs(secs: number): { value: number; unit: IntervalUnit } {
	if (secs % 86400 === 0) return { value: secs / 86400, unit: "days" };
	if (secs % 3600 === 0) return { value: secs / 3600, unit: "hours" };
	return { value: Math.round(secs / 60), unit: "minutes" };
}

export function intervalToSecs(value: number, unit: IntervalUnit): number {
	return value * UNIT_SECS[unit];
}

// ─── human-readable descriptions ─────────────────────────────────────────────

/** GitHub trigger events (hostd's whitelist) as the end of "When … in owner/repo". */
const GITHUB_EVENT_PHRASE: Record<string, string> = {
	pr_opened: "a PR is opened",
	pr_review_requested: "your review is requested",
	issue_opened: "an issue is opened",
	ci_failed: "CI fails",
	release: "a release is published",
};

/**
 * Produce a human-readable one-liner for a trigger kind.
 * Pass `tz` to append the timezone to schedule descriptions (used in the dialog's live sentence).
 */
export function describeTrigger(kind: Trigger["kind"], tz?: string): string {
	switch (kind.type) {
		case "schedule": {
			const spec = scheduleFromCron(kind.cron);
			if (spec) {
				const sorted = [...spec.days].sort((a, b) => a - b);
				const dayLabel =
					sorted.length === 7
						? "Every day"
						: sorted.length === 5 && sorted.join() === "1,2,3,4,5"
							? "Weekdays"
							: sorted.length === 2 && sorted.join() === "0,6"
								? "Weekends"
								: sorted.map((d) => DAY_NAMES[d]).join(", ");
				const timeLabel = `${String(spec.hour).padStart(2, "0")}:${String(spec.minute).padStart(2, "0")}`;
				const tzNote = tz ? ` (${tz})` : "";
				return `${dayLabel} at ${timeLabel}${tzNote}`;
			}
			return `Custom: ${kind.cron}`;
		}
		case "interval": {
			const s = kind.every_secs;
			if (s < 60) return `Every ${s}s`;
			const m = s / 60;
			if (s < 3600 && s % 60 === 0) return `Every ${m} minute${m !== 1 ? "s" : ""}`;
			const h = s / 3600;
			if (s % 3600 === 0) return `Every ${h} hour${h !== 1 ? "s" : ""}`;
			return `Every ${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
		}
		case "webhook":
			return "Webhook";
		case "github": {
			const phrases = kind.events.map((e) => GITHUB_EVENT_PHRASE[e] ?? e.replace(/_/g, " "));
			const list =
				phrases.length <= 1 ? (phrases[0] ?? "") : `${phrases.slice(0, -1).join(", ")} or ${phrases.at(-1)}`;
			return `When ${list} in ${kind.repo}`;
		}
		case "file":
			return `When ${kind.glob ?? kind.path} changes`;
		case "crewmate":
			return `When ${kind.from_crewmate} ${kind.on === "done" ? "finishes" : "fails"}`;
		case "service":
			return `When ${kind.service} ${kind.on === "failed" ? "fails" : "recovers"}`;
		default:
			return "";
	}
}

/**
 * Describe when the next fire will occur relative to `now` (ms).
 * `next_fire` is unix seconds.
 */
export function describeNext(next_fire: number, now: number): string {
	const fireMs = next_fire * 1000;
	const fire = new Date(fireMs);
	const nowDate = new Date(now);
	const tomorrow = new Date(now);
	tomorrow.setDate(tomorrow.getDate() + 1);

	const timeStr = fire.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

	if (
		fire.getFullYear() === nowDate.getFullYear() &&
		fire.getMonth() === nowDate.getMonth() &&
		fire.getDate() === nowDate.getDate()
	)
		return `today at ${timeStr}`;
	if (
		fire.getFullYear() === tomorrow.getFullYear() &&
		fire.getMonth() === tomorrow.getMonth() &&
		fire.getDate() === tomorrow.getDate()
	)
		return `tomorrow at ${timeStr}`;
	return `${fire.toLocaleDateString([], { weekday: "short" })} ${timeStr}`;
}
