// The ratchets in diff mode: a change is judged on the offenders it adds, against the merge-base, so a baseline that
// lags main never fails anyone and never needs a re-snapshot. Real temp git repositories, real `git`.
import { afterAll, describe, expect, test } from "bun:test";
import { fix } from "./fix.ts";
import { type Finding, loadCtx } from "./types.ts";
import { check, cleanup, makeRepo, put, read, rulesOf } from "./testkit.ts";

// The resolver (scripts/lib/base.ts, tested in scripts/lib/base.test.ts) reads the environment: run these with a clean
// one, whatever CI or a developer's shell exports.
const AMBIENT = ["ATLAS_BASE", "GITHUB_ACTIONS", "GITHUB_EVENT_NAME", "GITHUB_EVENT_PATH"];
const saved = Object.fromEntries(AMBIENT.map((k) => [k, process.env[k]]));
for (const k of AMBIENT) delete process.env[k];

afterAll(() => {
	for (const k of AMBIENT) {
		if (saved[k] === undefined) delete process.env[k];
		else process.env[k] = saved[k];
	}
	cleanup();
});

/** Run `fn` with extra environment variables, restoring them after. */
async function withEnv<T>(vars: Record<string, string>, fn: () => Promise<T>): Promise<T> {
	for (const [k, v] of Object.entries(vars)) process.env[k] = v;
	try {
		return await fn();
	} finally {
		for (const k of Object.keys(vars)) delete process.env[k];
	}
}

const OLD = "apps/desktop/src/Old.svelte";
const oldScreen = (n: number) => `${Array.from({ length: n }, (_, i) => `<p>Old text ${i}</p>`).join("\n")}\n`;

