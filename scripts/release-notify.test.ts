// Telling reporters when a fix ships (#321): which issues are told, what they read, that nobody is told twice, and the
// CLI end to end over a real git repo with `gh` stubbed. `fixtures/release-notify-search.json` is the REAL GraphQL answer
// of GitHub for `SEARCH` (closed issues of this repo in a window), not a hand-written mock.
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { carriesMarker, commentBody, type Gh, type IssueNode, issueOf, MARKER, notify, previousTag, targets } from "./release-notify.ts";

const search = JSON.parse(readFileSync(join(import.meta.dir, "fixtures/release-notify-search.json"), "utf8"));

const issue = (number: number, extra: Partial<IssueNode> = {}): IssueNode => ({ number, stateReason: "COMPLETED", labels: ["type: bug"], duplicates: [], ...extra });

describe("previousTag", () => {
  test("is the highest release below the tag, with a prerelease below its stable and non-release tags ignored", () => {
    const tags = ["v1.4.0", "v1.5.0-alpha.9", "v1.5.0-alpha.10", "v1.5.0-rc.1", "v1.5.0", "v1.5.1", "mobile-v3", "v1.5.0-alpha.11-extra!"];
    expect(previousTag(tags, "v1.5.0-alpha.11")).toBe("v1.5.0-alpha.10");
    expect(previousTag(tags, "v1.5.0")).toBe("v1.5.0-rc.1");
    expect(previousTag(tags, "v1.5.1")).toBe("v1.5.0");
    expect(previousTag(tags, "v1.4.0")).toBeUndefined();
  });
});

describe("targets", () => {
  test("only work that was completed is announced: not planned, duplicate, reopened and unlabelled-as-shipped are not", () => {
    const got = targets([
      issue(1),
      issue(2, { stateReason: "NOT_PLANNED" }),
      issue(3, { stateReason: "DUPLICATE" }),
      issue(4, { stateReason: "REOPENED" }),
      issue(5, { stateReason: null }),
      issue(6, { labels: ["type: question"] }),
      issue(7, { labels: ["type: bug", "status: wontfix"] }),
      issue(8, { labels: ["status: duplicate"] }),
    ]);
    expect(got).toEqual([{ number: 1, kind: "closed" }]);
  });

  test("a report marked as a duplicate of a fixed issue is told once, after the issues, and never when its original shipped nothing", () => {
    const got = targets([
      issue(10, { duplicates: [30, 31, 10, 11] }),
      issue(11, { duplicates: [30] }),
      issue(20, { stateReason: "NOT_PLANNED", duplicates: [40] }),
    ]);
    expect(got).toEqual([
      { number: 10, kind: "closed" },
      { number: 11, kind: "closed" },
      { number: 30, kind: "duplicate", of: 10 },
      { number: 31, kind: "duplicate", of: 10 },
    ]);
  });
});

describe("issueOf (GitHub's real answer)", () => {
  const nodes = search.data.search.nodes;

  test("reads every real node: number, reason and labels", () => {
    const issues = nodes.map(issueOf);
    expect(issues.map((i: IssueNode) => i.number)).toEqual([648, 647, 645, 644, 643, 642]);
    expect(issues.every((i: IssueNode) => i.stateReason === "COMPLETED")).toBe(true);
    expect(issues[0].labels).toContain("type: feature");
  });

  // Real data: #642 is cross-referenced by #648 (closed as COMPLETED) and by open issues (stateReason null), and the
  // references of pull requests come as `{}`. None of them is a duplicate, and none may be told as one.
  test("a reference from an open issue, a pull request or a completed issue is not a duplicate", () => {
    for (const n of nodes) expect(issueOf(n)?.duplicates, `#${n.number}`).toEqual([]);
  });

  test("a report closed as a duplicate that mentions the issue, and the duplicate event naming it canonical, are its duplicates", () => {
    const base = { number: 5, stateReason: "COMPLETED", labels: { nodes: [] } };
    const node = (...items: object[]) => ({ ...base, timelineItems: { nodes: items } });
    const got = issueOf(
      node(
        { __typename: "CrossReferencedEvent", source: { number: 9, stateReason: "DUPLICATE" } },
        { __typename: "CrossReferencedEvent", source: { number: 8, stateReason: null } },
        { __typename: "CrossReferencedEvent", source: {} },
        { __typename: "MarkedAsDuplicateEvent", canonical: { number: 5 }, duplicate: { number: 7 } },
        // This issue is the duplicate here, not the original: its canonical is another one, and nothing is told for it.
        { __typename: "MarkedAsDuplicateEvent", canonical: { number: 2 }, duplicate: { number: 5 } },
      ),
    );
    expect(got?.duplicates.sort()).toEqual([7, 9]);
  });

  test("a pull request in the search results (no number) is skipped", () => {
    expect(issueOf({ labels: { nodes: [] }, timelineItems: { nodes: [] } })).toBeNull();
  });
});

