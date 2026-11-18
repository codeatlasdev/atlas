// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { STATE_MESSAGES, stateLabel } from "./stateLabel.ts";

/** The variants of a Rust enum, in the form its `#[serde(rename_all = ...)]` serializes them. */
export function rustEnumValues(file, name) {
	const src = readFileSync(new URL(`../../../../../${file}`, import.meta.url), "utf8");
	const at = src.indexOf(`pub enum ${name} {`);
	expect(at).toBeGreaterThan(0);
	const attrs = src.slice(Math.max(0, at - 200), at);
	const body = src.slice(at, src.indexOf("\n}", at));
	const snake = /rename_all = "snake_case"/.test(attrs);
	const lower = /rename_all = "lowercase"/.test(attrs);
	expect(snake || lower).toBe(true);
	return [...body.matchAll(/^\s{4}([A-Z]\w*)\s*[,{(]/gm)].map(([, v]) =>
		snake ? v.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase() : v.toLowerCase(),
	);
}

describe("stateLabel", () => {
	test("every state the runner can send for a service or tunnel has a message", () => {
		const sent = rustEnumValues("crates/atlas-tui/src/runtime/service.rs", "ServiceState");
		expect(sent).toContain("suspended");
		for (const s of sent) expect(Object.hasOwn(STATE_MESSAGES, s), `no message for state ${s}`).toBe(true);
	});

	test("a state this build does not know shows as sent, and never throws", () => {
		expect(stateLabel("hibernating")).toBe("hibernating");
		expect(stateLabel("constructor")).toBe("constructor");
	});
});
