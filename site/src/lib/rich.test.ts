// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { describe, expect, test } from "bun:test";
import { richHtml } from "./rich";
import type { MessagePart } from "./paraglide/runtime.js";

const text = (value: string): MessagePart => ({ type: "text", value });
const start = (name: string): MessagePart => ({ type: "markup-start", name, options: {}, attributes: {} });
const end = (name: string): MessagePart => ({ type: "markup-end", name, options: {}, attributes: {} });
const standalone = (name: string): MessagePart => ({ type: "markup-standalone", name, options: {}, attributes: {} });

describe("richHtml", () => {
	test("draws a tag around its content and escapes text, inside tags too", () => {
		const parts = [text("Run "), start("code"), text("shasum <file> && x"), end("code"), text(" now")];
		expect(richHtml(parts, { code: "code" })).toBe("Run <code>shasum &lt;file&gt; &amp;&amp; x</code> now");
	});

	test("a link gets its attributes, escaped", () => {
		const parts = [start("link"), text("guide"), end("link")];
		expect(richHtml(parts, { link: { el: "a", attrs: { class: "ln", href: '/docs/"x"/?a=1&b=2' } } })).toBe('<a class="ln" href="/docs/&quot;x&quot;/?a=1&amp;b=2">guide</a>');
	});

	test("a tag the caller did not map renders its content plain, nested tags keep working", () => {
		const parts = [start("unknown"), text("a "), start("b"), text("bold"), end("b"), end("unknown")];
		expect(richHtml(parts, { b: "b" })).toBe("a <b>bold</b>");
	});

	test("a standalone tag is an empty element, an unmapped one nothing", () => {
		expect(richHtml([text("a"), standalone("br"), text("b"), standalone("nope")], { br: "br" })).toBe("a<br></br>b");
	});

	test("unbalanced parts do not throw: a stray closing part is dropped, an open tag is closed at the end", () => {
		expect(richHtml([text("x"), end("b"), start("b"), text("y")], { b: "b" })).toBe("x<b>y</b>");
		expect(richHtml([start("b"), start("i"), text("z")], { b: "b", i: "i" })).toBe("<b><i>z</i></b>");
	});
});
