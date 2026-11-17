// @ts-nocheck bun test runtime types are not part of the app tsconfig
/** Tests run in the base language; one that needs another renders inside `inLocale` and gets the previous one back. */
import { appLocale, setAppLocale } from "#lib/i18n/locale.svelte.ts";

export async function inLocale<T>(tag: "en" | "pt-BR", fn: () => T | Promise<T>): Promise<T> {
	const before = appLocale();
	setAppLocale(tag);
	try {
		return await fn();
	} finally {
		setAppLocale(before);
	}
}
