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
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { PSEUDO_LOCALE, pseudoEnabled } from "./pseudo-env.ts";

export { PSEUDO_LOCALE, pseudoEnabled };

const root = fileURLToPath(new URL("..", import.meta.url));

/** The committed project (en, pt-BR): a release compiles this one, untouched. */
const PROJECT = "./project.inlang";
/** A development build compiles this generated one: the committed settings plus the pseudo-locale. Gitignored (`.i18n/`). */
const DEV_PROJECT = "./.i18n/project.inlang";

/**
 * The inlang settings of a development build: the committed ones with `en-XA` appended to `locales`. The settings
 * file sits in `.i18n/`, so its message path is relative to that folder and its modules are absolute.
 */
export function devProjectSettings(committed: { locales: string[]; modules?: string[]; [k: string]: unknown }, base = root) {
	return {
		...committed,
		locales: [...committed.locales, PSEUDO_LOCALE],
		modules: (committed.modules ?? []).map((m) => join(base, m)),
		"plugin.inlang.messageFormat": { ...(committed["plugin.inlang.messageFormat"] as object), pathPattern: "./{locale}.json" },
	};
}

/** The ONE options object of Paraglide for a build kind (`pseudo`: see `pseudoEnabled`). */
export function paraglideOptions(pseudo: boolean): Parameters<typeof compile>[0] {
	return {
		project: pseudo ? DEV_PROJECT : PROJECT,
		outdir: "./src/lib/paraglide",
		outputStructure: "message-modules",
		// The locale is a Svelte `$state` (`lib/i18n/locale.svelte.ts`) so a switch re-renders without a reload; never the
		// navigator's language (the Rust side resolves it once: `atlas_core::lang`).
		strategy: ["custom-atlas", "baseLocale"],
		emitTsDeclarations: true,
	};
}

const repo = fileURLToPath(new URL("../../..", import.meta.url));

/**
 * Write `.i18n/{locale}.json` from i18n/messages (only the files that changed, so mtimes stay put when nothing did).
 * Always `bun`, never `process.execPath`: `svelte-kit sync` and `vite` load vite.config.ts (and so this file) in Node,
 * which cannot run the generator's TypeScript.
 */
export function generateMessages(pseudo = pseudoEnabled()): void {
	const run = (script: string, ...args: string[]) => {
		const r = spawnSync("bun", [join(repo, script), "--surface", "desktop", ...args], { cwd: repo, encoding: "utf8" });
		if (r.status !== 0) throw new Error(`${script} failed: ${r.stderr || r.stdout}`);
	};
	run("scripts/i18n/generate.ts");
	const out = join(root, ".i18n");
	if (pseudo) {
		// Generated on every run, from the English just written, so en-XA never goes stale.
		run("scripts/i18n/pseudo.ts");
		const settings = devProjectSettings(JSON.parse(readFileSync(join(root, "project.inlang/settings.json"), "utf8")));
		mkdirSync(join(out, "project.inlang"), { recursive: true });
		const file = join(out, "project.inlang/settings.json");
		const next = JSON.stringify(settings, null, "\t") + "\n";
		let current = "";
		try {
			current = readFileSync(file, "utf8");
		} catch {}
		if (next !== current) writeFileSync(file, next);
	} else {
		// A release build has no trace of it: not the catalog, not the project that lists it.
		rmSync(join(out, `${PSEUDO_LOCALE}.json`), { force: true });
		rmSync(join(out, "project.inlang"), { recursive: true, force: true });
	}
	writeOsMessages();
}

/** Where `m` gets its messages from: `lib/i18n/m.ts` re-exports it as a namespace. Generated, gitignored. */
const OS_MESSAGES = join(root, "src/lib/i18n/messages.gen.ts");

