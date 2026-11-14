// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { parseAsk } from "./parseAsk.ts";

describe("parseAsk", () => {
	const ask = { id: "a1", question: "Which?", options: [{ key: "x", title: "X" }], allow_text: false };

	test("reads the ask payload from a JSON result", () => {
		expect(parseAsk(JSON.stringify({ ask }))).toEqual(ask);
	});

	test("tolerates leading whitespace before the brace", () => {
		expect(parseAsk(`\n  ${JSON.stringify({ ask })}`)).toEqual(ask);
	});

	test("a plain markdown reply is not an ask", () => {
		expect(parseAsk("Done. The branch is main.")).toBeNull();
	});

	test("undefined and empty are not an ask", () => {
		expect(parseAsk(undefined)).toBeNull();
		expect(parseAsk("")).toBeNull();
	});

	test("JSON without an ask key is not an ask", () => {
		expect(parseAsk('{"result":"ok"}')).toBeNull();
	});

	test("broken JSON that starts with a brace is not an ask", () => {
		expect(parseAsk('{"ask": {"id":')).toBeNull();
	});

	test("a JSON array or scalar is not an ask", () => {
		expect(parseAsk("[1,2]")).toBeNull();
		expect(parseAsk("42")).toBeNull();
	});
});
