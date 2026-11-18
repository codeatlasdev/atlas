// How web sources reach the messages, in ONE place (`unused.ts` reads it to check references, `mv.ts` to rewrite them).
// A file reaches a message through a binding of the messages module:
//   import { m } from "#lib/i18n/m.ts"            the desktop's wrapper (also `$lib/…`, relative, `./m.ts`)
//   import { m } from ".../paraglide/messages.js" the site, and Paraglide's own output
//   import * as compiled from "…/messages.js"     a namespace under any name; `{ m as msg }` renames `m`
//   import { app_x } from ".../messages.js"       one message function, by name (`app_x as y` renames it)
// A local variable called `m` in a file with no such import is no message: callers only look at files with a binding.

/** The specifier of a module that exports the messages. */
export const MESSAGE_MODULE = /(?:^|\/)(?:paraglide\/messages|i18n\/m|messages\.gen)(?:\.[jt]s)?$|^\.\/m(?:\.[jt]s)?$/;

/** `import <clause> from "<specifier>"`, not `import type`. */
export const IMPORT = /import\s+(?!type\b)([^;'"]*?)\s*from\s*["']([^"']+)["']/g;

export interface MessageBindings {
	/** Local names that stand for the whole messages namespace (`m`, `msg`, `compiled`). */
	spaces: string[];
	/** Single messages imported by name: the exported name and the local one. */
	named: { imported: string; local: string }[];
}

export function messageBindings(text: string): MessageBindings {
	const out: MessageBindings = { spaces: [], named: [] };
	for (const [, clause, spec] of text.matchAll(IMPORT)) {
		if (!MESSAGE_MODULE.test(spec)) continue;
		const ns = /\*\s*as\s+([\w$]+)/.exec(clause);
		if (ns) out.spaces.push(ns[1]);
		for (const part of /\{([^}]*)\}/.exec(clause)?.[1].split(",") ?? []) {
			const p = part.trim();
			if (!p || /^type\s/.test(p)) continue;
			const [imported, local = imported] = p.split(/\s+as\s+/);
			if (imported === "m") out.spaces.push(local);
			else out.named.push({ imported, local });
		}
	}
	return out;
}

export const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Regex source up to the key of `space.key` / `space?.key` (an identifier that is not itself a member of something). */
export const memberHead = (space: string) => `(?<![\\w$.])${escapeRe(space)}\\??\\.`;

/** Regex source up to the key of `space["key"]` (also `space?.["key"]`, single or back quotes). */
export const bracketHead = (space: string) => `(?<![\\w$.])${escapeRe(space)}\\??\\.?\\[\\s*["'\`]`;
