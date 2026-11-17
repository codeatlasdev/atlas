import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { LINK_TARGET, MAX_BYTES, agentsMdProblems } from "./agents-md-lint.ts";

// Incident (AX audit, 2026-11-16): AGENTS.md was 18 KB, 55% the desktop-debugging manual, loaded into every session
// and subagent; and Claude Code does not read `.agents/skills/`, so repo skills were reachable only through prose.
const dirs: string[] = [];
afterEach(() => dirs.splice(0).forEach((d) => rmSync(d, { recursive: true, force: true })));

const SKILL = (name: string, extra = "") => `---\nname: ${name}\ndescription: Does ${name}.\n---\n\n# ${name}\n${extra}`;

/** A tree that passes: AGENTS.md naming one skill and one doc, that skill, and the symlink. */
function repo(files: Record<string, string> = {}, opts: { link?: boolean } = {}): string {
  const root = mkdtempSync(join(tmpdir(), "agents-md-"));
  dirs.push(root);
  const all: Record<string, string> = {
    "AGENTS.md": "# map\n\nRead `.agents/skills/alpha/SKILL.md` and [the guide](docs/guide.md).\n",
    "docs/guide.md": "guide\n",
    ".agents/skills/alpha/SKILL.md": SKILL("alpha"),
    ...files,
  };
  for (const [p, body] of Object.entries(all)) {
    mkdirSync(dirname(join(root, p)), { recursive: true });
    writeFileSync(join(root, p), body);
  }
  if (opts.link !== false) {
    mkdirSync(join(root, ".claude"), { recursive: true });
    symlinkSync(LINK_TARGET, join(root, ".claude/skills"));
  }
  return root;
}
const messages = (root: string) => agentsMdProblems(root).map((p) => `${p.file}: ${p.message}`);

describe.skipIf(process.platform === "win32")("agentsMdProblems", () => {
  test("a small map whose skills, paths and link exist is clean", () => {
    expect(messages(repo())).toEqual([]);
  });

  test("AGENTS.md over 8 KB is flagged with the fix", () => {
    const root = repo({ "AGENTS.md": `# map\n${"word\n".repeat(MAX_BYTES / 5 + 1)}` });
    const [m] = messages(root);
    expect(m).toMatch(/^AGENTS\.md: is \d+ bytes, max 8192 \(fix: move the detail into \.agents\/skills/);
  });

  test("exactly 8 KB is allowed (the limit is inclusive)", () => {
    const head = "# map\n";
    const root = repo({ "AGENTS.md": head + "x\n".repeat((MAX_BYTES - head.length) / 2) });
    expect(messages(root)).toEqual([]);
  });

  test("a line over 400 characters is flagged at its line number", () => {
    const root = repo({ "AGENTS.md": `# map\n${"a".repeat(401)}\n` });
    expect(messages(root)).toEqual([expect.stringMatching(/^AGENTS\.md:2: line is 401 chars, max 400/)]);
  });

  test("a skill named in AGENTS.md that does not exist is flagged, once however often it is named", () => {
    const root = repo({ "AGENTS.md": "`.agents/skills/ghost/SKILL.md` and `.agents/skills/ghost/x.md`\n" });
    expect(messages(root)).toEqual([expect.stringContaining('names skill "ghost"')]);
  });

  test("a repo path in backticks or a link that does not exist is flagged; globs and placeholders are not", () => {
    const root = repo({
      "AGENTS.md": "`docs/missing.md` [x](docs/also-missing.md#top) `crates/*/src` `docs/<name>.md` `bun scripts/nope.ts` `docs/guide.md`\n",
    });
    expect(messages(root).map((m) => /names (\S+),/.exec(m)?.[1]).sort()).toEqual(["docs/also-missing.md", "docs/missing.md", "scripts/nope.ts"]);
  });

  test("a skill with a name that is not its directory, or no description, is flagged", () => {
    const root = repo({
      ".agents/skills/beta/SKILL.md": "---\nname: other\ndescription:\n---\n",
      ".agents/skills/gamma/SKILL.md": "# no frontmatter\n",
      ".agents/skills/delta/notes.md": "no SKILL.md here\n",
    });
    const all = messages(root).join("\n");
    expect(all).toContain('name "other" must equal its directory "beta"');
    expect(all).toContain("beta/SKILL.md: description must be one non-empty line");
    expect(all).toContain("gamma/SKILL.md: no frontmatter");
    expect(all).toContain("delta/SKILL.md: missing SKILL.md");
  });

  test("a skill linking to a sibling file that is not there is flagged", () => {
    const root = repo({ ".agents/skills/alpha/SKILL.md": SKILL("alpha", "[more](more.md)\n") });
    expect(messages(root)).toEqual([expect.stringContaining("broken link more.md")]);
  });

  test("a missing .claude/skills is flagged: Claude Code would see no repo skill", () => {
    expect(messages(repo({}, { link: false }))).toEqual([expect.stringContaining(".claude/skills: missing")]);
  });

  test("a copy of the skills instead of the link is flagged: it drifts", () => {
    const root = repo({ ".claude/skills/alpha/SKILL.md": SKILL("alpha") }, { link: false });
    expect(messages(root)).toEqual([expect.stringContaining("is a directory; a copy drifts")]);
  });

  test("a link to another place is flagged", () => {
    const root = repo({ "elsewhere/alpha/SKILL.md": SKILL("alpha") }, { link: false });
    mkdirSync(join(root, ".claude"));
    symlinkSync("../elsewhere", join(root, ".claude/skills"));
    expect(messages(root)).toEqual([expect.stringContaining("not ../.agents/skills")]);
  });

  test("committed as a directory (not a symlink) is flagged even when the disk looks right", () => {
    const root = repo({ ".claude/skills/alpha/SKILL.md": SKILL("alpha") }, { link: false });
    for (const args of [["init", "-q"], ["add", "-A"]]) Bun.spawnSync(["git", "-C", root, ...args]);
    expect(messages(root).join("\n")).toMatch(/\.claude\/skills: is tracked with mode 100644, not as a symlink/);
  });

  test("committed as the symlink is clean, and an untracked link is flagged", () => {
    const root = repo();
    Bun.spawnSync(["git", "-C", root, "init", "-q"]);
    expect(messages(root)).toEqual([expect.stringContaining("is not tracked by git")]);
    Bun.spawnSync(["git", "-C", root, "add", "-A"]);
    expect(messages(root)).toEqual([]);
  });
});

describe("the repository", () => {
  test("AGENTS.md is a map: under 8 KB, every skill and path it names exists, .claude/skills mirrors .agents/skills", () => {
    expect(messages(join(import.meta.dir, ".."))).toEqual([]);
  });
});
