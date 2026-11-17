/**
 * The pure side of the display language: what Rust's `locale_get` / `locale_set` reply and the `locale://changed`
 * event carry (`crates/atlas-desktop/src/cmds/locale.rs`, `LocaleState`), parsed defensively, and the names each
 * language is shown under. No Svelte, no Tauri: `locale.svelte.ts` holds the state.
 */
import { isLocale, type Locale } from "#lib/paraglide/runtime.js";

/** Sent to every window when the language changes. */
export const LOCALE_CHANGED = "locale://changed";

/** Which input decided the language (`atlas_core::lang::Source`). `env` is `ATLAS_LANG`, which beats the saved choice. */
export type LocaleSource = "env" | "file" | "posix" | "os" | "default";
const SOURCES: readonly string[] = ["env", "file", "posix", "os", "default"];

export interface LocaleState {
	/** The language the UI renders in. */
	locale: Locale;
	/** What the user saved in Settings; `null` is "Follow system". */
	preference: Locale | null;
	source: LocaleSource;
	/** The languages with a catalog. */
	supported: Locale[];
}

/**
 * Each language under its own name: endonyms, never translated (a person who cannot read the current language must be
 * able to find theirs). `Record<Locale, …>` makes a language added to the catalogs a type error until it is named.
 */
export const LOCALE_NAMES: Record<Locale, string> = { en: "English", "pt-BR": "Português (Brasil)" };

/** A reply from Rust (or anything else) as a `LocaleState`; `null` when it is not one. */
export function parseLocaleState(raw: unknown): LocaleState | null {
	if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
	const o = Object.fromEntries(Object.entries(raw));
	if (!isLocale(o.locale)) return null;
	const source = SOURCES.find((s) => s === o.source) as LocaleSource | undefined;
	return {
		locale: o.locale,
		preference: isLocale(o.preference) ? o.preference : null,
		source: source ?? "default",
		supported: Array.isArray(o.supported) ? o.supported.filter(isLocale) : [],
	};
}
