// Small shared helpers of scripts/i18n: file walking, repo-relative paths, canonical JSON.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

const SKIP_DIRS = new Set(["node_modules", ".git", "target", ".svelte-kit", ".astro", "dist", ".wrangler"]);

/** Every file under `dir` (absolute paths, sorted) for which `pred(absolutePath)` holds. A missing dir is empty. */
export function walk(dir: string, pred: (path: string) => boolean = () => true): string[] {
	const out: string[] = [];
	const visit = (d: string) => {
		for (const e of readdirSync(d, { withFileTypes: true })) {
			const p = join(d, e.name);
			if (e.isDirectory()) {
				if (!SKIP_DIRS.has(e.name)) visit(p);
			} else if (pred(p)) out.push(p);
		}
	};
	if (existsSync(dir) && statSync(dir).isDirectory()) visit(dir);
	return out.sort();
}

/** Repo-relative path with forward slashes (what findings print and baselines key on). */
export const rel = (root: string, path: string) => relative(root, path).split(sep).join("/");

export const readText = (path: string) => readFileSync(path, "utf8");

/** 1-based line of a character offset. */
export function lineOf(text: string, offset: number): number {
	let n = 1;
	for (let i = 0; i < offset && i < text.length; i++) if (text.charCodeAt(i) === 10) n++;
	return n;
}

/** Plain code-point order, the one sort every catalog, lock and baseline in i18n/ uses. */
export const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

/** The indentation a JSON file already uses (tab or N spaces), so formatting never fights the inlang plugin. */
export function detectIndent(text: string): string {
	const m = /^([ \t]+)\S/m.exec(text);
	return m ? m[1] : "\t";
}

/** `obj` with keys in code-point order, `$schema` first, one key per line, LF, trailing newline. */
export function canonicalJson(obj: Record<string, unknown>, indent = "\t"): string {
	const keys = Object.keys(obj).sort((a, b) => (a === "$schema" ? -1 : b === "$schema" ? 1 : cmp(a, b)));
	const sorted: Record<string, unknown> = {};
	for (const k of keys) sorted[k] = obj[k];
	return `${JSON.stringify(sorted, null, indent)}\n`;
}

/** Glob with `*` wildcards (`settings_*`) to an anchored RegExp. */
export function globToRegExp(glob: string): RegExp {
	return new RegExp(`^${glob.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*")}$`);
}
