// The readers of scripts/ci-stall-report.ts on what the CI really wrote (recorded in scripts/fixtures/ci-stall): the
// watchdog's check run of run 44366063379 and the log tail of run 44413383839. The comment, label and issue line the
// rerun script makes of them are tested end to end in ci-lost-runner.test.ts.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { endingStep, logTail, lastSample } from "./ci-stall-report";

const fixture = (f: string) => readFileSync(join(import.meta.dir, "fixtures/ci-stall", f), "utf8");

describe("lastSample (the watchdog's check run text)", () => {
  test("run 44366063379: the last sample is the one 5 min into the tests, with its four running tests and their seconds", () => {
    const s = lastSample(JSON.parse(fixture("telemetry-44366063379.json")).output.text);
    expect(s).toEqual({
      at: "18:15:43",
      progress: "2509/3199",
      tests: [
        { name: "atlas_host crew::telegram::tests::five_wrong_codes_kill_the_link_code_even_for_the_right_one", secs: 14 },
        { name: "scrollback_memory a_wide_terminal_stays_inside_the_scrollback_byte_budget", secs: 7 },
        { name: "atlas_host crew::telegram::tests::start_code_links_chat_via_handle_update", secs: 1 },
        { name: "atlas_tui config::tests::test_merge_local_override", secs: 1 },
      ],
    });
  });

  // Seen in run 44095517976, a job that stopped answering while the test profile was still being built.
  test("a sample taken while the tests are still being built has no tests and no finished test", () => {
    const text = [
      "13:48:21 [watchdog] mem free 10.6/16.0 GB | commit 7.1/18.9 GB",
      "13:48:21 [watchdog] nextest: none finished yet | last line: Compiling atlas-host v1.5.0-alpha.26 (D:\\a\\atlas\\atlas\\crates\\atlas-host) | building: 3 rustc, 1 link",
      "13:48:21 [watchdog] tests running (0): none",
    ].join("\n");
    expect(lastSample(text)).toEqual({ at: "13:48:21", progress: "no test finished yet", tests: [] });
  });

  test("text with no sample (the check run was never updated) is null, not an empty list", () => {
    expect(lastSample("")).toBeNull();
    expect(lastSample("```\n18:10:30 [watchdog] start: Microsoft Windows Server 2025\n```")).toBeNull();
  });
});

describe("logTail (the job log, up to GitHub's cancel)", () => {
  test("run 44413383839: the three lines before `The operation was canceled.`, timestamps off", () => {
    expect(logTail(fixture("job-log-tail-44413383839.txt"))).toEqual([
      "bootstrap channel selection: 76 passed, 0 failed",
      "labels.yml: 37 labels, names unique, colors and descriptions valid",
      "sync-labels.sh --dry-run: 37 commands, all --force",
    ]);
  });

  test("a log without an error line gives its last lines; an empty log gives none", () => {
    expect(logTail("2026-11-18T23:12:38.1Z a\n2026-11-18T23:12:38.2Z \n2026-11-18T23:12:38.3Z b", 5)).toEqual(["a", "b"]);
    expect(logTail("")).toEqual([]);
  });
});

describe("endingStep", () => {
  test("the step still running, else the one cancelled; none when every step finished", () => {
    expect(endingStep([{ name: "Checkout", status: "completed", conclusion: "success" }, { name: "Test", status: "in_progress", conclusion: null }, { name: "Doctests", status: "pending", conclusion: null }])).toBe("Test");
    expect(endingStep([{ name: "bun run release:test", status: "completed", conclusion: "cancelled" }])).toBe("bun run release:test");
    expect(endingStep([{ name: "Checkout", status: "completed", conclusion: "success" }])).toBeNull();
  });
});
