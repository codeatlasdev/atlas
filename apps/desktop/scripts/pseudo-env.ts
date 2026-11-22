// Whether a build of the desktop frontend is a development build, the only kind that offers the pseudo-locale en-XA.
// No imports on purpose: `vite.config.ts`, `paraglide.ts` and `scripts/bundle-budget.ts` (which runs from the repo root and
// cannot resolve the app's dependencies) all read it.

/** The pseudo-locale (`scripts/i18n/pseudo.ts`): a language of DEVELOPMENT builds only, never of a shipped app. */
export const PSEUDO_LOCALE = "en-XA";

/**
 * Whether this build offers the pseudo-locale: the Vite dev server (`command === "serve"`: `bun run dev`, Atlas Lab,
 * `desktop:dev`), a Tauri debug build (the Tauri CLI sets `TAURI_ENV_DEBUG=true` for its `beforeBuildCommand`: the debug
 * app of `desktop:build:app`, agent instances), or an explicit `ATLAS_PSEUDO_LOCALE=1` (`desktop:build:debug` sets it).
 * Everything else, the release build above all, compiles exactly the languages of `project.inlang/settings.json`.
 */
export function pseudoEnabled(command?: string, env: Record<string, string | undefined> = process.env): boolean {
	return command === "serve" || env.TAURI_ENV_DEBUG === "true" || env.ATLAS_PSEUDO_LOCALE === "1";
}
