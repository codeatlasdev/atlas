// The planner decides which checks an edit skips, locally and in CI. Skipping a crate that depends on the changed one
// ships an untested break; classifying a source change as docs ships untested code. So: only known docs-only paths
// skip anything, and an unknown path runs everything. (Moved here from check-changed.test.ts and ci-paths.test.ts.)
import { describe, expect, test } from "bun:test";
import { AREAS } from "./map.ts";
import { type Graph, classify, ciOnly, dependents, plan } from "./plan.ts";

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
const on = (files: string[] | undefined, mode: "local" | "ci" = "ci") =>
	Object.entries(classify(files, mode).areas)
		.filter(([, a]) => a.on)
		.map(([k]) => k);
const ids = (files: string[], opts = {}) => plan(files, graph, opts).commands.map((c) => c.id);
const cmds = (files: string[], opts = {}) => plan(files, graph, opts).commands.map((c) => c.cmd);
const ALL = Object.keys(AREAS);

describe("areas (what CI's plan job outputs)", () => {
	test("CONTRIBUTING, licences and issue forms run nothing; the README is prose the writing lint reads", () => {
		expect(on(["CONTRIBUTING.md", "LICENSE-MIT", ".github/ISSUE_TEMPLATE/bug.yml"])).toEqual([]);
		expect(on(["README.md"])).toEqual(["i18n"]);
	});

	// scripts/agents-md.test.ts caps AGENTS.md at 8 KB and checks the skills and paths it names; it runs with the
	// scripts tests, so a change to AGENTS.md, the repo skills or the .claude/skills link must run that area.
	test("AGENTS.md and the repo skills run the scripts tests, and nothing else", () => {
		expect(on(["AGENTS.md"])).toEqual(["scripts"]);
		expect(on([".agents/skills/desktop-debug/SKILL.md"])).toEqual(["scripts"]);
		expect(on([".claude/skills"])).toEqual(["scripts"]);
	});

	test("the changelog is read by the scripts tests (changelog-lint, release-notes)", () => {
		expect(on(["CHANGELOG.md"])).toEqual(["i18n", "scripts"]);
	});

	test("docs/ rebuilds the site, which renders it, and is read by the i18n check (writing lint, translations)", () => {
		expect(on(["docs/contributing.md"])).toEqual(["i18n", "site"]);
	});

	test("a crate runs Rust (and duplication for its sources); the desktop frontend also rebuilds the shell that embeds it", () => {
		expect(on(["crates/atlas-host/src/lib.rs"])).toEqual(["rust", "i18n", "dup"]);
		expect(on(["apps/desktop/src/App.svelte"])).toEqual(["rust", "desktop", "i18n", "dup"]);
		expect(on(["Cargo.lock"])).toEqual(["rust"]);
	});

	test("markdown that is source is not skipped: skills are embedded and checked", () => {
		expect(on(["skills/atlas/yaml.md"])).toEqual(["rust", "i18n", "skills"]);
	});

	test("the Windows installer runs its own job, and only the changes that can affect it do", () => {
		expect(on(["site/public/install.ps1"])).toEqual(["scripts", "site", "installer"]);
		expect(on(["scripts/install-ps1.test.ts"])).toEqual(["scripts", "installer", "dup"]);
		expect(on(["site/public/install.sh"])).toEqual(["scripts", "site"]);
	});

	test("jscpd runs for the formats it reads, and for its own config", () => {
		expect(on(["site/src/pages/index.astro"])).toEqual(["i18n", "site", "dup"]);
		expect(on([".jscpd.json"])).toEqual(["dup"]);
		expect(on(["services/push-relay/main.go"])).toEqual(["relay"]);
	});

	test("the push relay and the mobile app run their own jobs only", () => {
		expect(on(["services/push-relay/go.mod"])).toEqual(["relay"]);
		expect(on(["apps/mobile/lib/main.dart"])).toEqual(["mobile", "dup"]);
	});

	// Incident class: the agent list lives in crates/ but the desktop imports it and Atlas Mobile's Dart copy is
	// generated from it (checked by the scripts tests); a Rust-only classification lets a stale copy through.
	test("the shared agent list runs the frontend, mobile and scripts checks, not Rust alone", () => {
		expect(on(["crates/atlas-core/data/agents.json"])).toEqual(["rust", "desktop", "scripts", "mobile"]);
		expect(ids(["crates/atlas-core/data/agents.json"])).toEqual(
			expect.arrayContaining(["desktop", "mobile", "scripts"]),
		);
		expect(on(["crates/atlas-core/data/channels.json"])).toEqual(["rust"]);
	});

	test("a changed workflow or action reruns every job in CI, and only its own lint locally", () => {
		expect(on([".github/workflows/ci.yml"], "ci")).toEqual(ALL);
		expect(on([".github/actions/rust-gate/action.yml"], "ci")).toEqual(ALL);
		expect(on([".github/workflows/ci.yml"], "local")).toEqual(["scripts", "workflows"]);
	});

	test("unknown paths and an unknown diff run everything", () => {
		for (const f of ["package.json", "atlas.yaml", "bun.lock", "demo/x.mp4", "new-top-level-dir/x.rs"]) {
			expect(on([f]), f).toEqual(ALL);
			expect(classify([f]).everything).toContain(f);
		}
		expect(on(undefined)).toEqual(ALL);
		expect(on(["README.md", "package.json"])).toEqual(ALL);
	});

	test("an empty list of changed files is a clean tree, not an unknown diff", () => {
		expect(on([])).toEqual([]);
		expect(plan([], graph).commands).toEqual([]);
	});

	test("every area has a way to turn it on: a probe path per area", () => {
		const probe: Record<string, string> = {
			rust: "crates/atlas-core/src/lib.rs",
			desktop: "apps/desktop/src/x.ts",
			i18n: "i18n/glossary.yaml",
			skills: "skills/atlas/SKILL.md",
			scripts: "scripts/release.sh",
			mobile: "apps/mobile/pubspec.yaml",
			relay: "services/push-relay/main.go",
			site: "site/src/pages/index.astro",
			installer: "site/public/install.ps1",
			workflows: ".github/zizmor.yml",
			dup: "crates/atlas-core/src/lib.rs",
		};
		expect(Object.keys(probe).sort()).toEqual(ALL.sort());
		for (const [area, f] of Object.entries(probe)) expect(classify([f]).areas[area]!.on, area).toBe(true);
	});

	test("the reason names the glob and the file", () => {
		expect(classify(["apps/desktop/src/a.ts", "apps/desktop/src/b.ts"]).areas.desktop!.because).toEqual([
			"apps/desktop/** ← apps/desktop/src/a.ts (+1 more)",
		]);
	});
});

