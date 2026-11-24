// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, spyOn, test } from "bun:test";
import { marked } from "marked";
import { markdownBlocks } from "./markdownBlocks.ts";

// Incident (2026-11-23, WebkitCpu4's profile of `chat-stream`, release build): `markdownBlocks` re-lexed the WHOLE message
// on every flush, ~18 % of the page's JS while an answer streams, and the cost grows with the message (quadratic over a
// long answer). Only the open tail block can change between two flushes, so only it may be lexed again.

const OPTIONS = { async: false, breaks: true } as const;

/** The full-lex reference: what every flush used to do. */
function fullLex(text: string): string[] {
	const tokens = marked.lexer(text, OPTIONS);
	return tokens
		.filter((t) => t.type !== "space")
		.map((t) => marked.parser(Object.assign([t], { links: tokens.links }), OPTIONS));
}

const FENCE = "```";
/** Every block kind whose end depends on what follows it: the cases an incremental lexer gets wrong. */
const CORPUS = [
	"# Title\n\nA paragraph with **bold**, `code` and a [link](https://example.com).\nA second line.\n\n",
	"Setext heading\n---\n\nAnother\n===\n\n",
	"- one\n- two\n  - nested a\n  - nested b\n- three\n\n",
	"- loose one\n\n- loose two\n\n  continued paragraph\n\n- loose three\n\n",
	"1. first\n2. second\n\n   indented under second\n3. third\n\n",
	`${FENCE}ts\nconst a = 1;\n\nconst b = 2;\n${FENCE}\n\n`,
	"> a quote\n> over two lines\n>\n> and a second paragraph\n\n",
	"| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\n",
	"<div>\nraw *html*\n</div>\n\n",
	"---\n\n",
	"    indented code\n    more code\n\n",
	'Text with a ref [foo] and [bar][baz].\n\n[foo]: https://foo.example "Foo title"\n[baz]: https://baz.example\n\n',
	"- [ ] task\n- [x] done\n\n",
	"Final paragraph that ends without a newline",
];
const MESSAGE = CORPUS.join("");

test("equivalence: every prefix of a streamed message renders the same HTML as a full lex", () => {
	const blocks = markdownBlocks();
	for (let end = 0; end <= MESSAGE.length; end++) {
		const prefix = MESSAGE.slice(0, end);
		expect(blocks(prefix)).toEqual(fullLex(prefix));
	}
});

test("equivalence: seeded shuffles of every block kind, every prefix (what ends a block depends on what follows it)", () => {
	const pieces = [
		...CORPUS,
		"para\n\n    code after para\n\n",
		"> lazy\ncontinued\n\n",
		"- a\n\n    code in item\n\n",
		"text\n- list right after\n\n",
		"<!-- c -->\n\n",
		"* * *\n\n",
		"a  \nb\n\n\n\n",
		"1) x\n2) y\n\n",
		"~~~\nopen fence\n\nmore",
	];
	let seed = 7;
	const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
	for (let round = 0; round < 12; round++) {
		const text = Array.from({ length: 6 }, () => pieces[Math.floor(rnd() * pieces.length)]).join("");
		const blocks = markdownBlocks();
		for (let end = 0; end <= text.length; end += 1 + (end % 3))
			expect(blocks(text.slice(0, end))).toEqual(fullLex(text.slice(0, end)));
		expect(blocks(text)).toEqual(fullLex(text));
	}
});

test("equivalence: a definition that arrives after a block that uses it still rewrites that block", () => {
	// A frozen block `[foo]` is plain text until `[foo]: url` shows up two blocks later.
	const text = "see [foo]\n\nsecond block\n\nthird block\n\n[foo]: https://foo.example\n";
	const blocks = markdownBlocks();
	for (let end = 0; end <= text.length; end++)
		expect(blocks(text.slice(0, end))).toEqual(fullLex(text.slice(0, end)));
	expect(blocks(text)[0]).toContain('<a href="https://foo.example">foo</a>');
});

test("equivalence: CRLF text, and a text that is not an extension of the previous one", () => {
	const blocks = markdownBlocks();
	const crlf = "# T\r\n\r\npara one\r\nline two\r\n\r\n- a\r\n- b\r\n\r\nend";
	for (let end = 0; end <= crlf.length; end++)
		expect(blocks(crlf.slice(0, end))).toEqual(fullLex(crlf.slice(0, end)));
	// The message is replaced (a retry, an edit, another message in the same component): never reuse the old blocks.
	for (const t of ["other\n\ntext\n\nhere", "other\n\ntext", "unrelated", "", "other\n\ntext\n\nhere!"])
		expect(blocks(t)).toEqual(fullLex(t));
	expect(blocks(MESSAGE.slice(0, 200))).toEqual(fullLex(MESSAGE.slice(0, 200)));
});

test("a streamed message lexes only the open tail block on each flush, not the whole text", () => {
	const lexer = spyOn(marked, "lexer");
	try {
		const blocks = markdownBlocks();
		const text = MESSAGE.replace("\n\n[foo]:", "\n\n[foo-gone]").replace(/\n\[baz\]: .*\n/, "\n"); // no definitions: the cached path
		const chunk = 7;
		let flushes = 0;
		for (let end = chunk; end < text.length + chunk; end += chunk, flushes++) blocks(text.slice(0, end));
		const lexed = lexer.mock.calls.reduce((n, [src]) => n + src.length, 0);
		const longestBlock = Math.max(...marked.lexer(text, OPTIONS).map((t) => t.raw.length));
		// Before: sum of all prefixes, ~ flushes * text.length / 2. Now: a flush lexes the open block plus what just arrived.
		expect(lexed).toBeLessThanOrEqual(flushes * (longestBlock + chunk));
		expect(lexed).toBeLessThan((flushes * text.length) / 4);
	} finally {
		lexer.mockRestore();
	}
});

test("closed blocks keep the very same HTML strings between flushes (the view keys its DOM by them)", () => {
	const blocks = markdownBlocks();
	const a = blocks("first\n\nsecond\n\nthi");
	const b = blocks("first\n\nsecond\n\nthird part");
	expect(b.slice(0, 2)).toEqual(a.slice(0, 2));
	expect(b[2]).not.toEqual(a[2]);
});
