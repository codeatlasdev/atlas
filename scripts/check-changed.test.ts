// check:changed decides which checks an edit skips. Skipping a crate that depends on the changed one ships an
// untested break; the full `bun run check` before a push is the safety net, this is about not lying in the loop.
import { describe, expect, test } from "bun:test";
import { type Graph, commands, impact, plan } from "./check-changed.ts";

const graph: Graph = {
	"atlas-core": [],
	"atlas-term": [],
	"atlas-lsp": ["atlas-core"],
	"atlas-tui": ["atlas-core"],
	"atlas-skills": ["atlas-core"],
	"atlas-host": ["atlas-core", "atlas-lsp", "atlas-tui"],
	"atlas-cli": ["atlas-core", "atlas-host", "atlas-skills", "atlas-tui"],
	"atlas-desktop": ["atlas-core", "atlas-host", "atlas-tui", "atlas-term"],
};

describe("plan", () => {
	test("a crate change checks it and every crate that depends on it, transitively", () => {
		expect(plan(["crates/atlas-lsp/src/manager.rs"], graph).crates).toEqual([
			"atlas-cli",
			"atlas-desktop",
			"atlas-host",
			"atlas-lsp",
		]);
		expect(plan(["crates/atlas-term/src/lib.rs"], graph).crates).toEqual(["atlas-desktop", "atlas-term"]);
		// atlas-term is the one crate that does not use atlas-core.
		expect(plan(["crates/atlas-core/src/sys.rs"], graph).crates).not.toContain("atlas-term");
		expect(plan(["crates/atlas-core/src/sys.rs"], graph).crates).toHaveLength(7);
	});

	test("workspace files mean the whole workspace, not a crate list", () => {
		for (const f of ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml"]) {
			expect(plan([f, "crates/atlas-term/src/lib.rs"], graph)).toMatchObject({ workspace: true, crates: [] });
		}
	});

	test("the frontend also rebuilds the desktop shell; skills also the crate that embeds them", () => {
		const p = plan(["apps/desktop/src/lib/crew/board.ts"], graph);
		expect(p.steps).toEqual(["bun run desktop:check"]);
		expect(p.crates).toEqual(["atlas-desktop"]);
		expect(plan(["skills/atlas/yaml.md"], graph).crates).toEqual(["atlas-cli", "atlas-skills"]);
	});

	test("docs only: nothing to run", () => {
		expect(plan(["docs/contributing.md", "CHANGELOG.md"], graph)).toEqual({ crates: [], workspace: false, steps: [] });
	});

	test("AGENTS.md, the repo skills and the .claude link run the agents-md test", () => {
		for (const f of ["AGENTS.md", ".agents/skills/real-tests/SKILL.md", ".claude/skills"]) {
			expect(plan([f], graph)).toEqual({ crates: [], workspace: false, steps: ["bun test scripts/agents-md.test.ts"] });
		}
	});

	test("a path that merely looks like a crate is ignored", () => {
		expect(plan(["crates/not-a-crate/x.rs", "mycrates/atlas-core/x.rs"], graph).crates).toEqual([]);
	});
});

// A `cfg(windows)` break (a function only Unix calls became dead code there, #326) passed every Linux check and
// failed only in CI's Windows job. With cargo-xwin, the loop lints the Windows target too.
describe("windows lane", () => {
	test("lints the same crates for the Windows target, in its own target dir, only when asked", () => {
		const p = plan(["crates/atlas-cli/src/main.rs"], graph);
		expect(commands(p).some((c) => c.includes("xwin"))).toBe(false);
		const win = commands(p, { windows: true }).find((c) => c.includes("cargo xwin clippy"));
		expect(win).toContain("--target x86_64-pc-windows-msvc");
		expect(win).toContain("-p atlas-cli");
		expect(win).toContain("-D warnings");
		expect(win).toStartWith("CARGO_TARGET_DIR=target/xwin ");
	});
	test("nothing Rust changed: no Windows lint either", () => {
		expect(commands(plan(["docs/cli.md"], graph), { windows: true })).toEqual([]);
	});
});

// Issue #128: `--json` makes check-changed the `affected:` plugin of atlas.yaml (`atlas_affected`): changed
// files on stdin, `{tasks, tests, routes, builds}` on stdout, plus the crates and steps it derived them from.
describe("--json (the atlas_affected plugin)", () => {
	const run = async (files: string[], args: string[] = []) => {
		const p = Bun.spawn(["bun", `${import.meta.dir}/check-changed.ts`, "--json", ...args], {
			stdin: new Blob([files.map((f) => `${f}\n`).join("")]),
			stdout: "pipe",
			stderr: "pipe",
			cwd: `${import.meta.dir}/..`,
		});
		const [out, code] = await Promise.all([new Response(p.stdout).text(), p.exited]);
		return { out, code };
	};

	test("commands of the plan are the tests, the crates are the builds", () => {
		const p = plan(["crates/atlas-lsp/src/manager.rs", "apps/desktop/src/lib/x.ts"], graph);
		const j = impact(p);
		expect(j.crates).toEqual(p.crates);
		expect(j.builds).toEqual(p.crates);
		expect(j.tests).toEqual([
			"cargo fmt --all -- --check",
			`cargo clippy ${p.crates.map((c) => `-p ${c}`).join(" ")} --all-targets --locked -- -D warnings`,
			`cargo nextest run ${p.crates.map((c) => `-p ${c}`).join(" ")} --locked`,
			"bun run desktop:check",
		]);
		expect(j.tasks).toEqual([]);
		expect(j.routes).toEqual([]);
	});

	test("workspace files mean --workspace; docs only means nothing to run", () => {
		expect(impact(plan(["Cargo.lock"], graph)).tests).toEqual([
			"cargo fmt --all -- --check",
			"cargo clippy --workspace --all-targets --locked -- -D warnings",
			"cargo nextest run --workspace --locked",
		]);
		expect(impact(plan(["docs/x.md"], graph))).toEqual({
			tasks: [],
			tests: [],
			routes: [],
			builds: [],
			crates: [],
			workspace: false,
			steps: [],
		});
	});

	// Both run the real script, which calls `cargo metadata`: on a fresh CI runner (no cache, first resolve of the
	// workspace) that alone takes longer than bun's 5 s default (seen at 5000 ms, killed with 143).
	const COLD_CARGO_MS = 60_000;

	test("the real script reads the files from stdin and prints only JSON", async () => {
		const { out, code } = await run(["scripts/check-changed.ts", "docs/contributing.md"]);
		expect(code).toBe(0);
		const j = JSON.parse(out);
		expect(j.tests).toEqual(["bun test scripts"]);
		expect(j.steps).toEqual(["bun test scripts"]);
		expect(j.builds).toEqual([]);
	}, COLD_CARGO_MS);

	test("a crate under crates/ resolves through cargo metadata to its dependents", async () => {
		const { out, code } = await run(["crates/atlas-core/src/lib.rs"]);
		expect(code).toBe(0);
		const j = JSON.parse(out);
		expect(j.crates).toContain("atlas-core");
		expect(j.crates).toContain("atlas-cli");
		expect(j.builds).toEqual(j.crates);
	}, COLD_CARGO_MS);
});
