/**
 * The display-language compiler: `messages/{locale}.json` (inlang message format) in, typed `m.*()` functions out at
 * `src/lib/paraglide/` (generated, gitignored). ONE options object for the two callers that must agree:
 * `vite.config.ts` (dev and build, recompiles on every catalog edit) and `compileMessages()` (`bun run i18n:compile`,
 * and the `bun test` preload, which has no Vite).
 *
 * Output is physical (`message-modules`), not Vite's default virtual modules: Bun's test loader and `svelte-check`
 * cannot resolve `virtual:` imports, and the test that pins "the compiled `m` has every key of en.json" must run
 * against exactly what the app ships.
 */
import { compile } from "@inlang/paraglide-js";
import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

export const PARAGLIDE: Parameters<typeof compile>[0] = {
	project: "./project.inlang",
	outdir: "./src/lib/paraglide",
	outputStructure: "message-modules",
	// The locale is a Svelte `$state` (`lib/i18n/locale.svelte.ts`) so a switch re-renders without a reload; never the
	// navigator's language (the Rust side resolves it once: `atlas_core::lang`).
	strategy: ["custom-atlas", "baseLocale"],
	emitTsDeclarations: true,
};

const newest = (dir: string) =>
	Math.max(0, ...readdirSync(dir).map((f) => statSync(join(dir, f)).mtimeMs));

/** Whether `src/lib/paraglide` is missing or older than the catalogs and the project settings. */
export function isStale(): boolean {
	let built: number;
	try {
		built = statSync(join(root, PARAGLIDE.outdir, "messages.js")).mtimeMs;
	} catch {
		return true;
	}
	return Math.max(newest(join(root, "messages")), newest(join(root, "project.inlang"))) > built;
}

/** Compile the catalogs to `src/lib/paraglide`; with `onlyIfStale`, only when they changed since. */
export async function compileMessages(onlyIfStale = false): Promise<void> {
	if (onlyIfStale && !isStale()) return;
	const cwd = process.cwd();
	process.chdir(root);
	try {
		await compile({ ...PARAGLIDE });
	} finally {
		process.chdir(cwd);
	}
}

if (import.meta.main) await compileMessages();
