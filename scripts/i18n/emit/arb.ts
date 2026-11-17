// Flutter ARB output (the future mobile surface): ICU plural/select messages and `@key.description`. Nothing declares
// an `arb` surface yet; the generator and its tests exist so the mobile PR is a surfaces.json entry.
import { type Node, buildTree, normalize, type MsgValue } from "../resolve.ts";

/** ICU quoting: a literal apostrophe is doubled, an escaped brace is quoted. */
const icuText = (text: string) => text.replace(/'/g, "''").replace(/\\([{}])/g, "'$1'");

type SelectNode = Extract<Node, { sel: unknown }>;

function icu(n: Node): string {
	if ("text" in n) return icuText(n.text);
	const sel = n as SelectNode;
	const keyOf = (l: string) => (sel.sel.kind === "plural" && /^\d+$/.test(l) ? `=${l}` : l);
	const arms = [...sel.branches.map((b) => `${keyOf(b.literal)}{${icu(b.node)}}`), `other{${icu(sel.fallback)}}`];
	return `{${sel.sel.var}, ${sel.sel.kind === "plural" ? "plural" : "select"}, ${arms.join(" ")}}`;
}

function varsOf(n: Node, out = new Map<string, "num" | "String" | "Object">()) {
	if ("text" in n) {
		for (const m of n.text.matchAll(/(?<!\\)\{([A-Za-z_]\w*)\}/g)) if (!out.has(m[1])) out.set(m[1], "Object");
		return out;
	}
	const sel = n as SelectNode;
	out.set(sel.sel.var, sel.sel.kind === "plural" ? "num" : "String");
	for (const b of sel.branches) varsOf(b.node, out);
	varsOf(sel.fallback, out);
	return out;
}

/** One message as its ARB entries: the ICU string and its `@key` metadata. */
export function arbMessage(key: string, value: MsgValue, description?: string): Record<string, unknown> {
	const tree = buildTree(normalize(value));
	const vars = varsOf(tree);
	const meta: Record<string, unknown> = {};
	if (description) meta.description = description;
	if (vars.size) meta.placeholders = Object.fromEntries([...vars].map(([name, type]) => [name, type === "Object" ? {} : { type }]));
	return { [key]: icu(tree), ...(Object.keys(meta).length ? { [`@${key}`]: meta } : {}) };
}

/** A whole `app_<locale>.arb`: `@@locale` (underscore form, as Flutter wants), then the messages by key. */
export function arbFile(locale: string, messages: { key: string; value: MsgValue; description?: string }[]): string {
	const entries = [...messages].sort((a, b) => (a.key < b.key ? -1 : 1)).flatMap((m) => Object.entries(arbMessage(m.key, m.value, m.description)));
	return `${JSON.stringify({ "@@locale": locale.replace("-", "_"), ...Object.fromEntries(entries) }, null, "\t")}\n`;
}
