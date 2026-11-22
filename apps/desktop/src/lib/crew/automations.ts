/**
 * Pure utilities for crewmate automations (trigger descriptions, cron ↔ builder conversion).
 * No side-effects; safe to import in tests with --conditions browser.
 */
import type { Trigger } from "#lib/crew.ts";
import { dateTime, list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";

export interface ScheduleSpec {
	/** Day-of-week indices: 0 = Sun … 6 = Sat (matches cron DOW). */
	days: number[];
	hour: number;
	minute: number;
}

/** A short weekday name in the UI language ("Mon", "seg"); 2024-01-07 is a Sunday, and `d` counts from it. */
function dayName(d: number): string {
	return dateTime(new Date(2024, 0, 7 + d), { weekday: "short" }).replaceAll(".", "");
}

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
function githubEventPhrase(event: string): string {
	switch (event) {
		case "pr_opened":
			return m.crew_automation_describe_github_pr_opened();
		case "pr_review_requested":
			return m.crew_automation_describe_github_review_requested();
		case "issue_opened":
			return m.crew_automation_describe_github_issue_opened();
		case "ci_failed":
			return m.crew_automation_describe_github_ci_failed();
		case "release":
			return m.crew_automation_describe_github_release();
		case "mention":
			return m.crew_automation_describe_github_mention();
		case "issue_assigned":
			return m.crew_automation_describe_github_issue_assigned();
		case "pr_review_comment":
			return m.crew_automation_describe_github_review_comment();
		default:
			return event.replace(/_/g, " ");
	}
}

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
				const days =
					sorted.length === 7
						? m.crew_automation_fields_every_day()
						: sorted.length === 5 && sorted.join() === "1,2,3,4,5"
							? m.crew_automation_fields_weekdays()
							: sorted.length === 2 && sorted.join() === "0,6"
								? m.crew_automation_describe_weekends()
								: list(sorted.map(dayName), "unit");
				const time = `${String(spec.hour).padStart(2, "0")}:${String(spec.minute).padStart(2, "0")}`;
				return tz
					? m.crew_automation_describe_schedule_tz({ days, time, tz })
					: m.crew_automation_describe_schedule({ days, time });
			}
			return m.crew_automation_describe_custom_cron({ cron: kind.cron });
		}
		case "interval": {
			const s = kind.every_secs;
			if (s < 60) return m.crew_automation_describe_every_seconds({ seconds: s });
			if (s < 3600 && s % 60 === 0) return m.crew_automation_describe_every_minutes({ count: s / 60 });
			if (s % 3600 === 0) return m.crew_automation_describe_every_hours({ count: s / 3600 });
			return m.crew_automation_describe_every_hours_minutes({
				hours: Math.floor(s / 3600),
				minutes: Math.floor((s % 3600) / 60),
			});
		}
		case "webhook":
			return m.crew_automation_describe_webhook();
		case "github":
			return m.crew_automation_describe_github({
				events: list(kind.events.map(githubEventPhrase), "disjunction"),
				repo: kind.repo,
			});
		case "file":
			return m.crew_automation_describe_file({ target: kind.glob ?? kind.path });
		case "crewmate":
			return kind.on === "done"
				? m.crew_automation_describe_crewmate_done({ name: kind.from_crewmate })
				: m.crew_automation_describe_crewmate_failed({ name: kind.from_crewmate });
		case "service":
			return kind.on === "failed"
				? m.crew_automation_describe_service_failed({ name: kind.service })
				: m.crew_automation_describe_service_recovered({ name: kind.service });
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

	const time = dateTime(fire, { hour: "2-digit", minute: "2-digit" });

	if (
		fire.getFullYear() === nowDate.getFullYear() &&
		fire.getMonth() === nowDate.getMonth() &&
		fire.getDate() === nowDate.getDate()
	)
		return m.crew_automation_describe_next_today({ time });
	if (
		fire.getFullYear() === tomorrow.getFullYear() &&
		fire.getMonth() === tomorrow.getMonth() &&
		fire.getDate() === tomorrow.getDate()
	)
		return m.crew_automation_describe_next_tomorrow({ time });
	return m.crew_automation_describe_next_day({ day: dateTime(fire, { weekday: "short" }), time });
}
