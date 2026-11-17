// CHANGELOG.md problems the eye misses and a merge creates.
//
// `CHANGELOG.md merge=union` (.gitattributes) keeps both sides of every hunk, so when two branches edit the
// same entry the file ends up with two (or three) versions of it, and a heading can be repeated. The release
// notes are cut from these sections verbatim. Incident (1.5.0-alpha.26 notes, 2026-11-16): `## [Unreleased]`
// held the "per-crewmate isolation" entry three times, at three stages of its text, and "Linux: one title
// row" twice; one copy also linked `[#437](…/issues/443)`.

export type Problem = { line: number; message: string };

/**
 * - an entry title or a `###` heading twice in one `## [version]` section (a union merge kept both sides);
 * - a link whose label is `#N` while its URL points to another issue or pull request number.
 */
export function changelogProblems(text: string): Problem[] {
  const problems: Problem[] = [];
  let section = "";
  let seen = new Map<string, number>();
  text.split("\n").forEach((line, i) => {
    const n = i + 1;
    if (line.startsWith("## ")) {
      section = line.slice(3).trim();
      seen = new Map();
      return;
    }
    // A heading starts a new subsection, but a repeated one in the same version is a merge leftover; an entry
    // is keyed by its bold lead (`- **Title.** body`).
    const key = line.startsWith("### ") ? line.trim() : (/^- \*\*(.+?)\*\*/.exec(line)?.[1] ?? null);
    if (key !== null && section) {
      const first = seen.get(key);
      if (first !== undefined) {
        problems.push({ line: n, message: `${section}: "${key.slice(0, 80)}" again (first at line ${first})` });
      } else {
        seen.set(key, n);
      }
    }
    for (const m of line.matchAll(/\[#(\d+)\]\(https:\/\/github\.com\/[^)\s]+\/(?:issues|pull)\/(\d+)\)/g)) {
      if (m[1] !== m[2]) {
        problems.push({ line: n, message: `link labelled #${m[1]} points to #${m[2]}` });
      }
    }
  });
  return problems;
}

if (import.meta.main) {
  const path = process.argv[2] ?? "CHANGELOG.md";
  const problems = changelogProblems(await Bun.file(path).text());
  for (const p of problems) console.error(`${path}:${p.line}: ${p.message}`);
  process.exit(problems.length ? 1 : 0);
}
