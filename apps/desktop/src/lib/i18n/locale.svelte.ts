/**
 * The display language of the UI: the ONE reactive value every `m.*()` call and every `fmt.ts` helper reads.
 *
 * It is a Svelte `$state` behind a Paraglide custom strategy (`custom-atlas`, see `scripts/paraglide.ts`), so a switch
 * re-renders what is on screen with no reload. Where the value comes from is decided once, in Rust
 * (`atlas_core::lang`: `ATLAS_LANG`, the saved choice, the environment, the OS, English): `bootLocale()` asks for it
 * before the first render, `chooseLocale()` saves a new choice, and the page never looks at `navigator.language`.
 *
 * Rules for code that uses it (`i18n:check` rule `frozen` enforces them): never call `m.*()` at module top level or keep its result in a plain variable (compute
 * it in a template, a `$derived` or a function, so it follows a switch).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	baseLocale,
	defineCustomClientStrategy,
	getTextDirection,
	type Locale,
} from "#lib/paraglide/runtime.js";
import { LOCALE_CHANGED, parseLocaleState, type LocaleSource, type LocaleState } from "./state.ts";

export { LOCALE_NAMES, localeName } from "./state.ts";

const store = $state<{ locale: Locale; preference: Locale | null; source: LocaleSource }>({
	locale: baseLocale,
	preference: null,
	source: "default",
});

defineCustomClientStrategy("custom-atlas", {
	getLocale: () => store.locale,
	// The guard is REQUIRED. Paraglide calls `setLocale(resolved)` the first time `getLocale()` runs, which is inside the
	// first template that renders a message, and writing a `$state` there throws `state_unsafe_mutation`.
	setLocale: (locale) => {
		if (store.locale !== locale) store.locale = locale as Locale;
	},
});

/** The language the UI renders in now. Reactive: read it inside a template, `$derived` or `$effect`. */
export const appLocale = (): Locale => store.locale;

/** What the user saved (`null` = Follow system). Reactive. */
export const localePreference = (): Locale | null => store.preference;

/** Which input decided the language; `env` means `ATLAS_LANG` overrides the saved choice. Reactive. */
export const localeSource = (): LocaleSource => store.source;

/** Render in `tag` right now, without saving it. Re-renders every message and formatted value on screen. */
export function setAppLocale(tag: Locale): void {
	if (store.locale !== tag) store.locale = tag;
}

function apply(s: LocaleState): void {
	setAppLocale(s.locale);
	store.preference = s.preference;
	store.source = s.source;
}

/**
 * Save the user's choice (`null` = Follow system) through Rust and render in the result. Rejects with Rust's reason
 * when the language has no catalog or the file cannot be written, leaving the UI as it was.
 */
export async function chooseLocale(preference: Locale | null): Promise<LocaleState> {
	const s = parseLocaleState(await invoke("locale_set", { tag: preference }));
	if (!s) throw new Error("locale_set: unexpected reply");
	apply(s);
	return s;
}

let stopSync: (() => void) | undefined;

/** Keep `<html lang dir>` equal to the language. Idempotent; returns the stop function. */
export function syncDocumentLocale(): () => void {
	stopSync ??= $effect.root(() => {
		$effect(() => {
			const root = document.documentElement;
			root.lang = store.locale;
			root.dir = getTextDirection(store.locale);
		});
	});
	return stopSync;
}

/**
 * Before the first render: ask Rust which language to use, follow later changes (a switch in another window), and keep
 * `<html lang>` in step. Without the command (a plain browser tab, no Tauri) the base language stays. With Tauri
 * present, a failure is logged: the app still opens, in English.
 */
export async function bootLocale(): Promise<void> {
	syncDocumentLocale();
	if (!("__TAURI_INTERNALS__" in window)) return;
	try {
		const s = parseLocaleState(await invoke("locale_get"));
		if (!s) throw new Error("locale_get: unexpected reply");
		apply(s);
	} catch (e) {
		console.error("[atlas] display language unavailable, using English:", e);
		return;
	}
	listen(LOCALE_CHANGED, (e) => {
		const next = parseLocaleState(e.payload);
		if (next) apply(next);
	}).catch((e) => console.error("[atlas] language changes in other windows are not followed:", e));
}
