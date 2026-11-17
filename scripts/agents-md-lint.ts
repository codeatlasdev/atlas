// bun scripts/agents-md-lint.ts [--json]
//
// AGENTS.md is loaded into every agent session and every subagent, so it is a map, not a manual (an 18 KB one, 55%
// of it the desktop-debugging manual, was ~4.5k tokens on every run). The details live in `.agents/skills/` and in
// nested AGENTS.md files, which agents read on demand. This checks, and every message ends with `fix: …`:
//  - AGENTS.md is at most 8 KB and has no line over 400 characters (a long line is a paragraph that wants a skill);
//  - every `.agents/skills/<name>` it names exists, and every repo path or link it names exists (link rot);
//  - every `.agents/skills/<name>/SKILL.md` has valid frontmatter (name = directory, one-line description) and its
//    relative .md links resolve: Claude Code and omp skip or mis-list a skill with broken frontmatter;
//  - `.claude/skills` is a symlink to `../.agents/skills` (committed as one, so a checkout that cannot make links,
//    such as Windows without developer mode, still carries the right target). Claude Code reads only
//    `.claude/skills`, not `.agents/`; Codex and omp read `.agents/skills` natively and de-duplicate by realpath.
import { existsSync, lstatSync, readFileSync, readdirSync, readlinkSync, realpathSync } from "node:fs";
import { join } from "node:path";

export const MAX_BYTES = 8 * 1024;
export const MAX_LINE = 400;
export const LINK_TARGET = "../.agents/skills";

export type Problem = { file: string; message: string };

const PATH_ROOTS = /^(\.agents|\.claude|\.github|crates|apps|docs|scripts|skills|studio|services|site|packaging|assets)\//;
const ROOT_FILE = /^[A-Za-z0-9_.-]+\.(md|toml|yaml)$/;

