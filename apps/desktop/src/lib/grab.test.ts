// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { grabPrompt } from "./grab.ts";

const g = {
	url: "http://localhost:4988/",
	title: "t",
	viewport: { width: 1, height: 1, dpr: 2 },
	target: {
		tagName: "button",
		selector: "#app > button.cta",
		htmlSnippet: "<button>Buy</button>",
		textSnippet: "Buy",
		attributes: {},
		computedStyles: { color: "red", margin: "auto", border: "none" },
		rectViewport: { x: 0, y: 0, width: 1, height: 1 },
		rectPage: { x: 0, y: 0, width: 1, height: 1 },
	},
	ancestorPath: [],
	nearbyText: [],
};

test("screenshot path is plain (no @ mention) and noise styles are dropped", () => {
	const p = grabPrompt(g, "/tmp/x.png");
	expect(p.endsWith("(read this image file): /tmp/x.png")).toBe(true);
	expect(p).not.toContain("@/tmp");
	expect(p).toContain("Computed: color: red");
	expect(p).not.toContain("margin");
});
