import { describe, expect, test } from "bun:test";
import { shArgv } from "./sh";
import { type Facts, conventional, decide, placeholder } from "./pr-merge-decide";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SHA = "a".repeat(40);
const OLD = "b".repeat(40);
const good: Facts = {
  repo: "codeatlasdev/atlas",
  number: 509,
  title: "fix(desktop): the first click after closing a modal is delivered",
  state: "OPEN",
  isDraft: false,
  headBranch: "fix/modal-click",
  headSha: SHA,
  baseRef: "main",
  behindBy: 0,
  branchTip: SHA,
  runs: [{ headSha: SHA, status: "completed", conclusion: "success", createdAt: "2026-11-16T10:00:00Z", url: "https://x/1" }],
  linked: ["codeatlasdev/atlas#501", "codeatlasdev/atlas#504"],
  signoff: { name: "Matheus", email: "matheus@codeatlas.com.br" },
  login: "mvilacad",
  maintainer: true,
};
const run = (o: Partial<Facts["runs"][number]>) => ({ ...good.runs[0], ...o });

describe("decide: refusals, one reason each, every one names its fix", () => {
  const cases: [string, Partial<Facts>, RegExp][] = [
    ["behind main", { behindBy: 3 }, /3 commit\(s\) behind main.*git merge origin\/main/],
    ["head is not the branch tip", { branchTip: OLD }, /not the tip of fix\/modal-click \(bbbbbbbb\)/],
    ["branch deleted", { branchTip: null }, /no longer exists/],
    ["CI pending", { runs: [run({ status: "in_progress", conclusion: null })] }, /is in_progress: wait/],
    ["CI queued", { runs: [run({ status: "queued", conclusion: null })] }, /is queued/],
    ["CI failed", { runs: [run({ conclusion: "failure" })] }, /concluded failure/],
    ["CI cancelled", { runs: [run({ conclusion: "cancelled" })] }, /concluded cancelled/],
    ["no CI run at all", { runs: [] }, /no CI run for head aaaaaaaa/],
    // The statusCheckRollup trap: a green run of the PREVIOUS head must not pass the new one.
    ["green run belongs to an older head", { runs: [run({ headSha: OLD })] }, /no CI run for head aaaaaaaa/],
    ["title not conventional", { title: "Fix the modal click" }, /title is not a conventional commit subject/],
    ["draft", { isDraft: true }, /is a draft.*gh pr ready 509/],
    ["already merged", { state: "MERGED" }, /is MERGED/],
    ["not a maintainer", { maintainer: false, login: "someone" }, /someone is not an active member of atlas-maintainers/],
    ["no git identity", { signoff: null }, /user\.name \/ user\.email are not set/],
    ["placeholder identity", { signoff: { name: "First User", email: "first@example.com" } }, /placeholder/],
    ["--subject not conventional", { subject: "fixed it" }, /--subject is not a conventional/],
  ];
  test.each(cases)("%s", (_name, patch, reason) => {
    const d = decide({ ...good, ...patch });
    expect(d.decision).toBe("refuse");
    expect(d.reasons).toHaveLength(1);
    expect(d.reasons[0]).toMatch(reason);
  });

  test("every problem is listed, not just the first", () => {
    const d = decide({ ...good, behindBy: 1, runs: [], title: "nope", maintainer: false });
    expect(d.reasons).toHaveLength(4);
  });
});

describe("decide: CI is the run of this exact head", () => {
  test("the newest run of the head wins (a rerun after a failure passes)", () => {
    const failed = run({ conclusion: "failure", createdAt: "2026-11-16T09:00:00Z" });
    expect(decide({ ...good, runs: [failed, run({})] }).decision).toBe("merge");
    const newerFailed = run({ conclusion: "failure", createdAt: "2026-11-16T11:00:00Z" });
    expect(decide({ ...good, runs: [run({}), newerFailed] }).decision).toBe("refuse");
  });
  test("runs of other commits are ignored even when newer", () => {
    expect(decide({ ...good, runs: [run({}), run({ headSha: OLD, conclusion: "failure", createdAt: "2027-01-01T00:00:00Z" })] }).decision).toBe("merge");
  });
});

