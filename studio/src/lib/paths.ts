import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const STUDIO = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const REPO = resolve(STUDIO, "..");
export const CACHE = join(STUDIO, ".cache");
export const BUILD = join(STUDIO, ".build");
export const OUT = join(STUDIO, "out");
export const FILMS = join(STUDIO, "films");
export const SITE_MEDIA = join(REPO, "site/public/media");

/**
 * Secrets come only from the process environment. For convenience the studio also reads `KEY=value` lines of the
 * files in `ATLAS_STUDIO_ENV` (colon-separated, default `~/.config/atlas-site/secrets.env`) into `process.env` at
 * start, without overriding variables that are already set. Nothing is ever written back or logged.
 */
export function loadEnv(): void {
	const files = (process.env.ATLAS_STUDIO_ENV ?? `${process.env.HOME}/.config/atlas-site/secrets.env`)
		.split(":")
		.filter(Boolean);
	for (const f of files) {
		if (!existsSync(f)) continue;
		for (const line of readFileSync(f, "utf8").split("\n")) {
			const m = /^\s*(?:export\s+)?([A-Z][A-Z0-9_]*)=(.*)$/.exec(line);
			if (!m) continue;
			const [, k, raw] = m as unknown as [string, string, string];
			const v = raw.trim().replace(/^(['"])(.*)\1$/, "$2");
			if (v && process.env[k] === undefined) process.env[k] = v;
		}
	}
}

export function secret(name: string): string {
	const v = process.env[name];
	if (!v) {
		throw new StudioError(
			`${name} is not set`,
			`export it, or put ${name}=… in a file listed in ATLAS_STUDIO_ENV (default ~/.config/atlas-site/secrets.env). It is read at run time only and never committed.`
		);
	}
	return v;
}

/** An error with a fix hint; the CLI prints both and exits 1 without a stack trace. */
export class StudioError extends Error {
	constructor(
		message: string,
		public hint?: string
	) {
		super(message);
		this.name = "StudioError";
	}
}