describe("commentBody", () => {
  test("names the version and the channel of the tag, and carries the marker", () => {
    expect(commentBody("v1.5.0-alpha.28", { number: 1, kind: "closed" })).toContain("**Fixed in v1.5.0-alpha.28** (alpha channel)");
    expect(commentBody("v1.5.0-beta.2", { number: 1, kind: "closed" })).toContain("(beta channel)");
    expect(commentBody("v1.5.0-rc.1", { number: 1, kind: "closed" })).toContain("(beta channel)"); // an rc is offered on beta
    expect(commentBody("v1.5.0", { number: 1, kind: "closed" })).toContain("(Stable channel)");
    expect(commentBody("v1.5.0-alpha.28", { number: 1, kind: "closed" })).toContain(`<!-- ${MARKER}v1.5.0-alpha.28 -->`);
  });

  test("an alpha says who has it now; Stable says everyone does", () => {
    expect(commentBody("v1.5.0-alpha.28", { number: 1, kind: "closed" })).toContain("if you follow the alpha channel");
    expect(commentBody("v1.5.0", { number: 1, kind: "closed" })).not.toContain("follow the");
  });

  test("a duplicate report is told which issue was fixed", () => {
    const b = commentBody("v1.5.0-alpha.28", { number: 30, kind: "duplicate", of: 10 });
    expect(b).toContain("#10, which this report was marked as a duplicate of, is **fixed in v1.5.0-alpha.28**");
  });
});

describe("carriesMarker", () => {
  test("recognises our comment of any release, not a person's mention of the phrase", () => {
    expect(carriesMarker(["thanks!", commentBody("v1.5.0-alpha.20", { number: 1, kind: "closed" })])).toBe(true);
    expect(carriesMarker(["Fixed in v1.5.0-alpha.20, I think", "<!-- something else -->"])).toBe(false);
    expect(carriesMarker([])).toBe(false);
  });
});

interface FakeGh {
  gh: Gh;
  comments: Record<number, string[]>;
  asked: { from: string; to: string }[];
}

/** A GitHub that holds issues, their comments and the window it was asked for; `comment` really adds one. */
function fakeGh(issues: IssueNode[], existing: Record<number, string[]> = {}): FakeGh {
  const comments: Record<number, string[]> = { ...existing };
  const asked: { from: string; to: string }[] = [];
  const gh: Gh = {
    async closedBetween(_repo, from, to) {
      asked.push({ from, to });
      return issues;
    },
    async comments(_repo, n) {
      return comments[n] ?? [];
    },
    async comment(_repo, n, body) {
      (comments[n] ??= []).push(body);
    },
  };
  return { gh, comments, asked };
}

