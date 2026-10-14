// Tests for the CI gate scripts: they decide whether a push to main skips its tests and whether a pull
// request title is accepted, so a wrong answer either ships untested code or blocks every PR.
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");

function run(cmd: string[], env: Record<string, string> = {}) {
	const p = Bun.spawnSync(cmd, { cwd: root, env: { ...process.env, ...env } });
	return { code: p.exitCode, out: p.stdout.toString().trim(), err: p.stderr.toString() };
}

describe("check-pr-title.sh", () => {
	const title = (t: string) => run(["scripts/check-pr-title.sh", t]).code;

	test.each([
		"feat: add the deploy engine",
		"fix(desktop): keep the sidebar width",
		"perf(tui): skip redraws",
		"ci(release.yml): pin actions",
		"feat!: drop the v1 config",
		"refactor(atlas-core)!: rename Channel",
		'Revert "feat: add the deploy engine"',
	])("accepts %p", (t) => expect(title(t)).toBe(0));

	test.each([
		"Add the deploy engine", // no type
		"feat:add", // no space after the colon
		"feat: ", // empty summary
		"Feat: add", // types are lowercase
		"feature: add", // unknown type
		"fix(Desktop): x", // scopes are lowercase
		"fix(): x", // empty scope
		"Merge pull request #3 from x/y", // a merge title
	])("rejects %p", (t) => expect(title(t)).toBe(1));
});

describe("ci-tested.sh", () => {
	// A fake `gh api` answering from a table keyed by the URL path; anything else fails like an API error.
	function fakeGh(answers: Record<string, string>) {
		const dir = mkdtempSync(join(tmpdir(), "ci-tested-"));
		const gh = join(dir, "gh");
		const cases = Object.entries(answers)
			.map(([path, out]) => `  ${JSON.stringify(path)}) printf '%s\\n' ${JSON.stringify(out)} ;;`)
			.join("\n");
		writeFileSync(gh, `#!/usr/bin/env bash\ncase "$2" in\n${cases}\n  *) exit 1 ;;\nesac\n`);
		chmodSync(gh, 0o755);
		return gh;
	}
	const R = "o/r";
	const base = {
		[`repos/${R}/git/commits/squash`]: "TREE",
		[`repos/${R}/commits/squash/pulls`]: "head",
		[`repos/${R}/git/commits/head`]: "TREE",
		[`repos/${R}/actions/workflows/ci.yml/runs?head_sha=head&event=pull_request&status=success`]: "1",
	};
	const tested = (answers: Record<string, string>) =>
		run(["scripts/ci-tested.sh", R, "squash"], { GH: fakeGh(answers) });

	test("same tree as a PR head with a green run: tested", () => {
		expect(tested(base)).toEqual({ code: 0, out: "true", err: "" });
	});

	test("tree differs (merged while behind main): full run", () => {
		expect(tested({ ...base, [`repos/${R}/git/commits/head`]: "OTHER" }).out).toBe("false");
	});

	test("no green CI run for the PR head: full run", () => {
		const k = `repos/${R}/actions/workflows/ci.yml/runs?head_sha=head&event=pull_request&status=success`;
		expect(tested({ ...base, [k]: "0" }).out).toBe("false");
	});

	test("no merged PR (a direct push): full run", () => {
		expect(tested({ ...base, [`repos/${R}/commits/squash/pulls`]: "" }).out).toBe("false");
	});

	test("an API error: full run, and the job does not fail", () => {
		const { [`repos/${R}/git/commits/head`]: _, ...broken } = base;
		expect(tested(broken)).toMatchObject({ code: 0, out: "false" });
	});
});
