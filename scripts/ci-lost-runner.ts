// A CI job lost to its runner: the pure rules and the GitHub reads shared by scripts/ci-rerun-lost.ts (reruns it, issue
// #520) and `bun run pr:merge` (tells a maintainer "runner lost, rerun pending" instead of a bare "failure").
//
// Hosted Windows runners sometimes die mid-step; GitHub notices ~45 min later, ends the job as `failure` and puts one
// annotation on it. That annotation is the only mark of the loss (the job has no log), so it is what is read:
// "The hosted runner lost communication with the server. Anything in your workflow that ...".
import { ghOut } from "./gh";

export const LOST_RUNNER = /hosted runner lost communication with the server/i;
/** A run is rerun while its attempt number is below this: attempts 1 and 2 are rerun, attempt 3 stays red. */
export const MAX_ATTEMPTS = 3;
/** The aggregate job (ci.yml `ci`): red whenever any job it needs is red, so it says nothing of its own. */
export const AGGREGATE = "CI";

const FAILED: Record<string, true> = { failure: true, timed_out: true, startup_failure: true };
/** A job conclusion that is a failure (cancelled, skipped, success and a job still running are not). */
export const failed = (conclusion: string | null): boolean => conclusion !== null && FAILED[conclusion] === true;

export type Job = {
  name: string;
  conclusion: string | null;
  /** The messages of the job's failure-level annotations (only read for a failed job). */
  failures: string[];
};

/** A failed job whose every failure annotation is the lost-runner one (and at least one exists). */
export const lost = (j: Job): boolean => j.failures.length > 0 && j.failures.every((m) => LOST_RUNNER.test(m));

/** The jobs that matter, by why they ended: `lost` runners, `real` failures, `cancelled`. The aggregate job is skipped. */
export function classify(jobs: Job[]): { lost: string[]; real: string[]; cancelled: string[] } {
  const out = { lost: [] as string[], real: [] as string[], cancelled: [] as string[] };
  for (const j of jobs) {
    if (j.name === AGGREGATE) continue;
    if (j.conclusion === "cancelled") out.cancelled.push(j.name);
    else if (failed(j.conclusion)) (lost(j) ? out.lost : out.real).push(j.name);
  }
  return out;
}

/** True when the run is red and every job that failed in it failed by a lost runner, nothing cancelled. */
export function lostOnly(jobs: Job[]): boolean {
  const c = classify(jobs);
  return c.lost.length > 0 && c.real.length === 0 && c.cancelled.length === 0;
}

export type RerunFacts = {
  conclusion: string | null; // the run's
  attempt: number;
  /** The run's head is still the tip of its branch: a run a newer push superseded is not worth a rerun. */
  headIsTip: boolean;
  jobs: Job[];
};

export function decideRerun(f: RerunFacts): { rerun: boolean; why: string } {
  if (f.conclusion !== "failure") return { rerun: false, why: `the run concluded ${f.conclusion}, not failure` };
  if (f.attempt >= MAX_ATTEMPTS) return { rerun: false, why: `attempt ${f.attempt} of ${MAX_ATTEMPTS}: no more reruns` };
  if (!f.headIsTip) return { rerun: false, why: "the head is no longer the tip of its branch: a newer run supersedes this one" };
  const c = classify(f.jobs);
  if (c.real.length) return { rerun: false, why: `a real failure, not a lost runner: ${c.real.join(", ")}` };
  if (c.cancelled.length) return { rerun: false, why: `cancelled: ${c.cancelled.join(", ")}` };
  if (!c.lost.length) return { rerun: false, why: "no job failed by a lost runner" };
  return { rerun: true, why: `lost runner: ${c.lost.join(", ")}` };
}

/** The jobs of one attempt of a run, with the failure annotations of those that failed. */
export async function fetchJobs(repo: string, runId: number, attempt: number): Promise<Job[]> {
  const raw = await ghOut(
    "api",
    `repos/${repo}/actions/runs/${runId}/attempts/${attempt}/jobs`,
    "--paginate",
    "--jq",
    ".jobs[] | {id, name, conclusion}",
  );
  const jobs: { id: number; name: string; conclusion: string | null }[] = raw
    .split("\n")
    .filter(Boolean)
    .map((l) => JSON.parse(l));
  return Promise.all(
    jobs.map(async (j) => {
      const readable = j.name !== AGGREGATE && failed(j.conclusion);
      const failures: string[] = readable
        ? JSON.parse(
            await ghOut(
              "api",
              `repos/${repo}/check-runs/${j.id}/annotations?per_page=100`,
              "--jq",
              '[.[] | select(.annotation_level == "failure") | .message]',
            ),
          )
        : [];
      return { name: j.name, conclusion: j.conclusion, failures };
    }),
  );
}
