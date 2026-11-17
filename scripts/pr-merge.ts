// bun run pr:merge <pr> [--subject <s>] [--dry-run] [--json]
// The one way a maintainer (or an agent a maintainer runs) squash-merges a pull request. It refuses unless the
// branch is not behind its base, the PR head is the tip of its branch, the CI run of that exact head concluded
// success, and the title is conventional; the squash body is written from the PR's `closingIssuesReferences`
// (never typed) plus the maintainer's Signed-off-by. Merging uses the ruleset bypass, so only an active member of
// `atlas-maintainers` gets that far; everyone else gets the decision and no merge.
//
// --json prints {pr, decision: "merge"|"refuse", reasons[], subject, body, mergedSha?}. Exit 0: merged (or, with
// --dry-run, would merge), 1: refused, 2: bad usage. Why: scripts/pr-closes.ts. How a maintainer uses it:
// docs/release.md "Merging a pull request". `GH` overrides the gh binary (tests).
import { type Facts, decide } from "./pr-merge-decide";

const gh = process.env.GH ?? "gh";

async function run(cmd: string[]): Promise<{ code: number; out: string; err: string }> {
  const p = Bun.spawn(cmd, { stdout: "pipe", stderr: "pipe", stdin: "ignore" });
  const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  return { code, out: out.trim(), err: err.trim() };
}

async function ghOut(...args: string[]): Promise<string> {
  const r = await run([gh, ...args]);
  if (r.code !== 0) throw new Error(`gh ${args.slice(0, 3).join(" ")} failed: ${r.err || r.out}`);
  return r.out;
}

type PrView = {
  number: number;
  title: string;
  state: string;
  isDraft: boolean;
  headRefName: string;
  headRefOid: string;
  baseRefName: string;
  headRepository: { name: string } | null;
  headRepositoryOwner: { login: string } | null;
  closingIssuesReferences: { number: number; repository: { name: string; owner: { login: string } } }[];
};

async function facts(n: number, subject: string | undefined): Promise<Facts> {
  const repo = await ghOut("repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner");
  const owner = repo.split("/")[0];
  const pr: PrView = JSON.parse(
    await ghOut(
      "pr",
      "view",
      String(n),
      "--json",
      "number,title,state,isDraft,headRefName,headRefOid,baseRefName,headRepository,headRepositoryOwner,closingIssuesReferences",
    ),
  );
  const headRepo = `${pr.headRepositoryOwner?.login ?? owner}/${pr.headRepository?.name ?? repo.split("/")[1]}`;
  const login = await ghOut("api", "user", "--jq", ".login");
  const sha = pr.headRefOid;

  const [behindBy, tip, runs, team, name, email] = await Promise.all([
    ghOut("api", `repos/${repo}/compare/${pr.baseRefName}...${sha}`, "--jq", ".behind_by").then(Number),
    run([gh, "api", `repos/${headRepo}/git/ref/heads/${pr.headRefName}`, "--jq", ".object.sha"]).then((r) => {
      if (r.code === 0) return r.out;
      if (/404|Not Found/.test(r.err + r.out)) return null;
      throw new Error(`gh api git/ref failed: ${r.err || r.out}`);
    }),
    ghOut(
      "api",
      `repos/${repo}/actions/workflows/ci.yml/runs?head_sha=${sha}&per_page=30`,
      "--jq",
      "[.workflow_runs[] | {headSha: .head_sha, status, conclusion, createdAt: .created_at, url: .html_url}]",
    ).then((s) => JSON.parse(s)),
    // Not a member answers 404; any failure counts as "not a maintainer" (fail closed).
    run([gh, "api", `orgs/${owner}/teams/atlas-maintainers/memberships/${login}`, "--jq", ".state"]),
    run(["git", "config", "--get", "user.name"]).then((r) => r.out),
    run(["git", "config", "--get", "user.email"]).then((r) => r.out),
  ]);

  return {
    repo,
    number: pr.number,
    title: pr.title,
    state: pr.state,
    isDraft: pr.isDraft,
    headBranch: pr.headRefName,
    headSha: sha,
    baseRef: pr.baseRefName,
    behindBy,
    branchTip: tip,
    runs,
    linked: pr.closingIssuesReferences.map((i) => `${i.repository.owner.login}/${i.repository.name}#${i.number}`),
    signoff: name || email ? { name, email } : null,
    login,
    maintainer: team.code === 0 && team.out === "active",
    subject,
  };
}

function usage(msg: string): never {
  console.error(`pr:merge: ${msg}\nusage: bun run pr:merge <pr> [--subject <s>] [--dry-run] [--json]`);
  process.exit(2);
}

const args = process.argv.slice(2);
let pr = 0;
let subject: string | undefined;
let dry = false;
let json = false;
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === "--dry-run") dry = true;
  else if (a === "--json") json = true;
  else if (a === "--subject") subject = args[++i] ?? usage("--subject needs a value");
  else if (/^\d+$/.test(a) && !pr) pr = Number(a);
  else usage(`unexpected argument ${a}`);
}
if (!pr) usage("which pull request?");

function finish(out: { pr: number; decision: string; reasons: string[]; subject?: string; body: string; mergedSha?: string }): never {
  if (json) {
    console.log(JSON.stringify(out, null, 2));
  } else {
    console.log(`#${out.pr}: ${out.decision}${dry ? " (dry run)" : ""}`);
    for (const r of out.reasons) console.log(`  - ${r}`);
    if (out.subject) console.log(`\n${out.subject}\n\n${out.body}`);
    if (out.mergedSha) console.log(`\nmerged as ${out.mergedSha}`);
  }
  process.exit(out.decision === "refuse" ? 1 : 0);
}

let f: Facts;
try {
  f = await facts(pr, subject);
} catch (e) {
  finish({ pr, decision: "refuse", reasons: [e instanceof Error ? e.message : String(e)], body: "" });
}

const d = decide(f);
if (d.decision === "refuse" || dry) finish({ pr, ...d });

// --match-head-commit: a push that lands between the decision and the merge makes gh refuse instead of merging
// a commit CI never saw.
const merged = await run([
  gh,
  "pr",
  "merge",
  String(pr),
  "--squash",
  "--admin",
  "--match-head-commit",
  f.headSha,
  "--subject",
  d.subject,
  "--body",
  d.body,
]);
if (merged.code !== 0) finish({ pr, ...d, decision: "refuse", reasons: [`gh pr merge failed: ${merged.err || merged.out}`] });
const mergedSha = await ghOut("pr", "view", String(pr), "--json", "mergeCommit", "-q", ".mergeCommit.oid");
finish({ pr, ...d, mergedSha });
