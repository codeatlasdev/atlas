/**
 * The display-language compiler: `.i18n/{locale}.json` (inlang message format, generated from the one source tree
 * `i18n/messages/**` by `scripts/i18n/generate.ts`, gitignored) in, typed `m.*()` functions out at
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
import { spawnSync } from "node:child_process";
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

const repo = fileURLToPath(new URL("../../..", import.meta.url));

/**
 * Write `.i18n/{locale}.json` from i18n/messages (only the files that changed, so mtimes stay put when nothing did).
 * Always `bun`, never `process.execPath`: `svelte-kit sync` and `vite` load vite.config.ts (and so this file) in Node,
 * which cannot run the generator's TypeScript.
 */
export function generateMessages(): void {
	const r = spawnSync("bun", [join(repo, "scripts/i18n/generate.ts"), "--surface", "desktop"], {
		cwd: repo,
		encoding: "utf8",
	});
	if (r.status !== 0) throw new Error(`i18n:generate failed: ${r.stderr || r.stdout}`);
}

/** Vite plugin: regenerate `.i18n` when a message under i18n/messages changes, so Paraglide recompiles. */
export function messagesWatcher() {
	const dir = join(repo, "i18n/messages");
	return {
		name: "atlas-i18n-generate",
		configureServer(server: {
			watcher: { add(p: string): void; on(e: string, f: (p: string) => void): void };
		}) {
			server.watcher.add(dir);
			for (const ev of ["add", "change", "unlink"])
				server.watcher.on(ev, (f) => f.startsWith(dir) && generateMessages());
		},
	};
}

const newest = (dir: string) => Math.max(0, ...readdirSync(dir).map((f) => statSync(join(dir, f)).mtimeMs));

/** Whether `src/lib/paraglide` is missing or older than the generated inputs and the project settings. */
export function isStale(): boolean {
	let built: number;
	try {
		built = statSync(join(root, PARAGLIDE.outdir, "messages.js")).mtimeMs;
	} catch {
		return true;
	}
	return Math.max(newest(join(root, ".i18n")), newest(join(root, "project.inlang"))) > built;
}

/** Compile the catalogs to `src/lib/paraglide`; with `onlyIfStale`, only when they changed since. */
export async function compileMessages(onlyIfStale = false): Promise<void> {
	generateMessages();
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
