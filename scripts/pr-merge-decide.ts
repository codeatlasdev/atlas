// The decision behind `bun run pr:merge`: pure, so every refusal is a table row in pr-merge.test.ts. Each reason
// carries the fix, because the reader is usually an agent that has to act on it without asking.
import { MAX_ATTEMPTS } from "./ci-lost-runner";
import { type Signoff, squashBody } from "./pr-closes";

export type Run = {
  headSha: string;
  status: string;
  conclusion: string | null;
  createdAt: string;
  url: string;
  /** The run id and attempt, and whether every job that failed in it was lost to its runner (ci-lost-runner.ts). */
  id?: number;
  attempt?: number;
  lostOnly?: boolean;
};

export type Facts = {
  repo: string; // owner/repo
  number: number;
  title: string;
  state: string; // OPEN | CLOSED | MERGED
  isDraft: boolean;
  headBranch: string;
  headSha: string; // the pull request's headRefOid
  baseRef: string;
  behindBy: number; // commits of the base the head does not have
  branchTip: string | null; // the tip of the head branch on its repository right now (null: no such branch)
  runs: Run[]; // ci.yml runs found for this head sha
  linked: string[]; // closingIssuesReferences as owner/repo#N
  signoff: Signoff | null; // git config user.name / user.email
  login: string; // gh api user
  maintainer: boolean; // active member of atlas-maintainers
  subject?: string; // --subject
};

export type Decision = { decision: "merge" | "refuse"; reasons: string[]; subject: string; body: string };

// The same grammar as scripts/check-pr-title.sh (the test runs both over one table so they cannot drift).
const TYPES = "feat|fix|perf|refactor|docs|test|build|ci|chore|revert|style";
const CONVENTIONAL = new RegExp(`^(${TYPES})(\\([a-z0-9][a-z0-9._/-]*\\))?!?: [^\\s].*$`);
export const conventional = (title: string): boolean => CONVENTIONAL.test(title) || /^Revert ".+"$/.test(title);

// The same grammar as `placeholder` in scripts/check-dco.sh: an e-mail nobody can be reached at, or a template name.
const PLACEHOLDER_EMAIL =
  /@(([^@]+\.)?example\.(com|org|net)|.+\.(example|test|invalid|localhost|local|localdomain)|localhost)$/;
const PLACEHOLDER_NAME = /^(your[ _-]?name|first[ _-]?user|test[ _-]?user|example[ _-]?user|user[ _-]?name)$/;
export const placeholder = (s: Signoff): boolean =>
  PLACEHOLDER_EMAIL.test(s.email.toLowerCase()) || PLACEHOLDER_NAME.test(s.name.toLowerCase());

/** The newest ci.yml run of exactly this commit. A run of another sha, or the status rollup, says nothing about it. */
export function runFor(runs: Run[], sha: string): Run | null {
  const mine = runs.filter((r) => r.headSha === sha).sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  return mine[0] ?? null;
}

export function decide(f: Facts): Decision {
  const reasons: string[] = [];
  const short = f.headSha.slice(0, 8);

  if (f.state !== "OPEN") reasons.push(`#${f.number} is ${f.state}, not OPEN: nothing to merge`);
  if (f.isDraft) reasons.push(`#${f.number} is a draft: finish it, then \`gh pr ready ${f.number}\``);

  if (f.behindBy > 0) {
    reasons.push(
      `branch is ${f.behindBy} commit(s) behind ${f.baseRef}: merge ${f.baseRef} into it (\`git merge origin/${f.baseRef}\`), push, and wait for CI on the new head`,
    );
  }

  if (f.branchTip === null) {
    reasons.push(`branch ${f.headBranch} no longer exists on its repository, so the head ${short} cannot be confirmed`);
  } else if (f.branchTip !== f.headSha) {
    reasons.push(
      `PR head ${short} is not the tip of ${f.headBranch} (${f.branchTip.slice(0, 8)}): a push has not reached the pull request yet; wait and rerun`,
    );
  }

  const run = runFor(f.runs, f.headSha);
  if (!run) {
    reasons.push(
      `no CI run for head ${short} (a run of an older commit does not count): push it, or \`gh workflow run CI --ref ${f.headBranch}\`, then rerun`,
    );
  } else if (run.status !== "completed") {
    reasons.push(`CI for head ${short} is ${run.status}: wait for it to complete, then rerun (${run.url})`);
  } else if (run.conclusion !== "success") {
    const attempt = run.attempt ?? 1;
    if (run.lostOnly && attempt < MAX_ATTEMPTS) {
      reasons.push(
        `CI for head ${short} concluded ${run.conclusion} only because a runner was lost: runner lost, rerun pending (attempt ${attempt} of ${MAX_ATTEMPTS}, ci-rerun-lost.yml): wait for the new attempt, then rerun (${run.url})`,
      );
    } else if (run.lostOnly) {
      reasons.push(
        `CI for head ${short} concluded ${run.conclusion} only because a runner was lost, on every attempt (${attempt} of ${MAX_ATTEMPTS}): no rerun is pending; \`gh run rerun --failed ${run.id ?? run.url}\` once, then rerun this (${run.url})`,
      );
    } else {
      reasons.push(`CI for head ${short} concluded ${run.conclusion}: fix it and push (\`gh run view --log-failed ${run.url}\`)`);
    }
  }

  if (!conventional(f.title)) {
    reasons.push(
      `title is not a conventional commit subject: "${f.title}" (use "<type>(<scope>): <summary>", \`gh pr edit ${f.number} --title …\`)`,
    );
  }

  let signoff = f.signoff;
  if (!signoff || !signoff.name.trim() || !signoff.email.trim()) {
    signoff = null;
    reasons.push("git config user.name / user.email are not set: the squash needs your Signed-off-by (`git config --global user.name …`)");
  } else if (placeholder(signoff)) {
    reasons.push(
      `git identity is a placeholder (${signoff.name} <${signoff.email}>): set your real user.name / user.email, nobody can be credited otherwise`,
    );
    signoff = null;
  }

  if (!f.maintainer) {
    reasons.push(
      `${f.login || "this gh login"} is not an active member of atlas-maintainers: only a maintainer merges (run \`gh auth status\` to see which account is in use)`,
    );
  }

  const title = f.subject?.trim() || f.title.trim();
  const suffix = ` (#${f.number})`;
  const subject = title.endsWith(suffix.trim()) ? title : `${title}${suffix}`;
  if (f.subject !== undefined && !conventional(subject)) {
    reasons.push(`--subject is not a conventional commit subject: "${subject}"`);
  }

  return {
    decision: reasons.length ? "refuse" : "merge",
    reasons,
    subject,
    body: squashBody(f.linked, f.repo, signoff),
  };
}
