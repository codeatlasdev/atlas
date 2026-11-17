// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { dateTime, formatNumber, languageName, list, relative, relativeTime, upperFirst } from "./fmt.ts";
import { inLocale } from "../../../test/locale.ts";

const NOW = Date.UTC(2026, 0, 10, 12, 0, 0);
const ago = (seconds: number, locale?: string) => relativeTime(NOW - seconds * 1000, NOW, locale);

test("relative time picks the largest whole unit, in the style guide's form, in English", () => {
	expect(
		[
			5,
			59,
			60,
			90,
			3 * 3600,
			86_400,
			3 * 86_400,
			7 * 86_400,
			20 * 86_400,
			40 * 86_400,
			400 * 86_400,
			3 * 365 * 86_400,
		].map((s) => ago(s)),
	).toEqual([
		"now",
		"now",
		"1 min ago",
		"1 min ago",
		"3 hr ago",
		"yesterday",
		"3 days ago",
		"last week",
		"2 wk ago",
		"last month",
		"last year",
		"3 yr ago",
	]);
});

test("a time in the future reads 'now', never 'in 3 sec'", () => {
	expect(relativeTime(NOW + 3_000, NOW)).toBe("now");
});

test("the same call reads right in Brazilian Portuguese, from the language alone", async () => {
	await inLocale("pt-BR", () => {
		expect([5, 90, 3 * 3600, 86_400, 3 * 86_400, 40 * 86_400].map((s) => ago(s))).toEqual([
			"agora",
			"há 1 min",
			"há 3 h",
			"ontem",
			"há 3 dias",
			"mês passado",
		]);
	});
});

// i18n/style/en.md says `5 min ago`, pt-BR.md says `há 5 minutos` in prose and the UI form `há 5 min`: Intl's `short`
// abbreviations without the trailing period, and written out where there is no number.
const UNITS = ["second", "minute", "hour", "day", "week", "month", "year"] as const;
const TABLE = {
	en: {
		second: ["5 sec ago", "1 sec ago", "now", "in 1 sec", "in 5 sec"],
		minute: ["5 min ago", "1 min ago", "this minute", "in 1 min", "in 5 min"],
		hour: ["5 hr ago", "1 hr ago", "this hour", "in 1 hr", "in 5 hr"],
		day: ["5 days ago", "yesterday", "today", "tomorrow", "in 5 days"],
		week: ["5 wk ago", "last week", "this week", "next week", "in 5 wk"],
		month: ["5 mo ago", "last month", "this month", "next month", "in 5 mo"],
		year: ["5 yr ago", "last year", "this year", "next year", "in 5 yr"],
	},
	"pt-BR": {
		second: ["há 5 seg", "há 1 seg", "agora", "em 1 seg", "em 5 seg"],
		minute: ["há 5 min", "há 1 min", "este minuto", "em 1 min", "em 5 min"],
		hour: ["há 5 h", "há 1 h", "esta hora", "em 1 h", "em 5 h"],
		day: ["há 5 dias", "ontem", "hoje", "amanhã", "em 5 dias"],
		week: ["há 5 sem", "semana passada", "esta semana", "próxima semana", "em 5 sem"],
		month: ["há 5 meses", "mês passado", "este mês", "próximo mês", "em 5 meses"],
		year: ["há 5 anos", "ano passado", "este ano", "próximo ano", "em 5 anos"],
	},
} as const;

test("every unit, past and future, zero and one, in both languages, never with a period", () => {
	for (const [locale, rows] of Object.entries(TABLE))
		for (const unit of UNITS) {
			const got = [-5, -1, 0, 1, 5].map((v) => relative(v, unit, locale));
			expect(got, `${locale} ${unit}`).toEqual(rows[unit]);
			for (const text of got) expect(text, `${locale} ${unit}`).not.toContain(".");
		}
});

test("an explicit locale wins over the UI language (text that must stay English for a maintainer)", async () => {
	await inLocale("pt-BR", () => expect(ago(3 * 3600, "en")).toBe("3 hr ago"));
});

test("relative() is the raw formatter for labels that are not 'ago' (axis ends, group headers)", () => {
	expect(relative(-30, "minute")).toBe("30 min ago");
	expect(relative(0, "second")).toBe("now");
	expect(upperFirst(relative(0, "day"))).toBe("Today");
});

test("absolute date and time follow the language; the default is a medium date with a short time", async () => {
	const t = Date.UTC(2026, 0, 10, 19, 5);
	// The joiner between date and time ("," or " at ") changed between CLDR versions: pin the parts, not the glue.
	expect(dateTime(t)).toMatch(/^Jan 10, 2026\D+7:05 PM$/);
	expect(dateTime(t, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" })).toBe("19:05");
	await inLocale("pt-BR", () => {
		expect(dateTime(t)).toMatch(/^10 de jan\. de 2026\D+19:05$/);
	});
});

test("numbers group and mark decimals by language", async () => {
	expect(formatNumber(1234567.5)).toBe("1,234,567.5");
	await inLocale("pt-BR", () => expect(formatNumber(1234567.5)).toBe("1.234.567,5"));
});

test("lists use the language's conjunction", async () => {
	expect(list(["a", "b", "c"])).toBe("a, b, and c");
	expect(list(["a", "b"], "disjunction")).toBe("a or b");
	await inLocale("pt-BR", () => expect(list(["a", "b", "c"])).toBe("a, b e c"));
});

// The Match system option names the system's language inside a parenthesis, so it must be flat: the base language in
// the UI language ("português", never "Português (Brasil)").
test("a language name is the base language in the given locale, with no region and no nested parentheses", () => {
	expect(languageName("en", "en")).toBe("English");
	expect(languageName("pt-BR", "en")).toBe("Portuguese");
	expect(languageName("pt-BR", "pt-BR")).toBe("português");
	expect(languageName("en", "pt-BR")).toBe("inglês");
	for (const l of ["en", "pt-BR"])
		for (const t of ["en", "pt-BR"]) expect(languageName(t, l)).not.toContain("(");
});