describe("Rust scope", () => {
	test("a crate change checks it and every crate that depends on it, transitively", () => {
		expect(plan(["crates/atlas-lsp/src/manager.rs"], graph).crates).toEqual(["atlas-cli", "atlas-desktop", "atlas-host", "atlas-lsp"]);
		expect(plan(["crates/atlas-term/src/lib.rs"], graph).crates).toEqual(["atlas-desktop", "atlas-term"]);
		// atlas-term is the one crate that does not use atlas-core.
		expect(plan(["crates/atlas-core/src/sys.rs"], graph).crates).not.toContain("atlas-term");
		expect(plan(["crates/atlas-core/src/sys.rs"], graph).crates).toHaveLength(7);
		expect(dependents(graph, new Set(["atlas-skills"]))).toEqual(new Set(["atlas-skills", "atlas-cli"]));
	});

	test("workspace files mean the whole workspace, not a crate list", () => {
		for (const f of ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "deny.toml", ".cargo/config.toml"]) {
			expect(plan([f, "crates/atlas-term/src/lib.rs"], graph), f).toMatchObject({ workspace: true, crates: [] });
		}
	});

	test("the frontend also rebuilds the desktop shell; skills also the crate that embeds them", () => {
		const p = plan(["apps/desktop/src/lib/crew/board.ts"], graph);
		expect(p.crates).toEqual(["atlas-desktop"]);
		expect(p.commands.map((c) => c.cmd)).toContain("bun run desktop:check");
		expect(plan(["skills/atlas/yaml.md"], graph).crates).toEqual(["atlas-cli", "atlas-skills"]);
	});

	test("a path under crates/ that is not a crate of the graph (a removed crate) checks the workspace", () => {
		expect(plan(["crates/gone/x.rs"], graph)).toMatchObject({ workspace: true, crates: [] });
		expect(plan(["mycrates/atlas-core/x.rs"], graph).workspace).toBe(true); // unknown path: everything
	});

	test("no Rust change: no cargo command, and no crate graph needed", () => {
		expect(cmds(["apps/mobile/lib/main.dart"]).some((c) => c.startsWith("cargo"))).toBe(false);
		expect(plan(["docs/x.md"], graph)).toMatchObject({ workspace: false, crates: [] });
	});
});