function git(root: string, ...args: string[]) {
	const r = Bun.spawnSync(["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
	if (r.exitCode !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr.toString()}`);
	return r.stdout.toString().trim();
}

/** A valid repo with `files` committed on `main` (the base) and a `feature` branch checked out. */
async function baseRepo(files: Record<string, string> = {}, branch = "main") {
	const root = await makeRepo(files);
	// makeRepo snapshots a baseline from what it just wrote; the base commit holds exactly the baseline the test gives (or none).
	put(root, "i18n/hardcoded-baseline.json", files["i18n/hardcoded-baseline.json"] ?? "{}\n");
	git(root, "init", "-q", "-b", branch);
	git(root, "add", "-A");
	git(root, "commit", "-q", "-m", "base");
	git(root, "checkout", "-q", "-b", "feature");
	return root;
}

const offenders = (fs: Finding[], rule = "hardcoded") => fs.filter((f) => f.rule === rule);

describe("Svelte ratchet in diff mode", () => {
	test("an unchanged file with 100 old offenders is not looked at, whatever the baseline says", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(100) });
		put(root, "docs/touched.md", "# Touched\n");
		expect(offenders(await check(root))).toEqual([]);
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ svelte: { [OLD]: 1 } }));
		expect(offenders(await check(root))).toEqual([]);
		expect(offenders(await check(root), "hardcoded-baseline")).toEqual([]);
	});

	test("the same repo without a base ref falls back to the committed baseline", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3) }, "trunk");
		expect(offenders(await check(root))).toHaveLength(3);
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ svelte: { [OLD]: 3 } }));
		expect(offenders(await check(root))).toEqual([]);
		put(root, OLD, oldScreen(2));
		expect(offenders(await check(root), "hardcoded-baseline")).toHaveLength(1);
	});

	test("a changed file keeps its old offenders and fails only on the new one, at its line", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3) });
		put(root, OLD, `${oldScreen(3)}{m.app_save_button()}\n`);
		expect(offenders(await check(root))).toEqual([]);
		put(root, OLD, `${oldScreen(3)}<p>Brand new text</p>\n`);
		const fs = offenders(await check(root));
		expect(fs).toHaveLength(1);
		expect(fs[0]).toMatchObject({ severity: "error", file: OLD, line: 4 });
		expect(fs[0].message).toContain("Brand new text");
		expect(fs[0].message).toContain("is new in this change");
		expect(fs[0].message).toContain("3 at main");
	});

	test("moving, reordering or re-wrapping old offenders adds nothing", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3) });
		put(root, OLD, "<section>\n<p>Old text 2</p>\n<p>Old text 0</p>\n</section>\n<p>Old text 1</p>\n");
		expect(offenders(await check(root))).toEqual([]);
	});

	test("one more copy of a string already there is new, and only one is reported", async () => {
		const root = await baseRepo({ [OLD]: "<p>Same</p>\n" });
		put(root, OLD, "<p>Same</p>\n<p>Same</p>\n");
		const fs = offenders(await check(root));
		expect(fs).toHaveLength(1);
		expect(fs[0].line).toBe(2);
	});

	test("fewer offenders than at the base is fine", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3) });
		put(root, OLD, oldScreen(1));
		expect(offenders(await check(root))).toEqual([]);
	});

	test("a new file with offenders fails, untracked or staged", async () => {
		const root = await baseRepo();
		put(root, "apps/desktop/src/New.svelte", "<p>One</p>\n<p>Two</p>\n");
		expect(offenders(await check(root))).toHaveLength(2);
		git(root, "add", "-A");
		expect(offenders(await check(root))).toHaveLength(2);
		git(root, "commit", "-q", "-m", "feature");
		expect(offenders(await check(root))).toHaveLength(2);
	});

	test("a renamed file keeps its offenders; a rename plus one new offender fails on that one", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3) });
		git(root, "mv", OLD, "apps/desktop/src/Renamed.svelte");
		expect(offenders(await check(root))).toEqual([]);
		put(root, "apps/desktop/src/Renamed.svelte", `${oldScreen(3)}<p>Extra</p>\n`);
		const fs = offenders(await check(root));
		expect(fs.map((f) => f.message.includes("Extra"))).toEqual([true]);
	});

	test("main adding screens after the branch point needs no re-snapshot once the branch merges main", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(1) });
		git(root, "checkout", "-q", "main");
		put(root, "apps/desktop/src/FromMain.svelte", oldScreen(40));
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "main adds a screen");
		git(root, "checkout", "-q", "feature");
		git(root, "merge", "-q", "--no-edit", "main");
		put(root, "docs/x.md", "# X\n");
		expect(offenders(await check(root))).toEqual([]);
	});

	test("the base can be given: ATLAS_BASE judges from that commit", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(1) });
		put(root, "apps/desktop/src/Second.svelte", "<p>Second</p>\n");
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "second");
		expect(offenders(await check(root))).toHaveLength(1);
		await withEnv({ ATLAS_BASE: "HEAD" }, async () => expect(offenders(await check(root))).toEqual([]));
	});
});

describe("the baseline file may not grow", () => {
	const FILE = "i18n/hardcoded-baseline.json";
	const seeded = (n: number) => JSON.stringify({ svelte: { [OLD]: n } });

	test("raising a count is an error that says not to snapshot; lowering or keeping it is fine", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3), [FILE]: seeded(3) });
		expect(rulesOf(await check(root))).not.toContain("baseline-grew");
		put(root, FILE, seeded(5));
		const f = (await check(root)).find((x) => x.rule === "baseline-grew");
		expect(f).toMatchObject({ severity: "error", file: FILE, key: OLD });
		expect(f?.message).toContain("from 3 to 5");
		expect(f?.message).toContain("do not snapshot");
		put(root, FILE, seeded(2));
		expect(rulesOf(await check(root))).not.toContain("baseline-grew");
	});

	test("a new entry counts as growth; a baseline or a section that did not exist at the base is its introduction", async () => {
		const root = await baseRepo({ [FILE]: JSON.stringify({ svelte: { [OLD]: 1 } }) });
		put(root, FILE, JSON.stringify({ svelte: { [OLD]: 1, "apps/desktop/src/B.svelte": 2 } }));
		expect((await check(root)).filter((x) => x.rule === "baseline-grew").map((x) => x.key)).toEqual(["apps/desktop/src/B.svelte"]);
		put(root, FILE, JSON.stringify({ svelte: { [OLD]: 1 }, rust: { "crates/atlas-cli/src/main.rs": 9 } }));
		expect(rulesOf(await check(root))).not.toContain("baseline-grew");
		const fresh = await baseRepo();
		put(fresh, FILE, JSON.stringify({ svelte: { [OLD]: 7 } }));
		expect(rulesOf(await check(fresh))).not.toContain("baseline-grew");
	});

	test("fix shrinks the baseline in a diff-mode repo and never raises it", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(3), [FILE]: seeded(3) });
		put(root, OLD, oldScreen(1));
		fix(loadCtx(root));
		expect(JSON.parse(read(root, FILE)).svelte).toEqual({ [OLD]: 1 });
		put(root, OLD, oldScreen(4));
		fix(loadCtx(root));
		expect(JSON.parse(read(root, FILE)).svelte).toEqual({ [OLD]: 1 });
	});
});

describe("the other ratchets share the mechanism", () => {
	const RS = "crates/atlas-cli/src/main.rs";
	const TS = "apps/desktop/src/lib/hints.ts";
	const NAV = "apps/desktop/src/lib/nav.ts";

	test.each([
		["rust", RS, 'fn main() {\n    println!("old one");\n}\n', 'fn main() {\n    println!("old one");\n    println!("new one");\n}\n', "hardcoded", "new one"],
		["shortcut", TS, 'export const a = "Cmd+K";\n', 'export const a = "Cmd+K";\nexport const b = "Cmd+J";\n', "shortcut", "Cmd+J"],
		["platform-check", NAV, "const p = navigator.platform;\n", "const p = navigator.platform;\nconst q = navigator.platform;\n", "platform-check", "navigator.platform"],
	])("%s: old offenders pass, an added one fails, an unchanged file is skipped", async (_n, file, before, after, rule, text) => {
		const root = await baseRepo({ [file]: before });
		put(root, "docs/x.md", "# X\n");
		expect(offenders(await check(root), rule)).toEqual([]);
		put(root, file, after);
		const fs = offenders(await check(root), rule);
		expect(fs).toHaveLength(1);
		expect(fs[0]).toMatchObject({ severity: "error", file });
		expect(fs[0].message).toContain(text);
		put(root, file, before);
		for (let i = 0; i < 50; i++) put(root, `crates/atlas-cli/src/old${i}.rs`, 'fn f() { println!("old"); }\n');
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "more");
		await withEnv({ ATLAS_BASE: "HEAD" }, async () => expect(offenders(await check(root), rule)).toEqual([]));
	});

	test("writing: a new em dash in a changed English doc fails, the old ones do not, an untouched doc is skipped", async () => {
		const rules = `rules:\n  - { id: em-dash, lang: any, severity: error, word: false, pattern: '[\\u2014]', why: x, suggest: Pick the relation }\n`;
		const root = await baseRepo({ "i18n/style/flagged-terms.yaml": rules, "README.md": "# Atlas\n\nOld \u2014 dash.\n", "docs/other.md": "# O\n\nA \u2014 B\n\nC \u2014 D\n" });
		put(root, "README.md", "# Atlas\n\nOld \u2014 dash.\n\nNew \u2014 dash.\n");
		const fs = (await check(root, "writing")).filter((f) => f.rule === "writing-em-dash");
		expect(fs).toHaveLength(1);
		expect(fs[0]).toMatchObject({ file: "README.md", line: 5, severity: "error" });
	});

	test("the committed-baseline checks (stale entries) are off in diff mode, and on without a base", async () => {
		const files = { [RS]: 'fn main() { println!("old one"); }\n', "i18n/hardcoded-baseline.json": JSON.stringify({ rust: { [RS]: 5 } }) };
		const root = await baseRepo(files);
		put(root, "docs/x.md", "# X\n");
		expect((await check(root)).filter((f) => f.rule.endsWith("-baseline"))).toEqual([]);
		const plain = await makeRepo(files);
		put(plain, "i18n/hardcoded-baseline.json", files["i18n/hardcoded-baseline.json"]);
		expect((await check(plain)).filter((f) => f.rule === "hardcoded-baseline")).toHaveLength(1);
	});
});

// GitHub checks a pull request out as the merge of its branch into the CURRENT main. The resolver's pull_request mode
// takes the merge's first parent (the main tip it was built on), not the base sha the event recorded when the branch
// started, which would blame the PR for main code merged since.
describe("a pull request merge ref", () => {
	const MAIN_SCREEN = "apps/desktop/src/FromMain.svelte";
	const PR_SCREEN = "apps/desktop/src/FromPr.svelte";
	const PR = { GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "pull_request" };

	/** main gains a screen after the PR branched, then the merge ref: a real merge commit of the PR into main. */
	async function mergeRef(prFile?: string) {
		const root = await baseRepo({ [OLD]: oldScreen(1) });
		put(root, "docs/pr.md", "# The PR\n");
		if (prFile) put(root, prFile, "<p>Added by the PR</p>\n");
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "pr");
		const branchPoint = git(root, "rev-parse", "main");
		git(root, "checkout", "-q", "main");
		put(root, MAIN_SCREEN, oldScreen(3));
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "main adds a screen after the PR branched");
		git(root, "checkout", "-q", "-b", "merge-ref");
		git(root, "merge", "-q", "--no-ff", "-m", "merge ref", "feature");
		return { root, branchPoint };
	}

	test("in pull_request mode main's code is not the PR's", async () => {
		const { root } = await mergeRef();
		await withEnv(PR, async () => expect(offenders(await check(root))).toEqual([]));
	});

	test("the sha recorded where the branch started WOULD blame the PR for main's screen (why the first parent)", async () => {
		const { root, branchPoint } = await mergeRef();
		await withEnv({ ATLAS_BASE: branchPoint }, async () => {
			expect(offenders(await check(root)).map((f) => f.file)).toEqual([MAIN_SCREEN, MAIN_SCREEN, MAIN_SCREEN]);
		});
	});

	test("a PR that adds a string itself fails on exactly that one", async () => {
		const { root } = await mergeRef(PR_SCREEN);
		await withEnv(PR, async () => {
			const fs = offenders(await check(root));
			expect(fs).toHaveLength(1);
			expect(fs[0]).toMatchObject({ file: PR_SCREEN, line: 1 });
		});
	});

	test("CI without a base (a push of a new branch, a manual run) compares with the committed baseline", async () => {
		const { root } = await mergeRef();
		put(root, "i18n/hardcoded-baseline.json", "{}\n");
		await withEnv({ GITHUB_ACTIONS: "true", GITHUB_EVENT_NAME: "workflow_dispatch" }, async () => {
			expect(offenders(await check(root)).length).toBeGreaterThan(0);
		});
	});

	test("locally, merge-base with main ignores main's code once the branch has merged main", async () => {
		const { root } = await mergeRef();
		// on the merge-ref branch, main is an ancestor: merge-base(main, HEAD) is main's tip
		expect(offenders(await check(root))).toEqual([]);
		git(root, "checkout", "-q", "feature");
		git(root, "merge", "-q", "--no-edit", "main");
		expect(offenders(await check(root))).toEqual([]);
	});
});
