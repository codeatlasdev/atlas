// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { titleState } from "./title.ts";

test("omp title markers map to states; plain titles do not", () => {
	expect(titleState("π : fixing tests")).toBe("working");
	expect(titleState("repo | π ! approve bash")).toBe("blocked");
	expect(titleState("π > done")).toBe("idle");
	expect(titleState("⠋ thinking")).toBe("working");
	expect(titleState("~/P/c/ac")).toBeNull();
	expect(titleState("api:3000")).toBeNull();
});
