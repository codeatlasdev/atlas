import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// scripts/check-dco.sh is the required "Signed-off-by" check. Driven against a real repository exactly as pr.yml passes
// base..head. One repository per file: every case chains its commits on the same base with `git commit-tree`, so a case
// costs one git spawn per commit plus the script (a repository per case was ~10 spawns each, and on a loaded Windows runner
// that is what ran into bun's 5 s test timeout, #550).
const script = join(import.meta.dir, "check-dco.sh");

const dir = mkdtempSync(join(tmpdir(), "dco-"));
afterAll(() => {
  // Best effort: on Windows git's read-only object files can refuse deletion, and a leftover temp dir is no failure.
  try {
    rmSync(dir, { recursive: true, force: true });
  } catch {}
});

const git = (args: string[], env: Record<string, string> = {}) => {
  const r = Bun.spawnSync(["git", ...args], {
    cwd: dir,
    env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null", ...env },
  });
  if (r.exitCode !== 0) throw new Error(r.stderr.toString());
  return r.stdout.toString().trim();
};
const as = (name: string, email: string) => ({
  GIT_AUTHOR_NAME: name,
  GIT_AUTHOR_EMAIL: email,
  GIT_COMMITTER_NAME: name,
  GIT_COMMITTER_EMAIL: email,
});
git(["init", "-q", "-b", "main"]);
const tree = Bun.spawnSync(["git", "mktree"], { cwd: dir, stdin: new Blob([""]) }).stdout.toString().trim(); // the empty tree
const base = git(["commit-tree", tree, "-m", "base"], as("Base", "base@codeatlas.com.br"));

function repo(commits: { name: string; email: string; message: string }[]) {
  let head = base;
  for (const c of commits) head = git(["commit-tree", tree, "-p", head, "-m", c.message], as(c.name, c.email));
  const r = Bun.spawnSync(["bash", script, base, head], { cwd: dir });
  return { code: r.exitCode, out: r.stdout.toString() + r.stderr.toString() };
}

const signed = (name: string, email: string) => `fix: x\n\nSigned-off-by: ${name} <${email}>`;

test("a signed-off commit by a real identity passes", () => {
  const r = repo([{ name: "Matheus", email: "matheus@codeatlas.com.br", message: signed("Matheus", "matheus@codeatlas.com.br") }]);
  expect(r.code).toBe(0);
});

test("a commit without Signed-off-by fails", () => {
  const r = repo([{ name: "Matheus", email: "matheus@codeatlas.com.br", message: "fix: x" }]);
  expect(r.code).toBe(1);
  expect(r.out).toContain("has no Signed-off-by");
});

// Incident (1.5.0-alpha.26): two commits on main were authored "First User <first@example.com>" by a test session on
// Linux, signed off with the same placeholder, so the DCO check let them through and nobody was credited for them.
for (const [name, email] of [
  ["First User", "first@example.com"],
  ["Matheus", "matheus@example.org"],
  ["Matheus", "dev@build.test"],
  ["Matheus", "matheus@MacBook-Pro.local"],
  ["Matheus", "root@localhost"],
  ["Your Name", "matheus@codeatlas.com.br"],
] as const) {
  test(`a placeholder identity (${name} <${email}>) fails even when signed off`, () => {
    const r = repo([{ name, email, message: signed(name, email) }]);
    expect(r.code).toBe(1);
    expect(r.out).toContain("placeholder identity");
  });
}

test("dependabot stays exempt", () => {
  const r = repo([
    {
      name: "dependabot[bot]",
      email: "49699333+dependabot[bot]@users.noreply.github.com",
      message: "chore(deps): bump x",
    },
  ]);
  expect(r.code).toBe(0);
});
