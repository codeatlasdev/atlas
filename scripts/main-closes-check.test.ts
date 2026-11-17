// The push-to-main check against a real repository (the squash commits) and a fake `gh` (the PRs' links).
import { expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dir, "main-closes-check.ts");

function main(commits: string[], links: Record<number, number[]>) {
  const dir = mkdtempSync(join(tmpdir(), "closes-"));
  const env = { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" };
  const git = (...args: string[]) => {
    const r = Bun.spawnSync(["git", ...args], {
      cwd: dir,
      env: { ...env, GIT_AUTHOR_NAME: "A", GIT_AUTHOR_EMAIL: "a@codeatlas.com.br", GIT_COMMITTER_NAME: "A", GIT_COMMITTER_EMAIL: "a@codeatlas.com.br" },
    });
    if (r.exitCode !== 0) throw new Error(r.stderr.toString());
    return r.stdout.toString().trim();
  };
  git("init", "-q", "-b", "main");
  git("commit", "-q", "--allow-empty", "-m", "base");
  const before = git("rev-parse", "HEAD");
  for (const m of commits) git("commit", "-q", "--allow-empty", "-m", m);
  const after = git("rev-parse", "HEAD");

  // A fake `gh` every OS can run: a JS file (GH=<it> makes the script run it through Bun).
  const world = Object.fromEntries(
    Object.entries(links).map(([n, issues]) => [
      n,
      { closingIssuesReferences: issues.map((i) => ({ number: i, repository: { name: "atlas", owner: { login: "o" } } })) },
    ]),
  );
  const comments = join(dir, "comments.log");
  const gh = join(dir, "gh.mjs");
  writeFileSync(
    gh,
    `import { appendFileSync } from "node:fs";
const world = ${JSON.stringify(world)};
const a = process.argv.slice(2);
if (a[0] === "pr" && a[1] === "view" && world[a[2]]) { console.log(JSON.stringify(world[a[2]])); process.exit(0); }
if (a[0] === "pr" && a[1] === "comment") { appendFileSync(${JSON.stringify(comments)}, a[2] + " " + a[6] + "\\n"); process.exit(0); }
console.error("unexpected gh " + a.join(" ")); process.exit(1);
`,
  );
  const run = (...extra: string[]) =>
    Bun.spawnSync([process.execPath, script, "--repo", "o/atlas", "--range", `${extra.includes("--first-push") ? "0".repeat(40) : before}..${after}`, ...extra.filter((a) => a !== "--first-push")], {
      cwd: dir,
      env: { ...env, GH: gh },
    });
  return { run, commented: () => (existsSync(comments) ? readFileSync(comments, "utf8").trim().split("\n") : []) };
}

// Regression: f44c293d (PR #509) closed #503 though #509 links #501 and #504.
test("a squash commit closing an issue its PR does not link fails and comments on that PR", () => {
  const m = main(["fix: modal click (#509)\n\nCloses #503\n\nSigned-off-by: A <a@codeatlas.com.br>"], { 509: [501, 504] });
  const r = m.run("--comment", "--json");
  expect(r.exitCode).toBe(1);
  const out = JSON.parse(r.stdout.toString());
  expect(out.findings).toHaveLength(1);
  expect(out.findings[0]).toMatchObject({ pr: 509, wrong: ["o/atlas#503"], linked: ["o/atlas#501", "o/atlas#504"] });
  expect(r.stderr.toString()).toContain("::error::");
  expect(m.commented()).toEqual([expect.stringMatching(/^509 .*o\/atlas#503/)]);
});

test("commits that close exactly what their PR links, or close nothing, or are not squash merges, pass", () => {
  const m = main(
    [
      "fix: a (#1)\n\nCloses #10\nFixes #11",
      "fix: b (#2)\n\nSigned-off-by: A <a@codeatlas.com.br>",
      "chore(release): 1.5.0-alpha.27\n\nCloses #99",
    ],
    { 1: [10, 11], 2: [] },
  );
  const r = m.run("--comment");
  expect(r.exitCode).toBe(0);
  expect(m.commented()).toEqual([]);
});

test("a PR that links nothing but whose squash closes an issue is wrong", () => {
  expect(main(["fix: c (#3)\n\nCloses #12"], { 3: [] }).run().exitCode).toBe(1);
});

test("a push with no previous commit (all-zero before) judges only the new head", () => {
  const m = main(["fix: old (#5)\n\nCloses #7", "fix: new (#6)\n\nCloses #8"], { 5: [], 6: [8] });
  expect(m.run("--first-push").exitCode).toBe(0);
  expect(m.run().exitCode).toBe(1); // the same history as a normal range does judge the old commit
});

test("bad usage exits 2", () => {
  expect(Bun.spawnSync([process.execPath, script]).exitCode).toBe(2);
});

test("a gh that cannot be spawned fails with a JSON error on stdout, not silence", () => {
  const dir = mkdtempSync(join(tmpdir(), "closes-nogh-"));
  const git = (...a: string[]) => Bun.spawnSync(["git", ...a], { cwd: dir, env: { ...process.env, GIT_AUTHOR_NAME: "A", GIT_AUTHOR_EMAIL: "a@codeatlas.com.br", GIT_COMMITTER_NAME: "A", GIT_COMMITTER_EMAIL: "a@codeatlas.com.br" } }).stdout.toString().trim();
  git("init", "-q", "-b", "main");
  git("commit", "-q", "--allow-empty", "-m", "base");
  const before = git("rev-parse", "HEAD");
  git("commit", "-q", "--allow-empty", "-m", "fix: f (#9)\n\nCloses #2");
  const p = Bun.spawnSync([process.execPath, script, "--repo", "o/atlas", "--range", `${before}..${git("rev-parse", "HEAD")}`, "--json"], {
    cwd: dir,
    env: { ...process.env, GH: join(tmpdir(), "no-such-gh-binary") },
  });
  expect(p.exitCode).toBe(1);
  expect(JSON.parse(p.stdout.toString()).error).toContain("cannot run");
});
