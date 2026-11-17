// bun scripts/ci-rerun-lost.ts [--dry-run]
// Run by .github/workflows/ci-rerun-lost.yml after a CI run failed (issue #520): reruns the failed jobs, once per
// attempt, when every job that failed in the run was lost to its runner ("The hosted runner lost communication with
// the server"); does nothing otherwise, so a real test failure is never retried into green. The rules are
// decideRerun in ci-lost-runner.ts. Input comes from the environment (never from the command line of a shell):
// GH_REPO, RUN_ID, RUN_ATTEMPT, RUN_CONCLUSION, HEAD_REPO, HEAD_BRANCH, HEAD_SHA. HEAD_BRANCH is a name a fork's
// author chose: it is validated and only ever passed as one argument to gh, never to a shell.
import { MAX_ATTEMPTS, decideRerun, fetchJobs } from "./ci-lost-runner";
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

const verdict =
  conclusion === "failure" && attempt < MAX_ATTEMPTS
    ? decideRerun({ conclusion, attempt, headIsTip: await headIsTip(), jobs: await fetchJobs(repo, runId, attempt) })
    : decideRerun({ conclusion, attempt, headIsTip: true, jobs: [] });

console.log(`run ${runId} attempt ${attempt}: ${verdict.rerun ? "rerun" : "no rerun"}: ${verdict.why}`);
if (verdict.rerun && !dry) {
  await ghOut("api", "--method", "POST", `repos/${repo}/actions/runs/${runId}/rerun-failed-jobs`);
  console.log(`rerun-failed-jobs requested for run ${runId} (attempt ${attempt + 1} follows)`);
}
