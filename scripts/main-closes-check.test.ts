// The push-to-main check against a real repository (the squash commits) and a fake `gh` (the PRs' links).
import { expect, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
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

  const cases = Object.entries(links)
    .map(([n, issues]) => {
      const refs = issues.map((i) => ({ number: i, repository: { name: "atlas", owner: { login: "o" } } }));
      return `  "pr view ${n} "*) echo '${JSON.stringify({ closingIssuesReferences: refs })}' ;;`;
    })
    .join("\n");
  const comments = join(dir, "comments.log");
  const gh = join(dir, "gh");
  writeFileSync(gh, `#!/usr/bin/env bash\ncase "$*" in\n${cases}\n  "pr comment"*) printf '%s %s\\n' "$3" "$7" >> '${comments}' ;;\n  *) echo "unexpected gh $*" >&2; exit 1 ;;\nesac\n`);
  chmodSync(gh, 0o755);
  const run = (...extra: string[]) =>
    Bun.spawnSync(["bun", script, "--repo", "o/atlas", "--range", `${extra.includes("--first-push") ? "0".repeat(40) : before}..${after}`, ...extra.filter((a) => a !== "--first-push")], {
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
  expect(Bun.spawnSync(["bun", script]).exitCode).toBe(2);
});
