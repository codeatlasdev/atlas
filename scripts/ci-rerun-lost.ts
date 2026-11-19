// bun scripts/ci-rerun-lost.ts [--dry-run]
// Run by .github/workflows/ci-rerun-lost.yml after a CI run failed (issue #520): reruns the failed jobs, once per
// attempt, when every job that failed in the run ended by GitHub's infrastructure ("The hosted runner lost communication
// with the server", or "The job was not acquired by Runner of type hosted even after multiple attempts"); does nothing
// otherwise, so a real test failure is never retried into green. The rules are
// decideRerun in ci-lost-runner.ts. Input comes from the environment (never from the command line of a shell):
// GH_REPO, RUN_ID, RUN_ATTEMPT, RUN_CONCLUSION, HEAD_REPO, HEAD_BRANCH, HEAD_SHA. HEAD_BRANCH is a name a fork's
// author chose: it is validated and only ever passed as one argument to gh, never to a shell. After a rerun of a lost,
// stalled or mirror job it also says so (ci-stall-report.ts): a comment and a label on the pull request, a line in #520.
import { AGGREGATE, MAX_ATTEMPTS, decideRerun, failed, fetchJobs, infraKinds } from "./ci-lost-runner";
import { describeJob, issueLine, publish, pullsOf, renderComment } from "./ci-stall-report";
import { ghOut } from "./gh";

const dry = process.argv.includes("--dry-run");

function need(name: string, pattern: RegExp): string {
  const v = process.env[name] ?? "";
  if (!pattern.test(v)) {
    console.error(`ci-rerun-lost: ${name} is missing or malformed`);
    process.exit(2);
  }
  return v;
}

const repo = need("GH_REPO", /^[\w.-]+\/[\w.-]+$/);
const runId = Number(need("RUN_ID", /^\d+$/));
const attempt = Number(need("RUN_ATTEMPT", /^\d+$/));
const conclusion = process.env.RUN_CONCLUSION ?? "";
const headRepo = need("HEAD_REPO", /^[\w.-]+\/[\w.-]+$/);
const headBranch = process.env.HEAD_BRANCH ?? "";
const headSha = need("HEAD_SHA", /^[0-9a-f]{40}$/);

// A branch the author deleted, or one a newer push moved on, has no use for a rerun of this run.
async function headIsTip(): Promise<boolean> {
  if (!/^[\w./-]+$/.test(headBranch)) return false;
  try {
    return (await ghOut("api", `repos/${headRepo}/git/ref/heads/${headBranch}`, "--jq", ".object.sha")) === headSha;
  } catch {
    return false;
  }
}

const jobs = conclusion === "failure" && attempt < MAX_ATTEMPTS ? await fetchJobs(repo, runId, attempt) : [];
const verdict = decideRerun({ conclusion, attempt, headIsTip: conclusion === "failure" && attempt < MAX_ATTEMPTS ? await headIsTip() : true, jobs });

console.log(`run ${runId} attempt ${attempt}: ${verdict.rerun ? "rerun" : "no rerun"}: ${verdict.why}`);
if (verdict.rerun && !dry) {
  await ghOut("api", "--method", "POST", `repos/${repo}/actions/runs/${runId}/rerun-failed-jobs`);
  console.log(`rerun-failed-jobs requested for run ${runId} (attempt ${attempt + 1} follows)`);
}

// A rerun must not hide a hang (issue #520): say which job, which kind and what was running, on the pull request and in
// the tracking issue. The rerun is already requested; a failure here is reported at the end, never a reason to skip it.
if (verdict.rerun) {
  const hits = jobs.flatMap((j) => {
    const kind = infraKinds(j).find((k) => k !== "unavailable");
    return kind && j.id !== undefined && j.name !== AGGREGATE && (j.conclusion === "cancelled" || failed(j.conclusion)) ? [{ id: j.id, name: j.name, kind }] : [];
  });
  if (hits.length) {
    const failures: string[] = [];
    try {
      const prs = await pullsOf(repo, headSha).catch(() => [] as number[]);
      const run = `https://github.com/${repo}/actions/runs/${runId}`;
      const stalled = await Promise.all(hits.map((h) => describeJob(repo, headSha, h)));
      const subject = prs.length ? prs.map((n) => `#${n}`).join(", ") : `${headBranch}@${headSha.slice(0, 8)}`;
      const lines = stalled.map((j) => issueLine(j, { run: String(runId), attempt, subject, date: new Date().toISOString().slice(0, 10) }));
      failures.push(...(await publish(repo, prs, renderComment(stalled, { url: run, attempt, max: MAX_ATTEMPTS }), lines, dry)));
    } catch (e) {
      failures.push(e instanceof Error ? e.message : String(e));
    }
    for (const f of failures) console.error(`ci-rerun-lost: could not report the rerun: ${f}`);
    if (failures.length) process.exitCode = 1;
  }
}
