/**
 * Locale-aware formatting for UI text: relative time, absolute date and time, numbers, lists. The ONE place the app
 * builds `Intl` formatters, so every screen shows "5 min ago" (or "há 5 min") the same way.
 *
 * Each function reads `appLocale()` (a `$state`), so a call inside a template or `$derived` re-runs on a language
 * switch with no extra wiring. Formatters are memoized per locale and options: constructing an `Intl` object costs far
 * more than formatting with it. Pass `locale` only for text that must not follow the UI language (a diagnostics report
 * for a maintainer is always English).
 *
 * Plural-safe counts are not here: a count that picks a word ("1 terminal", "2 terminais") is a message with plural
 * variants (`messages/*.json`), and `{count: number}` inside a message formats the digits for the locale.
 */
import { appLocale } from "./locale.svelte.ts";

const cache = new Map<string, unknown>();

function memo<T>(kind: string, locale: string, opts: object | undefined, make: () => T): T {
	const key = `${kind}|${locale}|${opts ? JSON.stringify(opts) : ""}`;
	let f = cache.get(key) as T | undefined;
	if (f === undefined) cache.set(key, (f = make()));
	return f;
}

/**
 * `value` units from now in words ("yesterday", "5 min ago", "in 2 hr"; "há 5 min", "ontem"). The style guides
 * (`i18n/style/en.md`, `pt-BR.md`) want `5 min ago` and `há 5 min`: the abbreviations of Intl's `short` style, whose
 * only blemish is a trailing period ("5 min.") that is removed from the text around the number. A phrase with no
 * number ("yesterday", "last week", "now") reads best written out, so it takes the `long` style ("last wk." is not a
 * phrase). All words, plurals and word order come from CLDR; nothing here knows a language.
 */
export function relative(value: number, unit: Intl.RelativeTimeFormatUnit, locale = appLocale()): string {
	const rtf = (style: Intl.RelativeTimeFormatStyle) =>
		memo("rtf", locale, { style }, () => new Intl.RelativeTimeFormat(locale, { numeric: "auto", style }));
	const parts = rtf("short").formatToParts(value, unit);
	if (!parts.some((p) => p.type === "integer")) return rtf("long").format(value, unit);
	return parts.map((p) => (p.type === "literal" ? p.value.replaceAll(".", "") : p.value)).join("");
}

const MINUTE = 60;
const HOUR = 3_600;
const DAY = 86_400;
const WEEK = 7 * DAY;
const MONTH = 30 * DAY;
const YEAR = 365 * DAY;

/**
 * How long before `now` the instant `ts` was (both epoch milliseconds), in the largest whole unit: "now" under a
 * minute, then minutes, hours, days, weeks, months, years, rounded down. A `ts` in the future reads "now": clocks
 * differ by a moment and "in 3 sec." would only look broken.
 */
export function relativeTime(ts: number, now = Date.now(), locale = appLocale()): string {
	const s = Math.max(0, Math.floor((now - ts) / 1000));
	if (s < MINUTE) return relative(0, "second", locale);
	const [size, unit] = (
		[
			[YEAR, "year"],
			[MONTH, "month"],
			[WEEK, "week"],
			[DAY, "day"],
			[HOUR, "hour"],
			[MINUTE, "minute"],
		] as const
	).find(([size]) => s >= size)!;
	return relative(-Math.floor(s / size), unit, locale);
}

/** An absolute date and time (`ts`: epoch milliseconds or a `Date`); `opts` picks the parts, medium date and short time by default. */
export function dateTime(
	ts: number | Date,
	opts: Intl.DateTimeFormatOptions = { dateStyle: "medium", timeStyle: "short" },
	locale = appLocale(),
): string {
	return memo("dtf", locale, opts, () => new Intl.DateTimeFormat(locale, opts)).format(ts);
}

/** A number for display: digit grouping and decimal mark of the locale. */
export function formatNumber(n: number, opts?: Intl.NumberFormatOptions, locale = appLocale()): string {
	return memo("nf", locale, opts, () => new Intl.NumberFormat(locale, opts)).format(n);
}

/** "a, b and c" / "a, b e c": the conjunction (or `disjunction`, "or") and the commas of the locale. */
export function list(
	items: Iterable<string>,
	type: Intl.ListFormatType = "conjunction",
	locale = appLocale(),
): string {
	return memo("lf", locale, { type }, () => new Intl.ListFormat(locale, { type })).format(items);
}

/** `s` with its first letter in capitals, by the locale's rules: a group header made of a relative word ("today"). */
export function upperFirst(s: string, locale = appLocale()): string {
	return s.charAt(0).toLocaleUpperCase(locale) + s.slice(1);
}

/**
 * The name of `tag`'s base language, written in `locale` (`en` in pt-BR is "inglês"; `pt-BR` in pt-BR is "português",
 * with no region): a short name that fits inside a parenthesis. The language lists show the full endonym instead.
 */
export function languageName(tag: string, locale = appLocale()): string {
	const base = tag.split("-")[0];
	const name =
		memo("dn", locale, undefined, () => new Intl.DisplayNames(locale, { type: "language" })).of(base) ?? base;
	// Portuguese writes language names in lower case inside a sentence ("igual ao sistema (português)"); engines differ on
	// whether they hand the name back capitalized (JavaScriptCore does, V8 does not), so the rule is applied here.
	return locale.split("-")[0] === "pt" ? name.toLocaleLowerCase(locale) : name;
}
