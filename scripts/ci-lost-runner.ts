// A CI job that failed by GitHub's infrastructure, not by the commit: the pure rules and the GitHub reads shared by
// scripts/ci-rerun-lost.ts (reruns it, issue #520) and `bun run pr:merge` (tells a maintainer "rerun pending" instead
// of a bare "failure"). Each kind is one annotation GitHub puts on the job, and the only mark of it (no log exists):
//   lost         "The hosted runner lost communication with the server. Anything in your workflow that ..."
//                (a hosted Windows runner died mid-step; GitHub notices ~45 min later and ends the job as a failure)
//   unavailable  "The job was not acquired by Runner of type hosted even after multiple attempts"
//                (no hosted runner took the job, e.g. macOS arm64 capacity; the job ends cancelled, not failure)
//   stalled      "The job has exceeded the maximum execution time of 30m0s" + "The operation was canceled.", on the
//                Windows Rust job only: it has a timeout on every step, so only its own limit can end it when the VM
//                froze and the runner stopped running steps (GitHub would only notice ~45 min later, as `lost`)
import { ghOut } from "./gh";

export type Infra = "lost" | "unavailable" | "stalled";
const LIMIT = /the job has exceeded the maximum execution time/i;
const INFRA: [Infra, RegExp][] = [
  ["lost", /hosted runner lost communication with the server/i],
  ["unavailable", /job was not acquired by runner of type hosted even after multiple attempts/i],
  ["stalled", LIMIT],
];
const WORDS: Record<Infra, string> = { lost: "runner lost", unavailable: "no runner was available", stalled: "runner stalled until the job's time limit" };
/** The jobs whose time limit is an infrastructure signal (ci.yml `rust-windows`: a timeout on each step, so it cannot be a hang). */
const STEP_TIMEOUT_JOBS = /^Rust \(Windows\b/;
/** What GitHub adds to every job it ends at its limit. */
const CANCELED = /^the operation was canceled\.?$/i;

/** A run is rerun while its attempt number is below this: attempts 1 and 2 are rerun, attempt 3 stays red. */
export const MAX_ATTEMPTS = 3;
/** The aggregate job (ci.yml `ci`): red whenever any job it needs is red, so it says nothing of its own. */
export const AGGREGATE = "CI";

const FAILED: Record<string, true> = { failure: true, timed_out: true, startup_failure: true };
/** A job conclusion that is a failure (skipped, success and a job still running are not; cancelled is its own case). */
export const failed = (conclusion: string | null): boolean => conclusion !== null && FAILED[conclusion] === true;

export type Job = {
  name: string;
  conclusion: string | null;
  /** The messages of the job's failure-level annotations (only read for a failed or cancelled job). */
  failures: string[];
};

/** The infrastructure kinds that ended a job: every failure annotation is one of them (and at least one exists), else none. */
export function infraKinds(j: Job): Infra[] {
  const kinds: Infra[] = [];
  const hitLimit = j.failures.some((m) => LIMIT.test(m));
  for (const m of j.failures) {
    if (hitLimit && CANCELED.test(m)) continue;
    const hit = INFRA.find(([k, re]) => re.test(m) && (k !== "stalled" || STEP_TIMEOUT_JOBS.test(j.name)));
    if (!hit) return [];
    if (!kinds.includes(hit[0])) kinds.push(hit[0]);
  }
  return kinds;
}

/** "runner lost", "no runner was available" or both, for the kinds seen. */
export const infraWords = (kinds: Infra[]): string => kinds.map((k) => WORDS[k]).join(" and ");

/**
 * The jobs that matter, by why they ended: `infra` (names) with the `kinds` seen, `real` failures, `cancelled` (by
 * hand or by a newer push: no infrastructure annotation). The aggregate job is skipped.
 */
export function classify(jobs: Job[]): { infra: string[]; kinds: Infra[]; real: string[]; cancelled: string[] } {
  const out = { infra: [] as string[], kinds: [] as Infra[], real: [] as string[], cancelled: [] as string[] };
  for (const j of jobs) {
    if (j.name === AGGREGATE || !(j.conclusion === "cancelled" || failed(j.conclusion))) continue;
    const kinds = infraKinds(j);
    if (kinds.length) {
      out.infra.push(j.name);
      for (const k of kinds) if (!out.kinds.includes(k)) out.kinds.push(k);
    } else (j.conclusion === "cancelled" ? out.cancelled : out.real).push(j.name);
  }
  return out;
}

/** The kinds when the run is red and every job that failed in it ended by infrastructure, nothing else; else null. */
export function infraOnly(jobs: Job[]): Infra[] | null {
  const c = classify(jobs);
  return c.infra.length > 0 && c.real.length === 0 && c.cancelled.length === 0 ? c.kinds : null;
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
  if (c.real.length) return { rerun: false, why: `a real failure, not infrastructure: ${c.real.join(", ")}` };
  if (c.cancelled.length) return { rerun: false, why: `cancelled: ${c.cancelled.join(", ")}` };
  if (!c.infra.length) return { rerun: false, why: "no job ended by infrastructure (a lost or unavailable runner)" };
  return { rerun: true, why: `${infraWords(c.kinds)}: ${c.infra.join(", ")}` };
}

/** The jobs of one attempt of a run, with the failure annotations of those that failed or were cancelled. */
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
      const readable = j.name !== AGGREGATE && (j.conclusion === "cancelled" || failed(j.conclusion));
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
