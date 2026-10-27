// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

// Incident class (term stubs, tray, dock): a command a page invokes but `main.rs` never registers fails only at
// runtime, in a window nobody has open. Every `shots_*` command any frontend file calls must be in the handler list,
// and every registered one must be called by something (no dead command left registered).
const root = join(import.meta.dir, "../..");
const rustMain = readFileSync(join(root, "../../../crates/atlas-desktop/src/main.rs"), "utf8");

function walk(dir, out = []) {
	for (const n of readdirSync(dir)) {
		const p = join(dir, n);
		if (statSync(p).isDirectory()) walk(p, out);
		else if (/\.(svelte|ts)$/.test(n) && !/\.test\.ts$/.test(n)) out.push(p);
	}
	return out;
}

const called = new Set();
for (const f of walk(root)) {
	for (const m of readFileSync(f, "utf8").matchAll(/invoke(?:<[^>]*>)?\(\s*"(shots_[a-z_]+)"/g))
		called.add(m[1]);
}
const registered = [...rustMain.matchAll(/shots::(?:[a-z_]+::)?(shots_[a-z_]+),/g)].map((m) => m[1]);

test("every shots command the frontend calls is registered in main.rs", () => {
	expect(called.size).toBeGreaterThan(20);
	for (const n of called) expect(registered, `${n} is called but not registered`).toContain(n);
});

// The debug build's bridge commands (`shots_debug_start`, `shots_perf`) are driven by `atlas desktop debug eval`, not by
// a page; they must stay out of release builds.
const [releaseList, debugList] = rustMain.split("#[cfg(debug_assertions)]\n    let builder");
const debugOnly = [...debugList.matchAll(/shots::(?:[a-z_]+::)?(shots_[a-z_]+),/g)].map((m) => m[1]);

test("the debug commands are registered only in the debug handler list", () => {
	expect(debugOnly).toContain("shots_debug_start");
	for (const n of debugOnly) expect(releaseList, n).not.toContain(n);
});

test("every registered shots command is called by some page", () => {
	for (const n of registered.filter((n) => !debugOnly.includes(n)))
		expect(called.has(n), `${n} is registered but nothing calls it`).toBe(true);
});
