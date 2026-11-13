import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { prefsMock } from "../../../test/prefsMock.ts";

// The mock must export exactly what the real store exports, or a module importing a missing
// name fails to link in whichever test file runs after the mock (CI: "Export named 'setPref'
// not found"). Read from the source: importing the real store would link `api.ts`.
test("prefsMock exports every name the real prefs store exports", () => {
	const src = readFileSync(new URL("./prefs.svelte.ts", import.meta.url), "utf8");
	const real = [...src.matchAll(/^export (?:async )?(?:function|const|let) (\w+)/gm)].map((m) => m[1]);
	expect(real.length).toBeGreaterThan(0);
	expect(Object.keys(prefsMock({})).sort()).toEqual(real.sort());
});
