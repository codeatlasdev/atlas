// What a rerun after infrastructure hid, made visible (issue #520). ci-rerun-lost.ts reruns a job that ended as `lost`,
// `stalled` or `mirror`; a real intermittent hang in one test would pass on that rerun and merge unseen. So each such
// rerun leaves a comment on the pull request (the one the commit belongs to), the label `ci: rerun-after-stall`, and one
// line in issue #520, naming the job, the kind and what was running when the job went quiet, so the same test showing up
// across pull requests is a pattern anyone can read.
//
// Where "what was running" comes from: nextest's CI profile prints no START lines (a real log has `Starting N tests` and
// one PASS line per test), and the log of a lost runner is gone (BlobNotFound). What a lost runner does leave is the
// check run of the Windows watchdog (scripts/ci-watchdog.ps1), which lists the tests running (the test processes of the
// process table) every 30 s, so that is read first; the job's log tail is added when the log exists.
import { ghOut } from "./gh";
import type { Infra } from "./ci-lost-runner";

export const LABEL = "ci: rerun-after-stall";
/** The issue that collects one line per occurrence. */
export const TRACKING_ISSUE = 520;
export const TELEMETRY_CHECK = "Windows test telemetry (#520)";

export type RunningTest = { name: string; secs: number };
export type Sample = { at: string; tests: RunningTest[]; progress: string | null };

/** The last sample of the watchdog's check-run text: the time, the tests running then ("pkg::bin test 14s; ...") and nextest's count. */
export function lastSample(text: string): Sample | null {
  let sample: Sample | null = null;
  let progress: string | null = null;
  for (const line of text.split("\n")) {
    const p = line.match(/^(\d\d:\d\d:\d\d) \[watchdog\] nextest: (\d+\/\d+|none finished yet)/);
    if (p) progress = p[2] === "none finished yet" ? "no test finished yet" : p[2];
    const m = line.match(/^(\d\d:\d\d:\d\d) \[watchdog\] tests running \((\d+)\): (.*)$/);
    if (!m) continue;
    const tests = m[2] === "0" ? [] : m[3].split("; ").map(parseRunning);
    sample = { at: m[1], tests, progress };
  }
  return sample;
}

function parseRunning(entry: string): RunningTest {
  const m = entry.match(/^(.*) (\d+)s$/);
  return m ? { name: m[1], secs: Number(m[2]) } : { name: entry, secs: 0 };
}

