// `bun run pr:merge` end to end against a fake `gh` (GH=…): the decision table is in pr-merge-decide.test.ts; here
// the CLI proves it reads the right API answers, never calls `gh pr merge` when it refuses or in a dry run, and
// calls it with exactly the guarded arguments when it merges.
import { expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SHA = "a".repeat(40);
const script = join(import.meta.dir, "pr-merge.ts");

// A fake `gh` every OS can run: a JS file (GH=<it> makes pr-merge.ts run it through Bun) answering from world.json.
const FAKE_GH = `
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const w = JSON.parse(readFileSync(join(import.meta.dir, "world.json"), "utf8"));
const a = process.argv.slice(2);
const out = (s) => { console.log(typeof s === "string" ? s : JSON.stringify(s)); process.exit(0); };
if (a[0] === "repo") out("codeatlasdev/atlas");
if (a[0] === "pr" && a[1] === "view") out(a[4].startsWith("number") ? w.pr : "c".repeat(40));
if (a[0] === "pr" && a[1] === "merge") { writeFileSync(join(import.meta.dir, "merge.json"), JSON.stringify(a)); process.exit(0); }
if (a[0] === "api") {
  const path = a[1];
  if (path === "user") out("mvilacad");
  if (path.includes("/compare/main...")) out(String(w.behind));
  if (path.includes("/git/ref/heads/fix/click")) out(w.tip);
  if (path.includes("/actions/workflows/ci.yml/runs")) out(w.runs);
  if (path.endsWith("/jobs")) out(w.jobs.map((j) => JSON.stringify({ id: j.id, name: j.name, conclusion: j.conclusion })).join("\\n"));
  const m = path.match(/check-runs\\/(\\d+)\\/annotations/);
  if (m) out(w.jobs.find((j) => String(j.id) === m[1]).failures);
  if (path.includes("/teams/atlas-maintainers/memberships/")) {
    if (w.maintainer) out("active");
    console.error("Not Found (HTTP 404)"); process.exit(1);
  }
}
console.error("unexpected gh " + a.join(" ")); process.exit(1);
`;

type Job = { id: number; name: string; conclusion: string; failures: string[] };
type World = { behind?: number; tip?: string; runs?: object[]; maintainer?: boolean; linked?: number[]; title?: string; jobs?: Job[] };

function cli(w: World, args: string[]) {
  const dir = mkdtempSync(join(tmpdir(), "pr-merge-"));
  const pr = {
    number: 7,
    title: w.title ?? "fix(desktop): the click is delivered",
    state: "OPEN",
    isDraft: false,
    headRefName: "fix/click",
    headRefOid: SHA,
    baseRefName: "main",
    headRepository: { name: "atlas" },
    headRepositoryOwner: { login: "codeatlasdev" },
    closingIssuesReferences: (w.linked ?? [501, 504]).map((n) => ({ number: n, repository: { name: "atlas", owner: { login: "codeatlasdev" } } })),
  };
  const runs = w.runs ?? [{ headSha: SHA, status: "completed", conclusion: "success", createdAt: "2026-11-16T10:00:00Z", url: "https://x/1" }];
  writeFileSync(join(dir, "gh.mjs"), FAKE_GH);
  writeFileSync(
    join(dir, "world.json"),
    JSON.stringify({ pr: JSON.stringify(pr), behind: w.behind ?? 0, tip: w.tip ?? SHA, runs: JSON.stringify(runs), maintainer: w.maintainer !== false, jobs: w.jobs ?? [] }),
  );
  const gitconfig = join(dir, "gitconfig");
  writeFileSync(gitconfig, "[user]\n\tname = Matheus\n\temail = matheus@codeatlas.com.br\n");
  const p = Bun.spawnSync([process.execPath, script, "7", "--json", ...args], {
    cwd: dir,
    env: { ...process.env, GH: join(dir, "gh.mjs"), GIT_CONFIG_GLOBAL: gitconfig, GIT_CONFIG_SYSTEM: "/dev/null" },
  });
  const log = join(dir, "merge.json");
  const merged: string[] | null = existsSync(log) ? JSON.parse(readFileSync(log, "utf8")) : null;
  return { code: p.exitCode, out: JSON.parse(p.stdout.toString() || "{}"), err: p.stderr.toString(), merged };
}

const body = "Closes #501\nCloses #504\n\nSigned-off-by: Matheus <matheus@codeatlas.com.br>";

test("--dry-run on a mergeable PR prints the decision and body and merges nothing", () => {
  const r = cli({}, ["--dry-run"]);
  expect(r.code).toBe(0);
  expect(r.out).toEqual({
    pr: 7,
    decision: "merge",
    reasons: [],
    subject: "fix(desktop): the click is delivered (#7)",
    body,
  });
  expect(r.merged).toBeNull();
});

test("a mergeable PR is merged with --squash --admin --match-head-commit <head>, the composed subject and body", () => {
  const r = cli({}, []);
  expect(r.code).toBe(0);
  expect(r.out.mergedSha).toBe("c".repeat(40));
  expect(r.merged).toEqual([
    "pr", "merge", "7", "--squash", "--admin", "--match-head-commit", SHA,
    "--subject", "fix(desktop): the click is delivered (#7)", "--body", body,
  ]);
});

test("a PR with no linked issues merges with only the sign-off in the body", () => {
  const r = cli({ linked: [] }, ["--dry-run"]);
  expect(r.out.body).toBe("Signed-off-by: Matheus <matheus@codeatlas.com.br>");
});

test.each<[string, World, RegExp]>([
  ["behind main", { behind: 2 }, /2 commit\(s\) behind main/],
  ["red CI", { runs: [{ headSha: SHA, status: "completed", conclusion: "failure", createdAt: "2026-11-16T10:00:00Z", url: "u" }] }, /concluded failure/],
  ["only an older head went green", { runs: [{ headSha: "b".repeat(40), status: "completed", conclusion: "success", createdAt: "2026-11-16T10:00:00Z", url: "u" }] }, /no CI run for head/],
  ["head is not the branch tip", { tip: "d".repeat(40) }, /not the tip of fix\/click/],
  ["bad title", { title: "fixed the click" }, /not a conventional/],
  ["not a maintainer", { maintainer: false }, /not an active member of atlas-maintainers/],
])("refuses and never calls gh pr merge: %s", (_n, w, reason) => {
  const r = cli(w, []);
  expect(r.code).toBe(1);
  expect(r.out.decision).toBe("refuse");
  expect(r.out.reasons.join("\n")).toMatch(reason);
  expect(r.merged).toBeNull();
});

// Issue #520: a red run whose only failure is a lost runner is a rerun to wait for, not a fix to make. The CLI reads the
// failed jobs' annotations to tell (jobs 11 lost, 12 green, 13 the aggregate, 14 a real failure).
const LOST_MSG = "The hosted runner lost communication with the server. Anything in your workflow ...";
const red = (attempt: number) => [{ id: 99, attempt, headSha: SHA, status: "completed", conclusion: "failure", createdAt: "2026-11-16T10:00:00Z", url: "https://x/99" }];
const lostJob: Job = { id: 11, name: "Rust (Windows, full workspace)", conclusion: "failure", failures: [LOST_MSG] };
const greenJob: Job = { id: 12, name: "Rust (Linux)", conclusion: "success", failures: [] };
const aggregateJob: Job = { id: 13, name: "CI", conclusion: "failure", failures: [] };
const realJob: Job = { id: 14, name: "Rust (macOS)", conclusion: "failure", failures: ["Process completed with exit code 100."] };

test.each<[string, World, RegExp]>([
  ["only a lost runner, attempt 1", { runs: red(1), jobs: [lostJob, greenJob, aggregateJob] }, /runner lost, rerun pending \(attempt 1 of 3/],
  ["only a lost runner, attempt 3", { runs: red(3), jobs: [lostJob, greenJob, aggregateJob] }, /on every attempt \(3 of 3\): no rerun is pending/],
  ["a lost runner and a real failure", { runs: red(1), jobs: [lostJob, realJob, aggregateJob] }, /concluded failure: fix it and push/],
])("a red CI run is told apart by its jobs' annotations: %s", (_n, w, reason) => {
  const r = cli(w, ["--dry-run"]);
  expect(r.code).toBe(1);
  expect(r.out.reasons.join("\n")).toMatch(reason);
  expect(r.merged).toBeNull();
});

test("bad usage exits 2", () => {
  const p = Bun.spawnSync([process.execPath, script, "--nope"]);
  expect(p.exitCode).toBe(2);
});

test("a gh that cannot be spawned is a JSON refusal on stdout, not silence", () => {
  const p = Bun.spawnSync([process.execPath, script, "7", "--dry-run", "--json"], { env: { ...process.env, GH: join(tmpdir(), "no-such-gh-binary") } });
  const out = JSON.parse(p.stdout.toString());
  expect(p.exitCode).toBe(1);
  expect(out.decision).toBe("refuse");
  expect(out.reasons[0]).toContain("cannot run");
});
