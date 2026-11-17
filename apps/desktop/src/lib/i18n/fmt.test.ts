// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { dateTime, formatNumber, list, relative, relativeTime, upperFirst } from "./fmt.ts";
import { inLocale } from "../../../test/locale.ts";

const NOW = Date.UTC(2026, 0, 10, 12, 0, 0);
const ago = (seconds: number, locale?: string) => relativeTime(NOW - seconds * 1000, NOW, locale);

test("relative time picks the largest whole unit, in one style, in English", () => {
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
		"1 min. ago",
		"1 min. ago",
		"3 hr. ago",
		"yesterday",
		"3 days ago",
		"last wk.",
		"2 wk. ago",
		"last mo.",
		"last yr.",
		"3 yr. ago",
	]);
});

test("a time in the future reads 'now', never 'in 3 sec.'", () => {
	expect(relativeTime(NOW + 3_000, NOW)).toBe("now");
});

test("the same call reads right in Brazilian Portuguese, from the language alone", async () => {
	await inLocale("pt-BR", () => {
		expect([5, 90, 3 * 3600, 86_400, 3 * 86_400, 40 * 86_400].map((s) => ago(s))).toEqual([
			"agora",
			"há 1 min.",
			"há 3 h",
			"ontem",
			"há 3 dias",
			"mês passado",
		]);
	});
});

test("an explicit locale wins over the UI language (text that must stay English for a maintainer)", async () => {
	await inLocale("pt-BR", () => expect(ago(3 * 3600, "en")).toBe("3 hr. ago"));
});

test("relative() is the raw formatter for labels that are not 'ago' (axis ends, group headers)", () => {
	expect(relative(-30, "minute")).toBe("30 min. ago");
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
