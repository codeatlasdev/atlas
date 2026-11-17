// Which message keys the app's source references. A key nobody references is dead weight in every catalog.
// References: `m.key`, `m["key"]`, `messages_key`, and names in `import { key } from ".../paraglide/messages"`.
// Dynamic access (`m[name]`) cannot be seen; annotate it where it happens: `// i18n-keys: settings_tab_*, other_key`.
import { globToRegExp, readText, walk } from "./util.ts";

const SOURCE = /\.(?:svelte|ts|tsx|js|jsx|mjs|astro)$/;

export interface Refs {
	names: Set<string>;
	patterns: RegExp[];
}

export function scanRefs(srcDir: string): Refs {
	const names = new Set<string>();
	const patterns: RegExp[] = [];
	for (const path of walk(srcDir, (p) => SOURCE.test(p) && !/[\\/]paraglide[\\/]/.test(p))) {
		const text = readText(path);
		for (const m of text.matchAll(/\bm\.([A-Za-z_]\w*)/g)) names.add(m[1]);
		for (const m of text.matchAll(/\bm\[\s*["'`]([^"'`]+)["'`]\s*\]/g)) names.add(m[1]);
		for (const m of text.matchAll(/\bmessages_([A-Za-z_]\w*)/g)) names.add(m[1]);
		for (const m of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*["'][^"']*paraglide\/messages[^"']*["']/g)) {
			for (const part of m[1].split(",")) {
				const name = part.trim().split(/\s+as\s+/)[0];
				if (name) names.add(name);
			}
		}
		for (const m of text.matchAll(/i18n-keys:\s*([^\n]*?)(?:-->|\*\/|$)/gm)) {
			for (const g of m[1].split(/[\s,]+/).filter(Boolean)) patterns.push(globToRegExp(g));
		}
	}
	return { names, patterns };
}

export const isUsed = (refs: Refs, key: string) => refs.names.has(key) || refs.patterns.some((p) => p.test(key));
