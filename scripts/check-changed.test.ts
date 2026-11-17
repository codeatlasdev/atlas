// The check:changed CLI end to end: the plan as JSON (what would run and why), the `affected:` plugin contract of
// atlas.yaml (changed files on stdin, `{tasks, tests, routes, builds}` on stdout, issue #128), a dry run that runs
// nothing, and a bad --base that fails instead of quietly widening or narrowing the change. The planner itself is
// tested in checks/plan.test.ts, the diff base in lib/base.test.ts.
import { describe, expect, test } from "bun:test";

const run = async (args: string[], files?: string[]) => {
	const p = Bun.spawn(["bun", `${import.meta.dir}/check-changed.ts`, ...args], {
		stdin: new Blob([(files ?? []).map((f) => `${f}\n`).join("")]),
		stdout: "pipe",
		stderr: "pipe",
		cwd: `${import.meta.dir}/..`,
	});
	const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
	return { out, err, code };
};

// These run the real script, which calls `cargo metadata` for a Rust-scoped change: on a fresh CI runner (no cache,
// first resolve of the workspace) that alone takes longer than bun's 5 s default (seen at 5000 ms, killed with 143).
const COLD_CARGO_MS = 60_000;

describe("--json --stdin (the atlas_affected plugin)", () => {
	test("the commands of the plan are the tests; no Rust change means no crate graph and no builds", async () => {
		const { out, code } = await run(["--json", "--stdin"], ["scripts/check-changed.ts", "docs/contributing.md"]);
		expect(code).toBe(0);
		const j = JSON.parse(out);
		expect(j.tests).toEqual(["bun run i18n:check", "bun run release:test", "bun run dup", "bun run site:check", "bun run site:build"]);
		expect(j.steps).toEqual(j.tests);
		expect(j.builds).toEqual([]);
		expect([j.tasks, j.routes]).toEqual([[], []]);
		expect(j.base).toBeNull();
	});

	test("a clean change set, or docs the checks do not read, prints an empty plan", async () => {
		const { out, code } = await run(["--json", "--stdin"], ["README.md"]);
		expect(code).toBe(0);
		expect(JSON.parse(out)).toMatchObject({ tests: [], builds: [], commands: [], everything: null, ignored: ["README.md"] });
	});

	test("says why: the glob and the file behind each command", async () => {
		const { out } = await run(["--json", "--stdin"], ["services/push-relay/main.go"]);
		const j = JSON.parse(out);
		expect(j.commands).toEqual([
			{ id: "relay", area: "relay", cmd: "bun scripts/sh.ts scripts/push-relay-check.sh", why: ["services/push-relay/** ← services/push-relay/main.go"] },
		]);
		expect(j.areas.relay).toEqual({ on: true, because: ["services/push-relay/** ← services/push-relay/main.go"] });
		expect(j.areas.rust.on).toBe(false);
	});

	test("an unknown path says it runs everything, and which path made it so", async () => {
		const { out } = await run(["--json", "--stdin"], ["brand-new/thing.bin"]);
		const j = JSON.parse(out);
		expect(j.everything).toContain("brand-new/thing.bin");
		expect(j.workspace).toBe(true);
		expect(j.commands.map((c: { id: string }) => c.id)).toContain("site-build");
		expect(j.commands.find((c: { id: string }) => c.id === "clippy").cmd).toContain("--workspace");
	});

	test("a crate under crates/ resolves through cargo metadata to its dependents", async () => {
		const { out, code } = await run(["--json", "--stdin", "--no-windows"], ["crates/atlas-core/src/lib.rs"]);
		expect(code).toBe(0);
		const j = JSON.parse(out);
		expect(j.crates).toContain("atlas-core");
		expect(j.crates).toContain("atlas-cli");
		expect(j.builds).toEqual(j.crates);
		expect(j.workspace).toBe(false);
		expect(j.tests).toContain(`cargo nextest run ${j.crates.map((c: string) => `-p ${c}`).join(" ")} --locked`);
	}, COLD_CARGO_MS);
});

describe("--dry-run", () => {
	test("lists the commands with their reasons and runs none of them (a failing command would fail the run)", async () => {
		const { out, code } = await run(["--stdin", "--dry-run", "--no-windows"], ["site/src/pages/index.astro"]);
		expect(code).toBe(0);
		expect(out).toContain("▶ site-build: bun run site:build");
		expect(out).toContain("site-build: site/** ← site/src/pages/index.astro");
		expect(out).not.toContain("passed");
	});

	test("nothing to check says so", async () => {
		const { out, code } = await run(["--stdin", "--dry-run"], ["AGENTS.md"]);
		expect(code).toBe(0);
		expect(out).toContain("nothing to check");
	});
});

describe("--base", () => {
	test("a ref that is not a commit sharing history with HEAD fails with its name, it does not fall back", async () => {
		const { err, code } = await run(["--base", "no-such-ref-xyz", "--dry-run"]);
		expect(code).toBe(2);
		expect(err).toContain("no-such-ref-xyz");
	});
});
