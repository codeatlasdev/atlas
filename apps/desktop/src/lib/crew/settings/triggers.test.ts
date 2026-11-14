// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { upsertTrigger, withTriggerEnabled, withoutTrigger } from "./triggers.ts";

const t = (id, enabled = true) => ({ id, kind: { type: "webhook", token: id }, prompt: id, enabled });

describe("withTriggerEnabled", () => {
	test("flips only the matching trigger and keeps the rest", () => {
		const out = withTriggerEnabled([t("a"), t("b")], "b", false);
		expect(out.map((x) => x.enabled)).toEqual([true, false]);
	});

	test("unknown id changes nothing", () => {
		expect(withTriggerEnabled([t("a")], "zzz", false)).toEqual([t("a")]);
	});
});

describe("upsertTrigger", () => {
	test("appends a new trigger", () => {
		expect(upsertTrigger([t("a")], t("b")).map((x) => x.id)).toEqual(["a", "b"]);
	});

	test("replaces in place, keeping the order", () => {
		const edited = { ...t("a"), prompt: "new" };
		const out = upsertTrigger([t("a"), t("b")], edited);
		expect(out.map((x) => x.id)).toEqual(["a", "b"]);
		expect(out[0].prompt).toBe("new");
	});
});

describe("withoutTrigger", () => {
	test("removes by id", () => {
		expect(withoutTrigger([t("a"), t("b")], "a").map((x) => x.id)).toEqual(["b"]);
	});
});