/** Repo paths named in AGENTS.md: words inside backticks and markdown link targets that look like files here. */
function namedPaths(text: string): string[] {
  const words = [...text.matchAll(/`([^`\n]+)`/g)].flatMap((m) => m[1].split(/\s+/));
  const links = [...text.matchAll(/\]\(([^)\s]+)\)/g)].map((m) => m[1]);
  return [...words, ...links]
    .map((w) => w.replace(/#.*$/, "").replace(/[.,;:]+$/, ""))
    .filter((w) => !/[<>*…{}|$]/.test(w) && (PATH_ROOTS.test(w) || ROOT_FILE.test(w)));
}

function skillProblems(root: string): Problem[] {
  const out: Problem[] = [];
  const dir = join(root, ".agents/skills");
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const base = join(dir, name);
    if (!lstatSync(base).isDirectory()) continue;
    const file = `.agents/skills/${name}/SKILL.md`;
    if (!existsSync(join(root, file))) {
      out.push({ file, message: `missing SKILL.md (fix: add ${file}; a skill is a directory with that file)` });
      continue;
    }
    const text = readFileSync(join(root, file), "utf8");
    const fm = /^---\n([\s\S]*?)\n---\n/.exec(text)?.[1];
    if (fm === undefined) {
      out.push({ file, message: "no frontmatter (fix: start the file with --- name: / description: ---)" });
      continue;
    }
    const field = (k: string) => new RegExp(`^${k}:[ \\t]*(.*)$`, "m").exec(fm)?.[1]?.trim() ?? "";
    if (field("name") !== name) out.push({ file, message: `name "${field("name")}" must equal its directory "${name}" (fix: set name: ${name})` });
    const desc = field("description");
    if (!desc || /^[>|]/.test(desc)) out.push({ file, message: "description must be one non-empty line (fix: write what it covers and when to read it)" });
    else if (desc.length > 1024) out.push({ file, message: `description is ${desc.length} chars, max 1024 (fix: shorten it, move detail into the body)` });
    for (const m of text.matchAll(/\]\(([A-Za-z0-9_./-]+\.md)\)/g)) {
      if (!existsSync(join(base, m[1]))) out.push({ file, message: `broken link ${m[1]} (fix: create it or correct the path)` });
    }
  }
  return out;
}

/** `.claude/skills` on disk (not on Windows, where a checkout may hold the link as a text file). */
function linkOnDiskProblems(root: string): Problem[] {
  const file = ".claude/skills";
  const fix = `fix: rm -rf .claude/skills && mkdir -p .claude && ln -s ${LINK_TARGET} .claude/skills`;
  let st;
  try {
    st = lstatSync(join(root, file));
  } catch {
    return [{ file, message: `missing, so Claude Code sees no repo skill (${fix})` }];
  }
  if (!st.isSymbolicLink()) return [{ file, message: `is a ${st.isDirectory() ? "directory" : "file"}; a copy drifts from .agents/skills (${fix})` }];
  if (readlinkSync(join(root, file)) !== LINK_TARGET) return [{ file, message: `points to ${readlinkSync(join(root, file))}, not ${LINK_TARGET} (${fix})` }];
  if (!existsSync(join(root, ".agents/skills")) || realpathSync(join(root, file)) !== realpathSync(join(root, ".agents/skills"))) {
    return [{ file, message: `does not resolve to .agents/skills (${fix})` }];
  }
  return [];
}

/** `.claude/skills` as committed: mode 120000 and blob `../.agents/skills`. Skipped outside a git checkout. */
function linkInIndexProblems(root: string): Problem[] {
  if (!existsSync(join(root, ".git"))) return [];
  const git = (...args: string[]) => Bun.spawnSync(["git", "-C", root, ...args]).stdout.toString().trim();
  const entry = git("ls-files", "-s", "--", ".claude/skills");
  const fix = `fix: git add .claude/skills (a symlink to ${LINK_TARGET}); if it is tracked as a directory, git rm -r --cached .claude/skills first`;
  if (!entry) return [{ file: ".claude/skills", message: `is not tracked by git (${fix})` }];
  const [mode, blob] = entry.split(/\s+/);
  if (mode !== "120000") return [{ file: ".claude/skills", message: `is tracked with mode ${mode}, not as a symlink (${fix})` }];
  const target = git("cat-file", "blob", blob);
  return target === LINK_TARGET ? [] : [{ file: ".claude/skills", message: `is tracked as a link to ${target}, not ${LINK_TARGET} (${fix})` }];
}

export function agentsMdProblems(root: string): Problem[] {
  const out: Problem[] = [];
  const file = "AGENTS.md";
  const text = readFileSync(join(root, file), "utf8");
  const bytes = Buffer.byteLength(text);
  if (bytes > MAX_BYTES) {
    out.push({
      file,
      message: `is ${bytes} bytes, max ${MAX_BYTES} (fix: move the detail into .agents/skills/<name>/SKILL.md or a nested AGENTS.md and leave one line pointing at it)`,
    });
  }
  text.split("\n").forEach((line, i) => {
    if (line.length > MAX_LINE) out.push({ file: `${file}:${i + 1}`, message: `line is ${line.length} chars, max ${MAX_LINE} (fix: split it, or move the paragraph into a skill)` });
  });
  for (const name of new Set([...text.matchAll(/\.agents\/skills\/([A-Za-z0-9_-]+)/g)].map((m) => m[1]))) {
    if (!existsSync(join(root, `.agents/skills/${name}/SKILL.md`))) {
      out.push({ file, message: `names skill "${name}" but .agents/skills/${name}/SKILL.md does not exist (fix: create the skill or correct the name)` });
    }
  }
  for (const p of new Set(namedPaths(text))) {
    // A path inside a missing skill is already reported as that skill.
    const skill = /^\.agents\/skills\/([^/]+)/.exec(p)?.[1];
    if (skill && !existsSync(join(root, `.agents/skills/${skill}/SKILL.md`))) continue;
    if (!existsSync(join(root, p))) out.push({ file, message: `names ${p}, which does not exist (fix: correct the path or drop the mention)` });
  }
  out.push(...skillProblems(root));
  if (process.platform !== "win32") out.push(...linkOnDiskProblems(root));
  out.push(...linkInIndexProblems(root));
  return out;
}

if (import.meta.main) {
  const root = join(import.meta.dir, "..");
  const problems = agentsMdProblems(root);
  if (process.argv.includes("--json")) console.log(JSON.stringify({ ok: problems.length === 0, problems }, null, 2));
  else for (const p of problems) console.error(`${p.file}: ${p.message}`);
  process.exit(problems.length ? 1 : 0);
}
