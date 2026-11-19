// The rules of a rerun for a job that ended by GitHub's infrastructure (a lost runner, no runner available;
// decideRerun, a table), and scripts/ci-rerun-lost.ts end to end against a fake `gh` (reads, and the one POST it may make).
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Job, type RerunFacts, classify, decideRerun, infraOnly, infraWords } from "./ci-lost-runner";

const LOST_MSG =
  "The hosted runner lost communication with the server. Anything in your workflow that terminates the runner process, starves it for CPU/Memory, or blocks its network access can cause this error.";
const win = (o: Partial<Job> = {}): Job => ({ name: "Rust (Windows, full workspace)", conclusion: "failure", failures: [LOST_MSG], ...o });
const aggregate: Job = { name: "CI", conclusion: "failure", failures: [] };
const green: Job = { name: "Rust (Linux)", conclusion: "success", failures: [] };
const realFailure: Job = { name: "Rust (macOS)", conclusion: "failure", failures: ["Process completed with exit code 100."] };
// Seen on #547 (run 44121673101): macOS arm64 capacity. The job ends `cancelled`, with this one failure annotation.
const NOT_ACQUIRED_MSG = "The job was not acquired by Runner of type hosted even after multiple attempts";
const mac = (o: Partial<Job> = {}): Job => ({ name: "Rust (macOS)", conclusion: "cancelled", failures: [NOT_ACQUIRED_MSG], ...o });
// What a job that reached its `timeout-minutes` shows (seen on run 44413383839, "Repo scripts (ubuntu-latest)", 15 min): it ends
// `cancelled` with these two failure annotations. The Windows job (ci.yml `rust-windows`) puts a timeout on each step, so
// when only the job's own limit fires the runner stopped running the steps (#520: a frozen VM, no step timeout ever fires).
const TIMEOUT_MSGS = ["The job has exceeded the maximum execution time of 30m0s", "The operation was canceled."];
const stalled = (o: Partial<Job> = {}): Job => ({ name: "Rust (Windows, full workspace)", conclusion: "cancelled", failures: TIMEOUT_MSGS, ...o });
// What ci-apt.sh leaves when its mirror stalled twice (run 44425249926, "Rust (Linux)"): its own annotation, and GitHub's for the exit code.
const MIRROR_MSGS = ["ci-apt: the apt mirror is too slow or down (the download did not finish in two tries of 150s); rerun the job", "Process completed with exit code 1."];
const mirror = (o: Partial<Job> = {}): Job => ({ name: "Rust (Linux)", conclusion: "failure", failures: MIRROR_MSGS, ...o });
const base: RerunFacts = { conclusion: "failure", attempt: 1, headIsTip: true, jobs: [green, win(), aggregate] };