describe("commands", () => {
	test("a crate change: fmt, clippy, nextest and doctests scoped to the crates; skills and the rest only if touched", () => {
		const c = cmds(["crates/atlas-lsp/src/manager.rs"]);
		const pk = "-p atlas-cli -p atlas-desktop -p atlas-host -p atlas-lsp";
		expect(c).toEqual([
			"cargo fmt --all -- --check",
			`cargo clippy ${pk} --all-targets --locked -- -D warnings`,
			`cargo nextest run ${pk} --locked`,
			`cargo test ${pk} --locked --doc`,
			"bun run dup",
		]);
	});

	test("cargo deny runs when a manifest, the lockfile or deny.toml changed, not for a source edit", () => {
		expect(ids(["crates/atlas-core/src/lib.rs"])).not.toContain("deny");
		expect(ids(["crates/atlas-core/Cargo.toml"])).toContain("deny");
		expect(ids(["Cargo.lock"])).toContain("deny");
		expect(ids(["deny.toml"])).toContain("deny");
	});

	test("docs only: the i18n check and the site build and type-check, nothing else", () => {
		expect(cmds(["docs/contributing.md"])).toEqual(["bun run i18n:check", "bun run site:check", "bun run site:build"]);
		expect(cmds(["CONTRIBUTING.md"])).toEqual([]);
	});

	test("each non-Rust area contributes its own check", () => {
		expect(cmds(["apps/mobile/pubspec.yaml"])).toEqual(["bun scripts/sh.ts scripts/mobile-check.sh"]);
		expect(cmds(["services/push-relay/main.go"])).toEqual(["bun scripts/sh.ts scripts/push-relay-check.sh"]);
		expect(cmds([".github/zizmor.yml"])).toEqual(["bun scripts/sh.ts scripts/lint-workflows.sh"]);
		expect(cmds(["scripts/ci-plan.ts"])).toEqual(["bun run release:test", "bun run dup"]);
	});

	test("unknown paths run every check, the whole workspace, in the order `bun run check` uses", () => {
		const p = plan(undefined, graph);
		expect(p.commands.map((c) => c.id)).toEqual([
			"fmt", "clippy", "nextest", "doctests", "deny", "skills", "workflows", "desktop", "i18n", "mobile", "relay", "scripts", "dup", "site-check", "site-build",
		]);
		expect(p.commands[1]!.cmd).toBe("cargo clippy --workspace --all-targets --locked -- -D warnings");
		expect(p.commands.every((c) => c.why[0]!.includes("no changed-file list"))).toBe(true);
	});

	test("the checks only CI can run are listed with the reason, per touched area", () => {
		expect(ciOnly().map((k) => k.id)).toEqual(["desktop-opens", "desktop-startup-baseline", "desktop-idle-gate", "root-tests", "arm64-build", "installer"]);
		expect(ciOnly(plan(["services/push-relay/main.go"], graph))).toEqual([]);
		expect(ciOnly(plan(["crates/atlas-core/src/lib.rs"], graph)).map((k) => k.id)).toEqual(["desktop-opens", "desktop-startup-baseline", "desktop-idle-gate", "root-tests", "arm64-build"]);
		expect(ciOnly().every((k) => k.why.length > 20)).toBe(true);
	});
});