/**
 * Write `src/lib/i18n/messages.gen.ts`: Paraglide's messages, with `os` added to the inputs of the ones that have it.
 *
 * It is generated, not a Proxy over the namespace, because a bundler cannot see through a Proxy: one wrapper object
 * made every message of the catalog land in the startup chunk (#468). Here each wrapper is its own `/*#__PURE__*\/`
 * export, so a message that no screen reaches is dropped, and the rest ends up in the chunk of the screens that use it.
 * Only the messages with an `os` input need a wrapper; the others stay Paraglide's own functions (`export *`).
 * A message has an `os` input when any language of the catalog declares one.
 */
export function writeOsMessages(): void {
	const dir = join(root, ".i18n");
	const keys = new Set<string>();
	for (const file of readdirSync(dir).filter((f) => f.endsWith(".json")))
		for (const [key, value] of Object.entries(JSON.parse(readFileSync(join(dir, file), "utf8")) as Record<string, unknown>))
			if (/"input os"/.test(JSON.stringify(value))) keys.add(key);
	const lines = [
		"// Generated by scripts/paraglide.ts (`bun run i18n:compile`); never edit by hand, never commit.",
		"// Paraglide's messages, with `os` added to the inputs of the ones that have it. See lib/i18n/withOs.ts and m.ts.",
		'import { OS } from "#lib/keys.ts";',
		'import * as g from "#lib/paraglide/messages/_index.js";',
		'import { withOs } from "./withOs.ts";',
		"",
		'export * from "#lib/paraglide/messages/_index.js";',
		"",
		"// A cycle with keys.ts (it builds text through `m`) that is safe: `OS` is only read when a message is called.",
		"const os = () => OS;",
		...[...keys].sort().map((k) => `export const ${k} = /*#__PURE__*/ withOs(g.${k}, os);`),
		"",
	];
	const next = lines.join("\n");
	let current = "";
	try {
		current = readFileSync(OS_MESSAGES, "utf8");
	} catch {}
	if (next !== current) writeFileSync(OS_MESSAGES, next);
}

/** Vite plugin: regenerate `.i18n` when a message under i18n/messages changes, so Paraglide recompiles. */
export function messagesWatcher(pseudo: boolean) {
	const dir = join(repo, "i18n/messages");
	return {
		name: "atlas-i18n-generate",
		configureServer(server: {
			watcher: { add(p: string): void; on(e: string, f: (p: string) => void): void };
		}) {
			server.watcher.add(dir);
			for (const ev of ["add", "change", "unlink"])
				server.watcher.on(ev, (f) => f.startsWith(dir) && generateMessages(pseudo));
		},
	};
}

const newest = (dir: string) => Math.max(0, ...readdirSync(dir).map((f) => statSync(join(dir, f)).mtimeMs));

/**
 * Whether `src/lib/paraglide` is missing, older than the generated inputs and the project settings, or compiled for the
 * other build kind (a release compile must not keep en-XA from a dev run, nor the reverse).
 */
export function isStale(pseudo = pseudoEnabled()): boolean {
	const outdir = paraglideOptions(pseudo).outdir ?? "";
	let built: number;
	try {
		built = statSync(join(root, outdir, "messages.js")).mtimeMs;
		const runtime = readFileSync(join(root, outdir, "runtime.js"), "utf8");
		if (runtime.includes(`"${PSEUDO_LOCALE}"`) !== pseudo) return true;
	} catch {
		return true;
	}
	return Math.max(newest(join(root, ".i18n")), newest(join(root, "project.inlang"))) > built;
}

/** Compile the catalogs to `src/lib/paraglide`; with `onlyIfStale`, only when they changed since. */
export async function compileMessages(onlyIfStale = false, pseudo = pseudoEnabled()): Promise<void> {
	generateMessages(pseudo);
	if (onlyIfStale && !isStale(pseudo)) return;
	const cwd = process.cwd();
	process.chdir(root);
	try {
		await compile({ ...paraglideOptions(pseudo) });
	} finally {
		process.chdir(cwd);
	}
}

if (import.meta.main) await compileMessages();
