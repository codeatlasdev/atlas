// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { projectsMock } from "../../../../test/projectsMock.ts";

// The mock must export exactly what the real module exports, or a module importing a missing name fails to link in
// whichever test file runs after the mock. Read from the source: importing the real module would link the stores.
test("projectsMock exports every name the real sidebar projects module exports", () => {
	const src = readFileSync(new URL("./projects.ts", import.meta.url), "utf8");
	const real = [...src.matchAll(/^export (?:async )?(?:function|const|let) (\w+)/gm)].map((m) => m[1]);
	expect(real.length).toBeGreaterThan(0);
	expect(Object.keys(projectsMock()).sort()).toEqual(real.sort());
});
