import { expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// scripts/check-dco.sh is the required "Signed-off-by" check. Driven against a real repository: one base commit, then
// the commits under test, exactly as pr.yml passes base..head.
const script = join(import.meta.dir, "check-dco.sh");

function repo(commits: { name: string; email: string; message: string }[]) {
  const dir = mkdtempSync(join(tmpdir(), "dco-"));
  const git = (args: string[], env: Record<string, string> = {}) => {
    const r = Bun.spawnSync(["git", ...args], {
      cwd: dir,
      env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null", ...env },
    });
    if (r.exitCode !== 0) throw new Error(r.stderr.toString());
    return r.stdout.toString().trim();
  };
  git(["init", "-q", "-b", "main"]);
  const as = (name: string, email: string) => ({
    GIT_AUTHOR_NAME: name,
    GIT_AUTHOR_EMAIL: email,
    GIT_COMMITTER_NAME: name,
    GIT_COMMITTER_EMAIL: email,
  });
  git(["commit", "-q", "--allow-empty", "-m", "base"], as("Base", "base@codeatlas.com.br"));
  const base = git(["rev-parse", "HEAD"]);
  for (const c of commits) git(["commit", "-q", "--allow-empty", "-m", c.message], as(c.name, c.email));
  const head = git(["rev-parse", "HEAD"]);
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