// Incident: `bun run check:changed` on a change confined to atlas-desktop (a bin-only crate) failed at `doctests`:
// `cargo test -p atlas-desktop --doc` says "no library targets found". Only packages with a library target have
// doctests. (A change that also widened the scope to `--workspace` hid it, which is why it was found late.)
describe("doctests only for packages with a library", () => {
	const libs = ["atlas-core", "atlas-lsp", "atlas-tui", "atlas-skills", "atlas-host", "atlas-term"]; // atlas-cli and atlas-desktop are bin-only
	const doc = (files: string[], opts = {}) => plan(files, graph, { libs, ...opts }).commands.find((c) => c.id === "doctests")?.cmd;

	test("a change confined to a bin-only crate has no doctests step, and the other steps keep their scope", () => {
		const p = plan(["crates/atlas-desktop/src/workspace.rs"], graph, { libs });
		expect(p.commands.map((c) => c.id)).toEqual(["fmt", "clippy", "nextest", "i18n", "dup"]);
		expect(p.commands.find((c) => c.id === "nextest")!.cmd).toBe("cargo nextest run -p atlas-desktop --locked");
	});

	test("a scoped change runs --doc for the library packages among the crates and their dependents only", () => {
		expect(doc(["crates/atlas-lsp/src/manager.rs"])).toBe("cargo test -p atlas-host -p atlas-lsp --locked --doc");
		expect(doc(["crates/atlas-core/src/fs.rs"])).toBe("cargo test -p atlas-core -p atlas-host -p atlas-lsp -p atlas-skills -p atlas-tui --locked --doc");
	});

	test("the whole workspace (the gate, an unknown path) keeps `--workspace`: cargo skips bin-only members itself", () => {
		expect(plan(undefined, graph, { libs }).commands.find((c) => c.id === "doctests")!.cmd).toBe("cargo test --workspace --locked --doc");
		expect(doc(["Cargo.lock"])).toBe("cargo test --workspace --locked --doc");
	});

	test("without library information every crate is assumed to have one (the planner stays pure)", () => {
		expect(plan(["crates/atlas-desktop/src/x.rs"], graph).commands.find((c) => c.id === "doctests")!.cmd).toBe("cargo test -p atlas-desktop --locked --doc");
	});
});

// A `cfg(windows)` break (a function only Unix calls became dead code there, #326) passed every Linux check and
// failed only in CI's Windows job. With cargo-xwin, the loop lints the Windows target too.
describe("windows lane", () => {
	test("lints the same crates for the Windows target, in its own target dir, only when asked", () => {
		const files = ["crates/atlas-cli/src/main.rs"];
		expect(cmds(files).some((c) => c.includes("xwin"))).toBe(false);
		const win = cmds(files, { windows: true }).find((c) => c.includes("cargo xwin clippy"))!;
		expect(win).toContain("--target x86_64-pc-windows-msvc");
		expect(win).toContain("-p atlas-cli");
		expect(win).toContain("-D warnings");
		expect(win).toStartWith("CARGO_TARGET_DIR=target/xwin ");
	});

	test("nothing Rust changed: no Windows lint either", () => {
		expect(cmds(["docs/cli.md"], { windows: true }).some((c) => c.includes("xwin"))).toBe(false);
	});
});
