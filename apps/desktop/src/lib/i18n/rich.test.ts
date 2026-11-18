// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { m } from "./m.ts";
import { richTree, type RichNode } from "./rich.ts";
import { inLocale } from "../../../test/locale.ts";

const text = (nodes: RichNode[]): string =>
	nodes.map((n) => (n.kind === "text" ? n.value : text(n.children))).join("");
const tagged = (nodes: RichNode[], name: string) =>
	nodes.flatMap((n) =>
		n.kind === "tag" ? [...(n.name === name ? [text(n.children)] : []), ...tagged(n.children, name)] : [],
	);

test("a real markup message becomes one tree whose tags hold the highlighted words", () => {
	const tree = richTree(m.mobile_pair_card_step_open_hint.parts());
	expect(text(tree)).toBe("Tap Scan pairing code, or Pair another computer in its Settings.");
	expect(tagged(tree, "em")).toEqual(["Scan pairing code", "Pair another computer"]);
});

test("the same message in another language keeps its tags around the words that follow, wherever they move", async () => {
	await inLocale("pt-BR", () => {
		const tree = richTree(m.mobile_pair_card_step_scan_hint.parts());
		expect(tagged(tree, "em")).toEqual(["Celulares pareados"]);
		expect(text(tree)).toBe(m.mobile_pair_card_step_scan_hint());
	});
});

test("tags nest, a standalone tag has no children, and a message that is not balanced still renders", () => {
	const open = (name: string) => ({ type: "markup-start", name, options: {}, attributes: {} });
	const close = (name: string) => ({ type: "markup-end", name, options: {}, attributes: {} });
	const t = (value: string) => ({ type: "text", value });
	expect(richTree([t("a "), open("b"), t("b "), open("i"), t("c"), close("i"), close("b"), t(" d")])).toEqual(
		[
			{ kind: "text", value: "a " },
			{
				kind: "tag",
				name: "b",
				children: [
					{ kind: "text", value: "b " },
					{ kind: "tag", name: "i", children: [{ kind: "text", value: "c" }] },
				],
			},
			{ kind: "text", value: " d" },
		],
	);
	expect(richTree([{ type: "markup-standalone", name: "br", options: {}, attributes: {} }])).toEqual([
		{ kind: "tag", name: "br", children: [] },
	]);
	// A closing part nothing opened is dropped; a tag left open is closed at the end.
	expect(richTree([close("x"), t("a"), open("b"), t("c")])).toEqual([
		{ kind: "text", value: "a" },
		{ kind: "tag", name: "b", children: [{ kind: "text", value: "c" }] },
	]);
});
