// Which issues a squash commit closes: what GitHub reads from a commit message on the default branch, and what
// `bun run pr:merge` writes there. The one rule is "the commit closes exactly what its pull request links"
// (`closingIssuesReferences`), never a hand-typed list.
//
// Incident (2026-11-16): f44c293d, the squash of PR #509, said `Closes #503`, a number typed from memory. #509 links
// #501 and #504; #503 was closed by the wrong commit and its real fix (#511) had nothing to close.

export type Signoff = { name: string; email: string };

/** `owner/repo#N`: the one spelling every reference is compared in. */
export const refKey = (repo: string, n: number): string => `${repo.toLowerCase()}#${n}`;

// GitHub's closing keywords (close, fix, resolve in their three forms), then `#N`, `owner/repo#N` or an issue URL.
const KEYWORD =
  /\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\b:?[ \t]+(?:https:\/\/github\.com\/([\w.-]+\/[\w.-]+)\/issues\/(\d+)|(?:([\w.-]+\/[\w.-]+))?#(\d+))/gi;

/** Every issue a commit message closes, as `owner/repo#N` keys (a bare `#N` is `repo`'s). In order, no duplicates. */
export function closingRefs(message: string, repo: string): string[] {
  const out: string[] = [];
  for (const m of message.matchAll(KEYWORD)) {
    const key = refKey(m[1] ?? m[3] ?? repo, Number(m[2] ?? m[4]));
    if (!out.includes(key)) out.push(key);
  }
  return out;
}

/** The pull request a squash subject names: `feat: x (#509)` is 509. Only the last `(#N)` counts. */
export function prOfSubject(subject: string): number | null {
  const m = /\(#(\d+)\)\s*$/.exec(subject.trim());
  return m ? Number(m[1]) : null;
}

/** Refs the message closes that the pull request does not link: each one is an issue closed by mistake. */
export function wrongCloses(message: string, linked: string[], repo: string): string[] {
  const ok = new Set(linked.map((k) => k.toLowerCase()));
  return closingRefs(message, repo).filter((k) => !ok.has(k));
}

/** The squash body: `Closes` lines from the linked issues (a `#N` for this repo), then the sign-off. */
export function squashBody(linked: string[], repo: string, signoff: Signoff | null): string {
  const lines = [...new Set(linked.map((k) => k.toLowerCase()))]
    .sort((a, b) => a.localeCompare(b, "en", { numeric: true }))
    .map((k) => `Closes ${k.startsWith(`${repo.toLowerCase()}#`) ? k.slice(repo.length) : k}`);
  const trailer = signoff ? [`Signed-off-by: ${signoff.name} <${signoff.email}>`] : [];
  return [lines.join("\n"), trailer.join("\n")].filter(Boolean).join("\n\n");
}