describe("decideRerun", () => {
  test.each<[string, Partial<RerunFacts>, boolean, RegExp]>([
    ["only a lost runner (the aggregate job is red too) -> rerun", {}, true, /runner lost: Rust \(Windows/],
    ["lost runner on attempt 2 -> rerun again", { attempt: 2 }, true, /runner lost/],
    ["two lost runners -> rerun", { jobs: [win(), win({ name: "Installer (Windows)" }), aggregate] }, true, /Rust \(Windows.*Installer/],
    ["a job that timed out with the lost annotation -> rerun", { jobs: [win({ conclusion: "timed_out" }), aggregate] }, true, /runner lost/],
    ["only 'not acquired' (the job ended cancelled) -> rerun", { jobs: [mac(), green, aggregate] }, true, /no runner was available: Rust \(macOS\)/],
    ["'not acquired' on a job that ended failure -> rerun", { jobs: [mac({ conclusion: "failure" }), aggregate] }, true, /no runner was available/],
    ["lost + not acquired -> rerun, both named", { jobs: [win(), mac(), aggregate] }, true, /runner lost and no runner was available: Rust \(Windows.*Rust \(macOS\)/],
    ["not acquired + a real failure -> none", { jobs: [mac(), win({ name: "Rust (Linux)", failures: ["Process completed with exit code 1."] }), aggregate] }, false, /real failure.*Rust \(Linux\)/],
    ["lost + a real failure -> none", { jobs: [win(), realFailure, aggregate] }, false, /real failure.*Rust \(macOS\)/],
    ["a real failure only -> none", { jobs: [realFailure, aggregate] }, false, /real failure/],
    ["attempt 3 -> none", { attempt: 3 }, false, /attempt 3 of 3/],
    ["attempt 3 with 'not acquired' -> none", { attempt: 3, jobs: [mac(), aggregate] }, false, /attempt 3 of 3/],
    ["the run was cancelled -> none", { conclusion: "cancelled" }, false, /concluded cancelled/],
    ["the run succeeded -> none", { conclusion: "success", jobs: [green] }, false, /concluded success/],
    ["a job cancelled by hand (no annotation) next to the lost one -> none", { jobs: [win(), { name: "Site build", conclusion: "cancelled", failures: [] }, aggregate] }, false, /cancelled: Site build/],
    ["a job cancelled with an unrelated annotation -> none", { jobs: [mac({ failures: ["The operation was canceled."] }), aggregate] }, false, /cancelled: Rust \(macOS\)/],
    ["the Windows job ended at its own time limit (its step timeouts never fired) -> rerun", { jobs: [stalled(), aggregate] }, true, /runner stalled until the job's time limit: Rust \(Windows/],
    ["another job that ran into its time limit is a hang or a slow mirror, not the platform -> none", { jobs: [stalled({ name: "Repo scripts (ubuntu-latest)" }), aggregate] }, false, /cancelled: Repo scripts/],
    ["the Windows job timed out next to a real failure -> none", { jobs: [stalled(), realFailure, aggregate] }, false, /real failure/],
    ["a time limit message next to a failure of the job's own -> none", { jobs: [stalled({ failures: [...TIMEOUT_MSGS, "Process completed with exit code 1."] }), aggregate] }, false, /cancelled|real failure/],
    ["a job whose apt mirror stalled twice (any job, its exit code annotation aside) -> rerun", { jobs: [mirror(), aggregate] }, true, /apt mirror too slow: Rust \(Linux\)/],
    ["the same job with a failure of its own as well -> none", { jobs: [mirror({ failures: [...MIRROR_MSGS, "error: could not compile `atlas-core`"] }), aggregate] }, false, /real failure/],
    ["an exit code annotation alone is a real failure -> none", { jobs: [mirror({ failures: [MIRROR_MSGS[1]] }), aggregate] }, false, /real failure/],
    ["a stalled mirror next to a real failure elsewhere -> none", { jobs: [mirror(), realFailure, aggregate] }, false, /real failure.*Rust \(macOS\)/],
    ["a newer push superseded the run -> none", { headIsTip: false }, false, /no longer the tip/],
    ["red with no infrastructure annotation (a bare failure) -> none", { jobs: [{ name: "Site build", conclusion: "failure", failures: [] }, aggregate] }, false, /real failure/],
    ["only the aggregate job is red -> none", { jobs: [green, aggregate] }, false, /no job ended by infrastructure/],
    ["an infrastructure annotation next to a real failure message in the same job -> none", { jobs: [win({ failures: [LOST_MSG, "Process completed with exit code 1."] }), aggregate] }, false, /real failure/],
  ])("%s", (_n, patch, rerun, why) => {
    const v = decideRerun({ ...base, ...patch });
    expect(v.rerun).toBe(rerun);
    expect(v.why).toMatch(why);
  });
});

test("classify skips the aggregate job and jobs that did not end badly, and lists the kinds seen", () => {
  expect(classify([green, win(), mac({ name: "Rust (macOS)" }), realFailure, aggregate, { name: "Mobile", conclusion: null, failures: [] }])).toEqual({
    infra: ["Rust (Windows, full workspace)", "Rust (macOS)"],
    kinds: ["lost", "unavailable"],
    real: ["Rust (macOS)"],
    cancelled: [],
  });
});

test("infraOnly is the same rule without attempt or head: attempt 3 is still 'only infrastructure'", () => {
  expect(infraOnly([green, win(), aggregate])).toEqual(["lost"]);
  expect(infraOnly([mac(), aggregate])).toEqual(["unavailable"]);
  expect(infraOnly([win(), mac(), aggregate])).toEqual(["lost", "unavailable"]);
  expect(infraOnly([win(), realFailure])).toBeNull();
  expect(infraOnly([green, aggregate])).toBeNull();
  expect(infraWords(["lost"])).toBe("runner lost");
  expect(infraWords(["unavailable"])).toBe("no runner was available");
});

// The CLI against a fake gh: jobs 11 (the lost Windows job), 12 (green), 13 (the aggregate), 14 (a real failure).
const FAKE_GH = `
import { appendFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
const w = JSON.parse(readFileSync(join(import.meta.dir, "world.json"), "utf8"));
const a = process.argv.slice(2);
const out = (s) => { console.log(typeof s === "string" ? s : JSON.stringify(s)); process.exit(0); };
if (a[0] === "api" && a.includes("--method")) { appendFileSync(join(import.meta.dir, "calls.log"), a.join(" ") + "\\n"); out("{}"); }
const path = a[1];
if (path.includes("/git/ref/heads/")) out(w.tip);
if (path.endsWith("/jobs")) out(w.jobs.map((j) => JSON.stringify({ id: j.id, name: j.name, conclusion: j.conclusion })).join("\\n"));
const m = path.match(/check-runs\\/(\\d+)\\/annotations/);
if (m) out(w.jobs.find((j) => String(j.id) === m[1]).failures);
console.error("unexpected gh " + a.join(" ")); process.exit(1);
`;

const HEAD = "a".repeat(40);
const jobs = (...ids: number[]) =>
  [
    { id: 11, ...win() },
    { id: 12, ...green },
    { id: 13, ...aggregate },
    { id: 14, ...realFailure },
    { id: 15, ...mac({ name: "Rust (macOS arm64)" }) },
  ].filter((j) => ids.includes(j.id));

function cli(w: { tip?: string; jobs: object[] }, env: Record<string, string> = {}, args: string[] = []) {
  const dir = mkdtempSync(join(tmpdir(), "rerun-lost-"));
  writeFileSync(join(dir, "gh.mjs"), FAKE_GH);
  writeFileSync(join(dir, "world.json"), JSON.stringify({ tip: w.tip ?? HEAD, jobs: w.jobs }));
  const p = Bun.spawnSync([process.execPath, join(import.meta.dir, "ci-rerun-lost.ts"), ...args], {
    env: {
      ...process.env,
      GH: join(dir, "gh.mjs"),
      GH_REPO: "codeatlasdev/atlas",
      RUN_ID: "4242",
      RUN_ATTEMPT: "1",
      RUN_CONCLUSION: "failure",
      HEAD_REPO: "codeatlasdev/atlas",
      HEAD_BRANCH: "ci/branch",
      HEAD_SHA: HEAD,
      ...env,
    },
  });
  const log = join(dir, "calls.log");
  return { code: p.exitCode, out: p.stdout.toString(), err: p.stderr.toString(), calls: existsSync(log) ? readFileSync(log, "utf8").trim().split("\n") : [] };
}

describe("ci-rerun-lost.ts", () => {
  test("only a lost runner: POSTs rerun-failed-jobs for that run, once", () => {
    const r = cli({ jobs: jobs(11, 12, 13) });
    expect(r.code).toBe(0);
    expect(r.out).toMatch(/run 4242 attempt 1: rerun: runner lost: Rust \(Windows/);
    expect(r.calls).toEqual(["api --method POST repos/codeatlasdev/atlas/actions/runs/4242/rerun-failed-jobs"]);
  });

  test("only 'not acquired' (job ended cancelled): POSTs the rerun too", () => {
    const r = cli({ jobs: jobs(15, 12, 13) });
    expect(r.out).toMatch(/attempt 1: rerun: no runner was available: Rust \(macOS arm64\)/);
    expect(r.calls).toEqual(["api --method POST repos/codeatlasdev/atlas/actions/runs/4242/rerun-failed-jobs"]);
  });

  test("'not acquired' next to a real failure: no POST", () => {
    const r = cli({ jobs: jobs(15, 14, 13) });
    expect(r.out).toMatch(/no rerun: a real failure, not infrastructure: Rust \(macOS\)/);
    expect(r.calls).toEqual([]);
  });

  test("--dry-run says so and POSTs nothing", () => {
    const r = cli({ jobs: jobs(11, 12, 13) }, {}, ["--dry-run"]);
    expect(r.out).toMatch(/: rerun: runner lost/);
    expect(r.calls).toEqual([]);
  });

  test("a real failure in the same run: no POST", () => {
    const r = cli({ jobs: jobs(11, 12, 13, 14) });
    expect(r.code).toBe(0);
    expect(r.out).toMatch(/no rerun: a real failure.*Rust \(macOS\)/);
    expect(r.calls).toEqual([]);
  });

  test("attempt 3: no POST, and gh is not even asked for the jobs", () => {
    const r = cli({ jobs: jobs(11, 12, 13) }, { RUN_ATTEMPT: "3" });
    expect(r.out).toMatch(/attempt 3 of 3/);
    expect(r.calls).toEqual([]);
  });

  test("a cancelled run: no POST", () => {
    const r = cli({ jobs: jobs(11, 12, 13) }, { RUN_CONCLUSION: "cancelled" });
    expect(r.out).toMatch(/concluded cancelled/);
    expect(r.calls).toEqual([]);
  });

  test("the branch moved on (a newer push): no POST", () => {
    const r = cli({ jobs: jobs(11, 12, 13), tip: "b".repeat(40) });
    expect(r.out).toMatch(/no longer the tip/);
    expect(r.calls).toEqual([]);
  });

  test("a branch name that is not a plain ref name is never passed to gh: no POST", () => {
    const r = cli({ jobs: jobs(11, 12, 13) }, { HEAD_BRANCH: "x; rm -rf /" });
    expect(r.out).toMatch(/no longer the tip/);
    expect(r.calls).toEqual([]);
  });

  test("a malformed run id is a usage error (exit 2), nothing runs", () => {
    const r = cli({ jobs: jobs(11) }, { RUN_ID: "1; echo hi" });
    expect(r.code).toBe(2);
    expect(r.err).toMatch(/RUN_ID is missing or malformed/);
    expect(r.calls).toEqual([]);
  });
});
