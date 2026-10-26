// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { VIEW_DROPPED, isViewDropped } from "./viewDropped.ts";

// Contract with the Rust side: the constant is read from the real source, not retyped.
test("VIEW_DROPPED is the exact string term_bounds returns in lifecycle.rs", () => {
	const rs = readFileSync(
		new URL("../../../../../../crates/atlas-desktop/src/term/lifecycle.rs", import.meta.url),
		"utf8",
	);
	expect(rs).toContain(`pub const VIEW_DROPPED: &str = "${VIEW_DROPPED}";`);
});

test("isViewDropped matches a Tauri rejection (string), an Error and nothing else", () => {
	expect(isViewDropped(VIEW_DROPPED)).toBe(true);
	expect(isViewDropped(new Error(VIEW_DROPPED))).toBe(true);
	expect(isViewDropped(`prefix: ${VIEW_DROPPED}`)).toBe(true);
	expect(isViewDropped("no terminal for tab t1")).toBe(false);
	expect(isViewDropped(undefined)).toBe(false);
	expect(isViewDropped(null)).toBe(false);
	expect(isViewDropped({})).toBe(false);
});
