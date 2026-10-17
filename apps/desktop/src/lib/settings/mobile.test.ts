// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { pairUrl, secondsLeft, toggleScope } from "./mobile.ts";

const p = {
	node_id: "abc",
	code: "012345",
	expires_in_s: 120,
	addrs: ["192.168.1.2:4433", "[fe80::1]:4433"],
	relay_url: null,
	hostname: "Luan's MacBook",
};

describe("pairUrl", () => {
	// The phone parses this with Uri.parse: every value must survive the round trip,
	// including a hostname with spaces/quotes and repeated addr params.
	test("round-trips through URL parsing", () => {
		const u = new URL(pairUrl(p));
		expect(u.protocol).toBe("atlas-mobile:");
		expect(u.searchParams.get("node")).toBe("abc");
		expect(u.searchParams.get("code")).toBe("012345");
		expect(u.searchParams.get("name")).toBe("Luan's MacBook");
		expect(u.searchParams.getAll("addr")).toEqual(p.addrs);
		expect(u.searchParams.has("relay")).toBe(false);
	});
});

test("secondsLeft counts down and stops at zero", () => {
	expect(secondsLeft(p, 1000, 1000)).toBe(120);
	expect(secondsLeft(p, 1000, 1000 + 119_500)).toBe(1);
	expect(secondsLeft(p, 1000, 1000 + 500_000)).toBe(0);
});

test("toggleScope keeps canonical order and no duplicates", () => {
	expect(toggleScope(["term_write", "read"], "act", true)).toEqual(["read", "act", "term_write"]);
	expect(toggleScope(["read", "act"], "act", true)).toEqual(["read", "act"]);
	expect(toggleScope(["read", "act"], "read", false)).toEqual(["act"]);
});
