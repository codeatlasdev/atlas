// ci-paths.sh decides which CI jobs a change skips. A source change classified as docs ships untested code;
// so the rule is: only known docs-only patterns may skip anything, and an unknown path runs everything.
import { expect, test } from "bun:test";
import { join } from "node:path";

const script = join(import.meta.dir, "ci-paths.sh");
function areas(files: string[]): Record<string, boolean> {
	const p = Bun.spawnSync([script], { stdin: new Blob([files.join("\n")]) });
	return Object.fromEntries(
		p.stdout.toString().trim().split("\n").map((l) => {
			const [k, v] = l.split("=");
			return [k, v === "true"];
		}),
	);
}
const none = { rust: false, front: false, mobile: false, site: false, relay: false, workflows: false };

test("README, CHANGELOG and AGENTS.md changes run nothing heavy", () => {
	expect(areas(["README.md", "CHANGELOG.md", "AGENTS.md"])).toEqual(none);
});

test("docs/ rebuilds only the site, which renders it", () => {
	expect(areas(["docs/contributing.md"])).toEqual({ ...none, site: true });
});

test("a crate runs Rust only; the desktop frontend also rebuilds the shell that embeds it", () => {
	expect(areas(["crates/atlas-host/src/lib.rs"])).toEqual({ ...none, rust: true });
	expect(areas(["apps/desktop/src/App.svelte"])).toEqual({ ...none, rust: true, front: true });
	expect(areas(["Cargo.lock"])).toEqual({ ...none, rust: true });
});

test("markdown that is source is not skipped: skills are embedded and checked", () => {
	expect(areas(["skills/atlas/yaml.md"])).toMatchObject({ rust: true, front: true });
});

test("unknown paths, workflows and an empty diff run everything", () => {
	const all = { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true };
	expect(areas(["package.json"])).toEqual(all);
	expect(areas(["scripts/release.sh"])).toEqual(all);
	expect(areas([])).toEqual(all);
	expect(areas([".github/workflows/ci.yml"])).toMatchObject({ rust: true, workflows: true });
});
