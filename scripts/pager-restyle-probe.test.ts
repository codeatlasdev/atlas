// Incident (#121): flipping the roles of two pager layers restyled every node of both (57 ms per 23k-node layer in the
// app, 286 ms for a switch with a 400-line diff in each of five spaces) because `inert` and `pointer-events` inherit.
// This runs the probe in a real Chromium against the real `SpaceLayer.svelte`, and against the design before the fix:
// the old one must be rejected (so the probe can fail), the current one must pass. Skipped where no Chromium exists.
import { describe, expect, test } from "bun:test";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";

const script = join(import.meta.dir, "pager-restyle-probe.ts");
const cache = join(process.env.HOME ?? "", "Library/Caches/ms-playwright");
const chrome =
	process.env.CHROME ||
	(existsSync(cache) &&
		readdirSync(cache).some((d) => d.startsWith("chromium-")));

const before = join(import.meta.dir, "fixtures/space-layer-before-121.svelte");
const probe = (...args: string[]) => {
	const r = Bun.spawnSync(["bun", script, "--json", ...args], {
		stdout: "pipe",
		stderr: "pipe",
	});
	return {
		code: r.exitCode,
		out: r.stdout.toString(),
		err: r.stderr.toString(),
	};
};

describe.skipIf(!chrome)(
	"a role flip does not restyle the layer's contents",
	() => {
		test("the current SpaceLayer restyles a handful of elements, not the content", () => {
			const r = probe();
			expect(r.err).toBe("");
			expect(r.code).toBe(0);
			const report = JSON.parse(r.out.trim().split("\n").pop()!);
			expect(report.nodes).toBeGreaterThan(60_000); // three layers of ~23k nodes: the probe really mounted them
			expect(report.elementsRestyled).toBeLessThan(200);
		}, 60_000);

		test("the design before the fix (root `inert`, `pointer-events: none`, `contain: style`) is rejected", () => {
			const r = probe("--layer", before);
			expect(r.code).toBe(1);
			const report = JSON.parse(r.out.trim().split("\n").pop()!);
			expect(report.elementsRestyled).toBeGreaterThan(5_000);
			expect(r.err).toMatch(/elements restyled/);
		}, 60_000);
	},
);
