// The ONE Paraglide configuration of the site: astro.config.mjs (Vite plugin) and scripts/i18n-compile.mjs (`bun run i18n:compile`,
// which `check` runs so svelte-check and astro check see the typed `m.*` without a build) both read it.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

/** Write `.i18n/{locale}.json` from i18n/messages, the one source tree. Run before anything compiles. */
export function generateMessages() {
	const repo = fileURLToPath(new URL("../", import.meta.url));
	const r = spawnSync("bun", ["scripts/i18n/generate.ts", "--surface", "site"], { cwd: repo, encoding: "utf8" });
	if (r.status !== 0) throw new Error(`i18n:generate failed: ${r.stderr || r.stdout}`);
}

export const paraglideOptions = {
	project: "./project.inlang",
	outdir: "./src/lib/paraglide",
	emitTsDeclarations: true,
	// One structure for the Vite plugin, `i18n:compile` and bun test: the plugin's default (virtual modules) cannot be imported outside Vite,
	// and the two would overwrite each other's files in outdir. A few hundred messages compile in well under a second either way.
	outputStructure: "message-modules",
	// SSG: the locale of a page is set by src/middleware.ts into the global variable while Astro prerenders. In the browser the page's
	// own <html lang> is the source (src/lib/locale-client.ts), so no URL pattern of Paraglide is involved: Astro owns routing.
	strategy: ["globalVariable", "custom-htmlLang", "baseLocale"],
};
