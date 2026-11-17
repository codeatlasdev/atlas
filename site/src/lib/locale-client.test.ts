// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { afterAll, beforeAll, describe, expect, test } from "bun:test";

// The browser reads the language of a script-rendered string (burger label) from <html lang>. This runs the REAL compiled Paraglide
// runtime and the real strategy against a fake document, in the order the strategy list runs them (globalVariable, custom-htmlLang).
// It needs `bun run i18n:compile` first (`bun run check` does it).
const html = { lang: "pt-BR" };
beforeAll(() => {
	globalThis.window = { location: { href: "https://atlas.codeatlas.com.br/pt-br/docs/x/" } };
	globalThis.document = { documentElement: html };
});
afterAll(() => {
	delete globalThis.window;
	delete globalThis.document;
});

describe("client strategy: the page's <html lang> is the locale", () => {
	test("a pt-BR page renders pt-BR strings, from the same catalog the server rendered", async () => {
		await import("./locale-client");
		const { m } = await import("./paraglide/messages.js");
		const { getLocale } = await import("./paraglide/runtime.js");
		expect(getLocale()).toBe("pt-BR");
		expect(m.nav_menu_open()).toBe("Abrir menu");
		expect(m.nav_menu_close()).toBe("Fechar menu");
	});
	test("an explicit locale option still wins (the 404 page renders both languages that way)", async () => {
		const { m } = await import("./paraglide/messages.js");
		expect(m.nav_menu_open({}, { locale: "en" })).toBe("Open menu");
	});
});
