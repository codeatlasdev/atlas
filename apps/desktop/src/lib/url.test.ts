// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { toUrl } from "./url.ts";

test("address bar: urls, local hosts, domains, searches", () => {
	expect(toUrl("http://x.test/a")).toBe("http://x.test/a");
	expect(toUrl("localhost:5173/app")).toBe("http://localhost:5173/app");
	expect(toUrl("example.com")).toBe("https://example.com");
	expect(toUrl("how to rust")).toBe("https://duckduckgo.com/?q=how%20to%20rust");
});