describe("decide: subject and body", () => {
  test("all green: merge, subject '<title> (#N)', body is Closes lines from the linked issues plus the sign-off", () => {
    const d = decide(good);
    expect(d.decision).toBe("merge");
    expect(d.reasons).toEqual([]);
    expect(d.subject).toBe(`${good.title} (#509)`);
    expect(d.body).toBe("Closes #501\nCloses #504\n\nSigned-off-by: Matheus <matheus@codeatlas.com.br>");
  });
  test("no linked issues: the body is only the sign-off", () => {
    expect(decide({ ...good, linked: [] }).body).toBe("Signed-off-by: Matheus <matheus@codeatlas.com.br>");
  });
  test("a title that already ends in (#N) is not suffixed twice; --subject overrides the title", () => {
    expect(decide({ ...good, title: "fix: x (#509)" }).subject).toBe("fix: x (#509)");
    expect(decide({ ...good, subject: "fix(desktop): shorter" }).subject).toBe("fix(desktop): shorter (#509)");
  });
  test("a refused decision never carries a placeholder sign-off", () => {
    const d = decide({ ...good, signoff: { name: "Your Name", email: "you@MacBook.local" } });
    expect(d.body).not.toContain("Signed-off-by");
  });
});

// The TS rules must say what the shell scripts the CI runs say; both are fed the same table.
describe("conventional() agrees with scripts/check-pr-title.sh", () => {
  const titles = [
    "feat: add the deploy engine",
    "fix(desktop): keep the sidebar width",
    "feat!: drop the v1 config",
    'Revert "feat: add the deploy engine"',
    "Add the deploy engine",
    "feat:add",
    "feat: ",
    "Feat: add",
    "feature: add",
    "fix(Desktop): x",
    "fix(): x",
    "Merge pull request #3 from x/y",
  ];
  test.each(titles)("%p", (t) => {
    const p = Bun.spawnSync(shArgv(join(import.meta.dir, "check-pr-title.sh"), t));
    expect(conventional(t)).toBe(p.exitCode === 0);
  });
});

describe("placeholder() agrees with scripts/check-dco.sh", () => {
  const ids: [string, string][] = [
    ["Matheus", "matheus@codeatlas.com.br"],
    ["First User", "first@example.com"],
    ["Real Name", "you@MacBook.local"],
    ["Real Name", "me@localhost"],
    ["Real Name", "me@foo.test"],
    ["Your Name", "real@codeatlas.com.br"],
    ["test user", "real@codeatlas.com.br"],
    ["Real Name", "me@sub.example.org"],
    ["Luan", "luan@gmail.com"],
  ];
  test.each(ids)("%s <%s>", (name, email) => {
    const dir = mkdtempSync(join(tmpdir(), "pm-dco-"));
    const env = { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" };
    const git = (args: string[], extra: Record<string, string> = {}) => {
      const r = Bun.spawnSync(["git", ...args], { cwd: dir, env: { ...env, ...extra } });
      if (r.exitCode !== 0) throw new Error(r.stderr.toString());
      return r.stdout.toString().trim();
    };
    const as = (n: string, e: string) => ({ GIT_AUTHOR_NAME: n, GIT_AUTHOR_EMAIL: e, GIT_COMMITTER_NAME: n, GIT_COMMITTER_EMAIL: e });
    git(["init", "-q", "-b", "main"]);
    git(["commit", "-q", "--allow-empty", "-m", "base"], as("Base", "base@codeatlas.com.br"));
    const base = git(["rev-parse", "HEAD"]);
    git(["commit", "-q", "--allow-empty", "-m", `fix: x\n\nSigned-off-by: ${name} <${email}>`], as(name, email));
    const r = Bun.spawnSync(shArgv(join(import.meta.dir, "check-dco.sh"), base, git(["rev-parse", "HEAD"])), { cwd: dir, env });
    expect(placeholder({ name, email })).toBe(r.exitCode !== 0);
  });
});
