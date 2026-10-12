// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { segments } from "./actionSegments.ts";

describe("segments", () => {
	test("plain text without links is one segment", () => {
		expect(segments("compiling…")).toEqual([{ text: "compiling…", css: "" }]);
	});

	test("ANSI colors become styled segments", () => {
		const s = segments("\x1b[31merror\x1b[0m done");
		expect(s.map((x) => x.text)).toEqual(["error", " done"]);
		expect(s[0].css).toContain("color");
		expect(s[1].css).toBe("");
	});

	test("path:line and URLs become links, split out of the surrounding text", () => {
		const s = segments("at src/app.ts:12:5 see https://example.com/x done");
		const links = s.filter((x) => x.link);
		expect(links.map((x) => [x.text, x.link.kind, x.link.target])).toEqual([
			["src/app.ts:12:5", "file", "src/app.ts"],
			["https://example.com/x", "url", "https://example.com/x"],
		]);
		expect(s.map((x) => x.text).join("")).toBe("at src/app.ts:12:5 see https://example.com/x done");
		expect(links[0].link).toMatchObject({ line: 12, col: 5 });
	});

	test("a link crossing an ANSI run boundary stays one link target", () => {
		const s = segments("\x1b[1msrc/a\x1b[0m.ts:7 x");
		const links = s.filter((x) => x.link);
		expect(links.map((x) => x.text).join("")).toBe("src/a.ts:7");
		expect(new Set(links.map((x) => x.link.target)).size).toBe(1);
		expect(s.map((x) => x.text).join("")).toBe("src/a.ts:7 x");
	});

	test("OSC 8 hyperlinks are kept as links", () => {
		const s = segments("\x1b]8;;https://example.com\x1b\\docs\x1b]8;;\x1b\\ ok");
		expect(s[0]).toMatchObject({ text: "docs", link: { kind: "osc", target: "https://example.com" } });
	});
});
