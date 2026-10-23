// check:changed decides which checks an edit skips. Skipping a crate that depends on the changed one ships an
// untested break; the full `bun run check` before a push is the safety net, this is about not lying in the loop.
import { describe, expect, test } from "bun:test";
import { type Graph, plan } from "./check-changed.ts";

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
		const p = plan(["apps/desktop/src/lib/bots/look.ts"], graph);
		expect(p.steps).toEqual(["bun run desktop:check"]);
		expect(p.crates).toEqual(["atlas-desktop"]);
		expect(plan(["skills/atlas/yaml.md"], graph).crates).toEqual(["atlas-cli", "atlas-skills"]);
	});

	test("docs only: nothing to run", () => {
		expect(plan(["docs/contributing.md", "CHANGELOG.md"], graph)).toEqual({ crates: [], workspace: false, steps: [] });
	});

	test("a path that merely looks like a crate is ignored", () => {
		expect(plan(["crates/not-a-crate/x.rs", "mycrates/atlas-core/x.rs"], graph).crates).toEqual([]);
	});
});
