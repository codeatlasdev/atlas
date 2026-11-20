// A message with markup (`The {#code}atlas{/code} command`, see i18n/README.md) compiles to `m.key.parts(inputs)`: a flat list of text and
// tag parts. This turns that list into HTML for Astro, so a sentence with code spans or links stays ONE message and a translation may
// move or reorder the tagged span. The caller maps each tag name to an element; a tag it does not map renders its content plain.
// Pure (no Paraglide imports at runtime) so bun test can run it.
import type { MessagePart } from "./paraglide/runtime.js";

/** An element name (`"code"`), or an element with attributes (`{ el: "a", attrs: { href: "/x/" } }`). */
export type RichTag = string | { el: string; attrs?: Record<string, string> };

const escText = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const escAttr = (s: string) => escText(s).replace(/"/g, "&quot;");

export function richHtml(parts: MessagePart[], tags: Record<string, RichTag>): string {
	let out = "";
	// Closing tags owed, innermost last: a part that closes a tag nobody opened (or one the caller did not map) emits nothing.
	const open: (string | null)[] = [];
	for (const p of parts) {
		if (p.type === "text") {
			out += escText(p.value);
			continue;
		}
		const tag = tags[p.name];
		const el = typeof tag === "string" ? tag : tag?.el;
		const attrs = typeof tag === "object" ? Object.entries(tag.attrs ?? {}).map(([k, v]) => ` ${k}="${escAttr(v)}"`).join("") : "";
		if (p.type === "markup-start") {
			open.push(el ?? null);
			if (el) out += `<${el}${attrs}>`;
		} else if (p.type === "markup-end") {
			const closing = open.pop();
			if (closing) out += `</${closing}>`;
		} else if (el) {
			out += `<${el}${attrs}></${el}>`;
		}
	}
	// Tags still open at the end are closed there (the catalog check rejects such a message; this only keeps a bad one from breaking the page).
	for (const closing of open.reverse()) if (closing) out += `</${closing}>`;
	return out;
}