/** The last lines a job log printed before GitHub cancelled it (before the first `##[error]`, else the end), timestamps and colors off. */
export function logTail(log: string, n = 3): string[] {
  const lines = log
    .split("\n")
    .map((l) => l.replace(/\u001b\[[0-9;]*m/g, "").replace(/^\S+Z /, "").trimEnd())
    .filter((l) => l.trim() !== "");
  const end = lines.findIndex((l) => l.startsWith("##[error]"));
  return lines.slice(0, end === -1 ? undefined : end).slice(-n);
}

export type Stalled = {
  job: string;
  url: string;
  kind: Infra;
  /** The step that was running when the job ended (null when the API names none). */
  step: string | null;
  sample: Sample | null;
  sampleUrl: string | null;
  tail: string[];
};

const KIND_WORDS: Record<Infra, string> = {
  lost: "the runner was lost",
  stalled: "the runner stalled until the job's time limit",
  mirror: "the apt mirror stalled",
  unavailable: "no runner was available",
};

const tests = (s: Sample | null): string =>
  s === null ? "none known" : s.tests.length === 0 ? "none running" : s.tests.map((t) => `${t.name} (${t.secs}s)`).join(", ");

/** The comment on the pull request: one block per job, what it was doing, and how to read a repeat. */
export function renderComment(jobs: Stalled[], run: { url: string; attempt: number; max: number }): string {
  const out = [`**CI rerun after the infrastructure stalled** ([run ${run.url.split("/").pop()}](${run.url}), attempt ${run.attempt}, rerun requested; at most ${run.max} attempts)`, ""];
  for (const j of jobs) {
    out.push(`- [${j.job}](${j.url}): ${KIND_WORDS[j.kind]} (\`${j.kind}\`)`);
    if (j.step) out.push(`  - step when it ended: ${j.step}`);
    if (j.sample) {
      out.push(`  - tests running at the last sample (${j.sample.at} UTC${j.sample.progress ? `, nextest ${j.sample.progress}` : ""}${j.sampleUrl ? `, [watchdog](${j.sampleUrl})` : ""}): ${tests(j.sample)}`);
    }
    if (j.tail.length) out.push(`  - last log lines: ${j.tail.map((l) => `\`${l.slice(0, 120)}\``).join(" / ")}`);
  }
  out.push("", `A runner that really froze says nothing of the commit. A test that shows up here again, on this or another pull request, is a hang, not the platform: see #${TRACKING_ISSUE}, which collects one line per occurrence.`);
  return out.join("\n");
}

/** One line of the tracking issue per job: where, when, what kind, what was running. */
export function issueLine(j: Stalled, ctx: { run: string; attempt: number; subject: string; date: string }): string {
  return `- ${ctx.date} ${ctx.subject}, run ${ctx.run} attempt ${ctx.attempt}: [${j.job}](${j.url}) \`${j.kind}\`${j.step ? `, step ${j.step}` : ""}, tests running: ${tests(j.sample)}`;
}

type ApiStep = { name: string; status: string; conclusion: string | null };

/** The step a job was in when it ended: the one still running, else the last that did not finish or was cancelled. */
export function endingStep(steps: ApiStep[]): string | null {
  const running = steps.filter((s) => s.status === "in_progress" || s.conclusion === "cancelled");
  return running.length ? running[0].name : null;
}

/** What is known about one job that was rerun: the API's step list, the watchdog's last sample, the log tail when it exists. */
export async function describeJob(repo: string, sha: string, job: { id: number; name: string; kind: Infra }): Promise<Stalled> {
  const j = JSON.parse(await ghOut("api", `repos/${repo}/actions/jobs/${job.id}`, "--jq", "{html_url, started_at, completed_at, steps: [.steps[] | {name, status, conclusion}]}"));
  let sample: Sample | null = null;
  let sampleUrl: string | null = null;
  if (/^Rust \(Windows\b/.test(job.name)) {
    // The check run of this attempt: the one that started while this job ran.
    const runs: { name: string; html_url: string; started_at: string; output: { text: string | null } }[] = JSON.parse(
      await ghOut("api", `repos/${repo}/commits/${sha}/check-runs?per_page=100`, "--jq", `[.check_runs[] | select(.name == "${TELEMETRY_CHECK}") | {name, html_url, started_at, output: {text: .output.text}}]`),
    );
    const mine = runs.filter((r) => r.started_at >= j.started_at && (!j.completed_at || r.started_at <= j.completed_at)).sort((a, b) => b.started_at.localeCompare(a.started_at))[0];
    if (mine?.output.text) {
      sample = lastSample(mine.output.text);
      sampleUrl = mine.html_url;
    }
  }
  let tail: string[] = [];
  try {
    tail = logTail(await ghOut("api", `repos/${repo}/actions/jobs/${job.id}/logs`, "--allow-escape-sequences"));
  } catch {
    // The log of a lost runner is not kept (BlobNotFound): nothing to add.
  }
  return { job: job.name, url: j.html_url, kind: job.kind, step: endingStep(j.steps), sample, sampleUrl, tail };
}

/** The pull requests a commit belongs to (a squash-merged commit on main: the one it came from). */
export async function pullsOf(repo: string, sha: string): Promise<number[]> {
  const out = await ghOut("api", `repos/${repo}/commits/${sha}/pulls`, "--jq", ".[].number");
  return out.split("\n").filter(Boolean).map(Number);
}

/** Comment and label every pull request, and add the lines to the tracking issue. Returns what failed (the rerun is already requested). */
export async function publish(repo: string, prs: number[], comment: string, lines: string[], dry: boolean): Promise<string[]> {
  const failures: string[] = [];
  const attempt = async (what: string, ...args: string[]) => {
    try {
      await ghOut("api", "--method", "POST", ...args);
    } catch (e) {
      failures.push(`${what}: ${e instanceof Error ? e.message : String(e)}`);
    }
  };
  if (dry) {
    console.log(`--dry-run: comment on ${prs.length ? prs.map((n) => `#${n}`).join(", ") : "no pull request"} and on #${TRACKING_ISSUE}:\n${comment}\n${lines.join("\n")}`);
    return failures;
  }
  for (const n of prs) {
    await attempt(`comment on #${n}`, `repos/${repo}/issues/${n}/comments`, "-f", `body=${comment}`);
    await attempt(`label #${n}`, `repos/${repo}/issues/${n}/labels`, "-f", `labels[]=${LABEL}`);
  }
  await attempt(`line on #${TRACKING_ISSUE}`, `repos/${repo}/issues/${TRACKING_ISSUE}/comments`, "-f", `body=${lines.join("\n")}`);
  return failures;
}
