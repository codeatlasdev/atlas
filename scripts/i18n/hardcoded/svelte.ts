// Hardcoded user-facing text in Svelte templates, found on the real svelte/compiler AST: Text nodes with letters
// and static `title`/`placeholder`/`aria-label`/... attributes. Not flagged: `<code>`, `<pre>`, `<kbd>`, `<style>`,
// `<script>`, text without letters, and whatever follows a `<!-- i18n-ignore -->` comment (the next sibling).
import { parse } from "svelte/compiler";
import type { Found, Violation } from "../baseline.ts";
import { lineOf, readText, rel, walk } from "../util.ts";

/** Where user-facing Svelte lives, per surface name (the app directory's basename). */
export const SVELTE_ROOTS: { surface: string; dir: string }[] = [
	{ surface: "desktop", dir: "apps/desktop/src" },
	{ surface: "site", dir: "site/src" },
];

const ATTRS = new Set(["title", "placeholder", "aria-label", "aria-description", "alt", "label", "tooltip", "description", "message"]);
const SKIP_ELEMENTS = new Set(["code", "pre", "kbd", "style", "script"]);
const LETTERS = /\p{L}/u;

interface SNode {
	type: string;
	start: number;
	[k: string]: unknown;
}

const asNode = (v: unknown): SNode | undefined =>
	typeof v === "object" && v !== null && "type" in v && typeof v.type === "string" && "start" in v && typeof v.start === "number" ? (v as SNode) : undefined;

function fragmentNodes(v: unknown): unknown[] | undefined {
	if (typeof v !== "object" || v === null || !("type" in v) || v.type !== "Fragment") return undefined;
	return "nodes" in v && Array.isArray(v.nodes) ? v.nodes : undefined;
}

/** Static text of an attribute value, or undefined when it holds an expression (`title={x}`) or is a bare flag. */
function staticText(value: unknown): string | undefined {
	const parts = Array.isArray(value) ? value : [value];
	let text = "";
	for (const p of parts) {
		const n = asNode(p);
		if (n?.type !== "Text" || typeof n.data !== "string") return undefined;
		text += n.data;
	}
	return text;
}

export function scanSvelteSource(source: string): Violation[] {
	const out: Violation[] = [];
	const flag = (offset: number, text: string) => out.push({ line: lineOf(source, offset), text: text.trim().replace(/\s+/g, " ").slice(0, 60) });
	const visit = (nodes: unknown[]) => {
		let ignoreNext = false;
		for (const raw of nodes) {
			const n = asNode(raw);
			if (!n) continue;
			if (n.type === "Comment") {
				if (typeof n.data === "string" && /^\s*i18n-ignore\b/.test(n.data)) ignoreNext = true;
				continue;
			}
			if (n.type === "Text" && typeof n.data === "string" && n.data.trim() === "") continue; // whitespace keeps the flag
			const skip = ignoreNext;
			ignoreNext = false;
			if (skip) continue;
			if (n.type === "Text") {
				if (typeof n.data === "string" && LETTERS.test(n.data)) flag(n.start, n.data);
				continue;
			}
			if (typeof n.name === "string" && SKIP_ELEMENTS.has(n.name)) continue;
			if (Array.isArray(n.attributes)) {
				for (const a of n.attributes) {
					const attr = asNode(a);
					if (attr?.type !== "Attribute" || typeof attr.name !== "string" || !ATTRS.has(attr.name)) continue;
					const text = staticText(attr.value);
					if (text !== undefined && LETTERS.test(text)) flag(attr.start, `${attr.name}="${text}"`);
				}
			}
			for (const child of Object.values(n)) {
				const nodes = fragmentNodes(child);
				if (nodes) visit(nodes);
			}
		}
	};
	const nodes = fragmentNodes(parse(source, { modern: true }).fragment);
	if (nodes) visit(nodes);
	return out;
}

/** Every `.svelte` file under `dirs` (repo-relative): its violations, files with none omitted. Unparsable files
 * come back in `unparsable` so a scanner that cannot read a file never reports it clean. */
export function scanSvelte(root: string, dirs: string[]): { found: Found; unparsable: { file: string; message: string }[] } {
	const found: Found = new Map();
	const unparsable: { file: string; message: string }[] = [];
	for (const dir of dirs) {
		for (const path of walk(`${root}/${dir}`, (p) => p.endsWith(".svelte"))) {
			const file = rel(root, path);
			try {
				const v = scanSvelteSource(readText(path));
				if (v.length) found.set(file, v);
			} catch (e) {
				unparsable.push({ file, message: (e as Error).message });
			}
		}
	}
	return { found, unparsable };
}
