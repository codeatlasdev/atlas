// bun scripts/main-closes-check.ts --repo <owner/repo> --range <before>..<after> [--comment] [--json]
// Run by .github/workflows/closes.yml on every push to main: for each squash commit in the range whose subject ends
// in `(#N)`, every `Closes|Fixes|Resolves #M` in its message must be an issue pull request N links
// (`closingIssuesReferences`). A commit that closes anything else closed an issue by mistake: the job fails, and
// with --comment it says so on the pull request. `bun run pr:merge` writes bodies that always pass. `GH` replaces
// the gh binary (tests; see gh.ts). Exit 0: clean, 1: a wrong closure, 2: bad usage.
import { ghOut, run } from "./gh";
import { prOfSubject, wrongCloses } from "./pr-closes";

async function git(...args: string[]): Promise<string> {
  const r = await run(["git", ...args]);
  if (r.code !== 0) throw new Error(`git ${args[0]} failed: ${r.err || r.out}`);
  return r.out;
}

export type Finding = { commit: string; pr: number; subject: string; wrong: string[]; linked: string[] };

/** Commits of `range` (`before..after`; an all-zero or unknown `before` means only `after`) as {sha, message}. */
async function commits(range: string): Promise<{ sha: string; message: string }[]> {
  const [before, after] = range.split("..");
  const log = (...spec: string[]) => git("log", "--no-merges", "--format=%H%x1f%B%x1e", ...spec);
  const out = /^0+$/.test(before) ? await log("-1", after) : await log(range).catch(() => log("-1", after));
  return out
    .split("\x1e")
    .map((c) => c.trim())
    .filter(Boolean)
    .map((c) => {
      const [sha, message] = c.split("\x1f");
      return { sha, message };
    });
}

export async function check(repo: string, range: string): Promise<Finding[]> {
  const findings: Finding[] = [];
  for (const { sha, message } of await commits(range)) {
    const subject = message.split("\n")[0];
    const pr = prOfSubject(subject);
    if (pr === null) continue;
    const view = await ghOut("pr", "view", String(pr), "--repo", repo, "--json", "closingIssuesReferences");
    const linked = (
      JSON.parse(view).closingIssuesReferences as { number: number; repository: { name: string; owner: { login: string } } }[]
    ).map((i) => `${i.repository.owner.login}/${i.repository.name}#${i.number}`);
    const wrong = wrongCloses(message, linked, repo);
    if (wrong.length) findings.push({ commit: sha, pr, subject, wrong, linked });
  }
  return findings;
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  const opt = (name: string) => argv[argv.indexOf(name) + 1];
  const repo = argv.includes("--repo") ? opt("--repo") : "";
  const range = argv.includes("--range") ? opt("--range") : "";
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo) || !/^[0-9a-f]{7,40}\.\.[0-9a-f]{7,40}$/.test(range)) {
    console.error("usage: bun scripts/main-closes-check.ts --repo <owner/repo> --range <before>..<after> [--comment] [--json]");
    process.exit(2);
  }
  const json = argv.includes("--json");
  let findings: Finding[];
  try {
    findings = await check(repo, range);
  } catch (e) {
    // gh or git could not answer: say so (as JSON too), never exit with nothing on stdout.
    const error = e instanceof Error ? e.message : String(e);
    console.error(`::error::${error}`);
    if (json) console.log(JSON.stringify({ range, error, findings: [] }, null, 2));
    process.exit(1);
  }
  for (const f of findings) {
    const linked = f.linked.length ? f.linked.join(", ") : "nothing";
    const msg = `${f.commit.slice(0, 8)} (${f.subject}) closes ${f.wrong.join(", ")} but #${f.pr} links ${linked}`;
    console.error(`::error::${msg}`);
    if (argv.includes("--comment")) {
      // A comment that cannot be posted must not hide the finding: the run fails either way.
      await ghOut(
        "pr",
        "comment",
        String(f.pr),
        "--repo",
        repo,
        "--body",
        `The squash commit \`${f.commit}\` on main closes ${f.wrong.join(", ")}, which this pull request does not link (it links ${linked}). Reopen what was closed by mistake; merge with \`bun run pr:merge ${f.pr}\`, which writes the \`Closes\` lines from the linked issues.`,
      ).catch((e) => console.error(`::warning::${e instanceof Error ? e.message : e}`));
    }
  }
  if (json) console.log(JSON.stringify({ range, findings }, null, 2));
  else if (!findings.length) console.log(`closing keywords: ok (${range})`);
  process.exit(findings.length ? 1 : 0);
}
