// The ratchets in diff mode: a change is judged on the offenders it adds, against the merge-base, so a baseline that
// lags main never fails anyone and never needs a re-snapshot. Real temp git repositories, real `git`.
import { afterAll, describe, expect, test } from "bun:test";
import { parseChanged, resolveBase } from "./base.ts";
import { fix } from "./fix.ts";
import { type Finding, loadCtx } from "./types.ts";
import { check, cleanup, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(() => {
	delete process.env.I18N_BASE;
	cleanup();
});

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

describe("resolving the base", () => {
	test("merge-base against main, with committed, staged, unstaged and untracked changes, and renames mapped back", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(2), "docs/a.md": "# A\n" });
		put(root, "docs/a.md", "# A\n\nmore\n");
		put(root, "docs/new.md", "# New\n");
		git(root, "mv", OLD, "apps/desktop/src/Renamed.svelte");
		const b = resolveBase(root);
		expect(b?.ref).toBe("main");
		expect(b?.mb).toBe(git(root, "rev-parse", "main"));
		expect(b?.changed.get("docs/a.md")).toBe("docs/a.md");
		expect(b?.changed.has("docs/new.md")).toBe(true);
		expect(b?.changed.get("docs/new.md")).toBeUndefined();
		expect(b?.changed.get("apps/desktop/src/Renamed.svelte")).toBe(OLD);
		expect(b?.changed.has(OLD)).toBe(false);
	});

	test("origin/main wins over main, and I18N_BASE wins over both", async () => {
		const root = await baseRepo({ "docs/a.md": "# A\n" });
		put(root, "docs/b.md", "# B\n");
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "second");
		git(root, "update-ref", "refs/remotes/origin/main", "HEAD");
		expect(resolveBase(root)?.ref).toBe("origin/main");
		expect(resolveBase(root)?.changed.has("docs/b.md")).toBe(false);
		expect(resolveBase(root, { I18N_BASE: "main" })?.ref).toBe("main");
		expect(resolveBase(root, { I18N_BASE: "main" })?.changed.has("docs/b.md")).toBe(true);
	});

	test("no usable base: not a git checkout, a missing ref, no main", async () => {
		expect(resolveBase(await makeRepo())).toBeUndefined();
		const trunk = await baseRepo({}, "trunk");
		expect(resolveBase(trunk)).toBeUndefined();
		expect(resolveBase(trunk, { I18N_BASE: "no-such-ref" })).toBeUndefined();
		expect(resolveBase(trunk, { I18N_BASE: "trunk" })?.ref).toBe("trunk");
	});
});

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

	test("the base can be given: I18N_BASE (what CI sets) judges from that commit", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(1) });
		put(root, "apps/desktop/src/Second.svelte", "<p>Second</p>\n");
		git(root, "add", "-A");
		git(root, "commit", "-q", "-m", "second");
		expect(offenders(await check(root))).toHaveLength(1);
		process.env.I18N_BASE = "HEAD";
		try {
			expect(offenders(await check(root))).toEqual([]);
		} finally {
			delete process.env.I18N_BASE;
		}
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
		process.env.I18N_BASE = "HEAD";
		try {
			expect(offenders(await check(root), rule)).toEqual([]);
		} finally {
			delete process.env.I18N_BASE;
		}
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

// GitHub checks a pull request out as the merge of its branch into the CURRENT main. The base sha recorded in the
// event is where the branch started, so main code merged since then would look like the PR's. CI passes I18N_BASE =
// HEAD^1, the main tip the merge ref was built on.
describe("a pull request merge ref", () => {
	const MAIN_SCREEN = "apps/desktop/src/FromMain.svelte";
	const PR_SCREEN = "apps/desktop/src/FromPr.svelte";

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
		return { root, branchPoint, mainTip: git(root, "rev-parse", "HEAD^1") };
	}

	test("with I18N_BASE = HEAD^1 main's code is not the PR's", async () => {
		const { root, mainTip } = await mergeRef();
		process.env.I18N_BASE = mainTip;
		try {
			expect(offenders(await check(root))).toEqual([]);
			expect(resolveBase(root)?.changed.has(MAIN_SCREEN)).toBe(false);
			expect([...(resolveBase(root)?.changed.keys() ?? [])].sort()).toEqual(["docs/pr.md"]);
		} finally {
			delete process.env.I18N_BASE;
		}
	});

	test("the sha recorded where the branch started WOULD blame the PR for main's screen (why HEAD^1)", async () => {
		const { root, branchPoint } = await mergeRef();
		process.env.I18N_BASE = branchPoint;
		try {
			expect(offenders(await check(root)).map((f) => f.file)).toEqual([MAIN_SCREEN, MAIN_SCREEN, MAIN_SCREEN]);
		} finally {
			delete process.env.I18N_BASE;
		}
	});

	test("a PR that adds a string itself fails on exactly that one", async () => {
		const { root, mainTip } = await mergeRef(PR_SCREEN);
		process.env.I18N_BASE = mainTip;
		try {
			const fs = offenders(await check(root));
			expect(fs).toHaveLength(1);
			expect(fs[0]).toMatchObject({ file: PR_SCREEN, line: 1 });
		} finally {
			delete process.env.I18N_BASE;
		}
	});

	test("locally, merge-base with main ignores main's code once the branch has merged main", async () => {
		const { root } = await mergeRef();
		// on the merge-ref branch, main is an ancestor: merge-base(main, HEAD) is main's tip
		expect(offenders(await check(root))).toEqual([]);
		git(root, "checkout", "-q", "feature");
		git(root, "merge", "-q", "--no-edit", "main");
		expect(offenders(await check(root))).toEqual([]);
	});

	test("an all-zero `before` (a new branch push) or an unknown ref falls through to main", async () => {
		const root = await baseRepo({ [OLD]: oldScreen(1) });
		put(root, "apps/desktop/src/New.svelte", "<p>New</p>\n");
		for (const bad of ["0000000000000000000000000000000000000000", "not-a-ref", "  "]) {
			expect(resolveBase(root, { I18N_BASE: bad })?.ref).toBe("main");
		}
	});
});

describe("OS-neutral git parsing", () => {
	test("CRLF output (Git for Windows through a pipe) parses like LF, renames included", () => {
		const nameStatus = "M\tdocs/a.md\r\nA\tdocs/new.md\r\nR100\told/Old.svelte\tnew/Renamed.svelte\r\nD\tgone.md\r\n";
		const changed = parseChanged(nameStatus, "untracked.md\r\n\r\n");
		expect([...changed]).toEqual([
			["docs/a.md", "docs/a.md"],
			["docs/new.md", undefined],
			["new/Renamed.svelte", "old/Old.svelte"],
			["untracked.md", undefined],
		]);
		expect(parseChanged(nameStatus.replaceAll("\r\n", "\n"), "untracked.md\n")).toEqual(changed);
	});

	test("the repository is recognised at its top by `rev-parse --show-prefix`, with no path comparison", async () => {
		const root = await baseRepo({ "docs/a.md": "# A\n" });
		expect(resolveBase(root)?.ref).toBe("main");
		// a subdirectory of a work tree is not a root: the ratchets would read paths relative to the wrong place
		expect(resolveBase(`${root}/docs`)).toBeUndefined();
	});
});
