// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import fixture from "../__fixtures__/hygiene-report.json";
import { byDisk, bytes, fixSummary, fixedText, indexedDirs, projectTotal } from "./hygiene.ts";

// Contract: the fixture is the REAL serialization of `atlas_core::hygiene::Report` (Rust test
// `hygiene_report_fixture_is_the_real_serialization`); a renamed field there breaks these reads.
test("the panel reads every field of the real report", () => {
	const p = fixture.projects[0];
	expect(fixture.orphans[0].pid).toBe(4242);
	expect(projectTotal(p)).toBe(p.target + p.node_modules + p.logs + p.worktrees[0][1]);
	expect(indexedDirs(fixture)).toEqual(["/work/app/target"]);
	expect(fixture.dirs[0].cap_bytes).toBe(4096);
});

test("sizes use binary units and never show a fraction of a kilobyte", () => {
	expect(bytes(0)).toBe("0 B");
	expect(bytes(1023)).toBe("1023 B");
	expect(bytes(1024)).toBe("1 KB");
	expect(bytes(1536 * 1024)).toBe("1.5 MB");
	expect(bytes(3 * 1024 ** 3)).toBe("3.0 GB");
	expect(bytes(2048 * 1024 ** 4)).toBe("2048.0 TB");
});

test("the biggest project leads and the input is not reordered", () => {
	const small = { ...fixture.projects[0], name: "small", target: 1, node_modules: 0, logs: 0, worktrees: [] };
	const input = [small, fixture.projects[0]];
	expect(byDisk(input).map((p) => p.name)).toEqual(["app", "small"]);
	expect(input[0].name).toBe("small");
});

test("the fix summary names only what there is to do, and nothing when clean", () => {
	expect(fixSummary(fixture)).toBe("end 1 orphaned process, free 2 KB, keep Spotlight out of 1 folder");
	const clean = {
		...fixture,
		orphans: [],
		freeable: { dirs_removed: 0, entries_removed: 0, bytes: 0 },
		projects: [],
	};
	expect(fixSummary(clean)).toBeNull();
	const two = { ...clean, orphans: [fixture.orphans[0], fixture.orphans[0]] };
	expect(fixSummary(two)).toBe("end 2 orphaned processes");
});

test("a fix reports what was ended, freed and marked, and what would not die", () => {
	const f = {
		reaped: 1,
		survivors: [],
		freed: { dirs_removed: 0, entries_removed: 0, bytes: 1024 },
		marked: [],
	};
	expect(fixedText(f)).toBe("Ended 1 process, freed 1 KB");
	expect(fixedText({ ...f, reaped: 3, marked: ["a", "b"], survivors: [9] })).toBe(
		"Ended 3 processes, freed 1 KB, marked 2 folders never-index, 1 still running (stuck in the kernel)",
	);
});
