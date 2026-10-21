// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import devicesFixture from "../__fixtures__/mobile-devices.json";
import pairStartFixture from "../__fixtures__/mobile-pair-start.json";
import { pairUrl, SCOPES, secondsLeft, toggleScope } from "./mobile.ts";

// The REAL `mobile_pair_start` reply (written by atlas-host `mobile::tests`), never a hand-made mock.
const p = pairStartFixture;
// The URL below is what the phone's `pair_url_test.dart` parses; UPDATE_FIXTURES=1 rewrites it.
const urlFixture = join(import.meta.dir, "../../../../mobile/test/fixtures/pair_url.txt");

describe("pairUrl", () => {
	test("carries every field of the real reply, a hostname with spaces and quotes, repeated addrs", () => {
		const url = pairUrl(p);
		const u = new URL(url);
		expect(u.protocol).toBe("atlas-mobile:");
		expect(u.searchParams.get("node")).toBe(p.node_id);
		expect(u.searchParams.get("code")).toBe(p.code);
		expect(u.searchParams.get("name")).toBe(p.hostname);
		expect(u.searchParams.getAll("addr")).toEqual(p.addrs);
		expect(u.searchParams.has("relay")).toBe(false);
		if (process.env.UPDATE_FIXTURES) writeFileSync(urlFixture, `${url}\n`);
		expect(readFileSync(urlFixture, "utf8").trim()).toBe(url);
	});
});

test("the real devices reply has the fields Settings reads", () => {
	const [d] = devicesFixture;
	expect(typeof d.node_id).toBe("string");
	expect(typeof d.name).toBe("string");
	expect(typeof d.paired_at).toBe("number");
	expect(typeof d.revoked).toBe("boolean");
	expect(d.scopes.every((s) => SCOPES.some((x) => x.id === s))).toBe(true);
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