describe("notify", () => {
  const run = (f: FakeGh, dry = false, tag = "v1.5.0-alpha.28") => {
    const lines: string[] = [];
    return notify({ repo: "o/r", tag, from: "A", to: "B", dry, gh: f.gh, log: (l) => lines.push(l) }).then((out) => ({ out, lines }));
  };

  test("comments once on each issue and on its duplicate, and asks GitHub for the window it was given", async () => {
    const f = fakeGh([issue(1, { duplicates: [3] }), issue(2, { stateReason: "NOT_PLANNED" })]);
    const { out } = await run(f);
    expect(out.commented).toEqual([1, 3]);
    expect(f.comments[1]).toHaveLength(1);
    expect(f.comments[3][0]).toContain("#1, which this report");
    expect(f.comments[2]).toBeUndefined();
    expect(f.asked).toEqual([{ from: "A", to: "B" }]);
  });

  // Incident class: `release.sh` resumes a publish, and the next release's window starts at the previous tag, so the same
  // issue can be offered twice. A person must never get two "Fixed in" notices.
  test("a second run, and the next release, write nothing to an issue already told", async () => {
    const f = fakeGh([issue(1), issue(2)]);
    await run(f);
    const again = await run(f);
    expect(again.out.commented).toEqual([]);
    expect(again.out.skipped.map((s) => s.number)).toEqual([1, 2]);
    const next = await run(f, false, "v1.5.0-alpha.29");
    expect(next.out.commented).toEqual([]);
    expect(f.comments[1]).toHaveLength(1);
  });

  test("an issue that already has a person's comment is still told", async () => {
    const f = fakeGh([issue(1)], { 1: ["me too", "any news?"] });
    expect((await run(f)).out.commented).toEqual([1]);
    expect(f.comments[1]).toHaveLength(3);
  });

  test("dry run prints one line per issue, with the full text once, and writes nothing", async () => {
    const f = fakeGh([issue(1), issue(2), issue(3, { stateReason: "NOT_PLANNED" })]);
    const { out, lines } = await run(f, true);
    expect(out.commented).toEqual([1, 2]);
    expect(f.comments).toEqual({});
    const text = lines.join("\n");
    expect(text.match(/\+ comment on #\d+/g)).toEqual(["+ comment on #1", "+ comment on #2"]);
    expect(text.match(/Run `atlas update`/g)).toHaveLength(1);
    expect(text).toContain("1 issue(s) closed in the window were not completed work");
  });
});

// The CLI end to end: a real git repo whose tags have known dates, and a `gh` that serves the real GraphQL answer and logs.
describe.skipIf(process.platform === "win32")("release-notify.ts CLI", () => {
  function scratch() {
    const dir = mkdtempSync(join(tmpdir(), "notify-"));
    const env = { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1", XDG_CONFIG_HOME: undefined };
    const git = (date: string, ...a: string[]) => {
      const r = Bun.spawnSync(["git", "-c", "user.name=t", "-c", "user.email=t@e.x", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", ...a], {
        cwd: dir,
        env: { ...env, GIT_COMMITTER_DATE: date, GIT_AUTHOR_DATE: date },
      });
      expect(r.exitCode, r.stderr.toString()).toBe(0);
    };
    git("2026-11-01T10:00:00Z", "init", "-q", "-b", "main");
    for (const [n, date] of [["alpha.1", "2026-11-01T10:00:00Z"], ["alpha.2", "2026-11-05T12:30:00Z"]]) {
      git(date, "commit", "-q", "--allow-empty", "-m", n);
      git(date, "tag", "-a", `v1.0.0-${n}`, "-m", n);
    }
    const bin = join(dir, "bin");
    mkdirSync(bin);
    const log = join(dir, "calls.log");
    // gh api graphql -> the real answer; gh api .../comments -> the file of that issue (or none); gh issue comment -> logged with its body.
    writeFileSync(
      join(bin, "gh"),
      `#!/bin/sh
case "$1 $2" in
  "api graphql") for a in "$@"; do case "$a" in q=*) echo "$a" >> "${log}";; esac; done; cat "${join(import.meta.dir, "fixtures/release-notify-search.json")}";;
  "api repos"*) f="${dir}/comments-$(echo "$2" | sed 's|.*/issues/\\([0-9]*\\)/.*|\\1|').json"; [ -f "$f" ] && cat "$f" || echo '[[]]';;
  "issue comment") echo "comment $3 $(cat | tr '\\n' ' ')" >> "${log}";;
  *) echo "unexpected gh $*" >> "${log}"; exit 1;;
esac
`,
    );
    chmodSync(join(bin, "gh"), 0o755);
    const run = (...args: string[]) => {
      const r = Bun.spawnSync([process.execPath, join(import.meta.dir, "release-notify.ts"), ...args], { cwd: dir, env: { ...env, PATH: `${bin}:${process.env.PATH}` } });
      return { code: r.exitCode, out: r.stdout.toString(), err: r.stderr.toString() };
    };
    const calls = () => (existsSync(log) ? readFileSync(log, "utf8").split("\n").filter(Boolean) : []);
    return { dir, run, calls };
  }

  test("the window is between the commit dates of the previous tag and this one, in UTC", () => {
    const s = scratch();
    const r = s.run("v1.0.0-alpha.2", "--dry-run");
    expect(r.code, r.err).toBe(0);
    expect(s.calls()).toEqual(["q=repo:codeatlasdev/atlas is:issue is:closed closed:2026-11-01T10:00:00Z..2026-11-05T12:30:00Z"]);
    expect(r.out).toContain("between v1.0.0-alpha.1");
    expect(r.out.match(/\+ comment on #\d+/g)).toHaveLength(6);
    expect(r.out).toContain("would comment on 6 issue(s)");
  });

  test("a dry run posts nothing; a real run posts one comment per issue, with the tag in it", () => {
    const s = scratch();
    s.run("v1.0.0-alpha.2", "--dry-run");
    expect(s.calls().filter((c) => c.startsWith("comment"))).toEqual([]);
    const r = s.run("v1.0.0-alpha.2");
    expect(r.code, r.err).toBe(0);
    const posted = s.calls().filter((c) => c.startsWith("comment"));
    expect(posted.map((c) => c.split(" ")[1])).toEqual(["648", "647", "645", "644", "643", "642"]);
    expect(posted.every((c) => c.includes("Fixed in v1.0.0-alpha.2** (alpha channel)") && c.includes("atlas-fixed-in:v1.0.0-alpha.2"))).toBe(true);
  });

  test("an issue whose comments already carry our marker is left alone", () => {
    const s = scratch();
    writeFileSync(join(s.dir, "comments-647.json"), JSON.stringify([[{ body: commentBody("v1.0.0-alpha.1", { number: 647, kind: "closed" }) }]]));
    const r = s.run("v1.0.0-alpha.2");
    expect(r.code, r.err).toBe(0);
    expect(s.calls().filter((c) => c.startsWith("comment")).map((c) => c.split(" ")[1])).not.toContain("647");
    expect(r.out).toContain("#647: already has");
  });

  test("a tag with nothing before it, and a malformed tag, fail without calling GitHub", () => {
    const s = scratch();
    expect(s.run("v1.0.0-alpha.1").code).toBe(1);
    expect(s.run("alpha.2").code).toBe(2);
    expect(s.calls()).toEqual([]);
  });
});
