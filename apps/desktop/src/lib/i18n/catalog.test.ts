// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import * as compiled from "#lib/paraglide/messages.js";
import { locales } from "#lib/paraglide/runtime.js";
import { inLocale } from "../../../test/locale.ts";

const root = join(import.meta.dir, "../../..");
const json = (rel: string) => JSON.parse(readFileSync(join(root, rel), "utf8"));
// What Paraglide compiles is `.i18n/{locale}.json`, generated from the one source tree (`i18n/messages/<area>/…`) by
// `scripts/i18n/generate.ts`; descriptions live in each area's `meta.json`.
const catalog = (locale: string) => Object.keys(json(`.i18n/${locale}.json`)).filter((k) => k !== "$schema");
const SOURCE = join(root, "../../i18n/messages");
const descriptions = () =>
	Object.assign(
		{},
		...readdirSync(SOURCE).map((area) => JSON.parse(readFileSync(join(SOURCE, area, "meta.json"), "utf8"))),
	);

// A plugin path that does not resolve makes the Paraglide CLI log a warning and still print "Successfully compiled"
// with ZERO messages: every `m.*()` then fails at runtime, far from the cause. What is compiled is what is asserted.
test("the compiled messages are every key of the English catalog, no fewer", () => {
	const keys = catalog("en");
	expect(keys.length).toBeGreaterThan(0);
	const exposed = Object.keys(compiled).filter((k) => typeof compiled[k] === "function");
	expect(exposed.sort()).toEqual([...keys].sort());
});

test("the project loads its format plugin from a local, installed path (no CDN)", () => {
	const settings = json("project.inlang/settings.json");
	expect(settings.baseLocale).toBe("en");
	expect(settings.locales).toEqual([...locales]);
	for (const module of settings.modules) {
		expect(module).not.toMatch(/^https?:/);
		expect(existsSync(join(root, module)), `${module} is not installed`).toBe(true);
	}
});

test("every language has the same keys as English", () => {
	for (const l of locales) expect(catalog(l), l).toEqual(catalog("en"));
});

test("plural variants: zero is its own sentence in both languages, one and other follow the language's rules", async () => {
	const lines = (m) => [0, 1, 2, 21].map((count) => m.settings_host_sub_current({ count }));
	expect(await inLocale("en", () => lines(compiled))).toEqual([
		"Up to date · no running terminals",
		"Up to date · 1 running terminal",
		"Up to date · 2 running terminals",
		"Up to date · 21 running terminals",
	]);
	expect(await inLocale("pt-BR", () => lines(compiled))).toEqual([
		"Atualizado · sem terminais abertos",
		"Atualizado · 1 terminal aberto",
		"Atualizado · 2 terminais abertos",
		"Atualizado · 21 terminais abertos",
	]);
});

test("every key has an English description that is not the text itself", () => {
	const described = descriptions();
	const en = json(".i18n/en.json");
	for (const key of catalog("en")) {
		// A meta entry is the description, or `{ description, distinct }` when two near-identical texts need telling apart.
		const d = typeof described[key] === "string" ? described[key] : described[key]?.description;
		expect(d, `${key} has no description in i18n/messages/*/meta.json`).toBeString();
		expect(d, key).not.toBe(en[key]);
	}
});
