/**
 * A message with markup tags (`Tap {#em}Scan{/em}`, written `{#tag}..{/tag}` in the catalog) is not a string: Paraglide
 * compiles it to `m.key.parts(inputs)`, a flat list of text and tag-open / tag-close parts. `richTree` nests that list so
 * `Rich.svelte` can draw each tag with the caller's element and still render the sentence as ONE message, so a
 * translation may move or reorder the highlighted span freely. Never split a sentence around an element in a template.
 */
import type { MessagePart } from "#lib/paraglide/runtime.js";

export type RichNode = { kind: "text"; value: string } | { kind: "tag"; name: string; children: RichNode[] };

/**
 * `parts` as a tree. A `{#tag/}` with no closing part (standalone) is a tag with no children; a closing part that
 * matches nothing is dropped, and tags still open at the end are closed there (the catalog check rejects both, so
 * this only keeps a bad message from throwing at render time).
 */
export function richTree(parts: MessagePart[]): RichNode[] {
	const root: RichNode[] = [];
	const open: { name: string; children: RichNode[] }[] = [];
	const into = () => open[open.length - 1]?.children ?? root;
	for (const p of parts) {
		if (p.type === "text") into().push({ kind: "text", value: p.value });
		else if (p.type === "markup-standalone") into().push({ kind: "tag", name: p.name, children: [] });
		else if (p.type === "markup-start") {
			const tag = { name: p.name, children: [] as RichNode[] };
			into().push({ kind: "tag", ...tag });
			open.push(tag);
		} else if (open.some((t) => t.name === p.name)) {
			while (open.pop()?.name !== p.name);
		}
	}
	return root;
}
