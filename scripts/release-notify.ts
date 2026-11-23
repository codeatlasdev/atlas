#!/usr/bin/env bun
// After a release is published, tell the people whose issues it closed (#321, docs/release.md "Telling reporters").
// `Fixes #N` closes an issue at merge, which can be days before the alpha that carries it ships, and says nothing
// about which version to run. This comments on every issue closed as completed between the previous release tag and
// this one, and on the reports that were marked as duplicates of it, with the version and the channel.
//
//   bun scripts/release-notify.ts vX.Y.Z[-pre.N] [--dry-run] [--repo owner/name]
//
// It is safe to run again: an issue that already has a "Fixed in" comment of ours is left alone. `bun run release`
// calls it after the release is published and never fails because of it. The pure parts (`previousTag`, `targets`,
// `commentBody`, `carriesMarker`) and `notify`, which takes the GitHub calls as an argument, are unit-tested
// (release-notify.test.ts); the CLI at the bottom only reads git and talks to `gh`.

import { channelOfTag, type Channel } from "./release-version.ts";

/** Issues with one of these labels were closed for a reason other than "we changed Atlas": nothing shipped for them. */
export const SKIP_LABELS = ["status: wontfix", "status: duplicate", "type: question"];

/** Every comment this script writes ends with `<!-- atlas-fixed-in:vX -->`: how a rerun, or the next release, knows. */
export const MARKER = "atlas-fixed-in:";

/** An issue closed in the window, with what decides whether and why it is told. */
export interface IssueNode {
  number: number;
  /** `COMPLETED`, `NOT_PLANNED`, `DUPLICATE`, `REOPENED` or null (GitHub's `stateReason`). */
  stateReason: string | null;
  labels: string[];
  /** Issues that were marked as duplicates of this one, whenever they were closed. */
  duplicates: number[];
}

/** One comment to write: on `number`, because it was `closed` itself, or is a `duplicate` of `of`. */
export interface Target {
  number: number;
  kind: "closed" | "duplicate";
  of?: number;
}

/** The GitHub calls `notify` makes, so tests can stand a fake in. */
export interface Gh {
  /** Issues of `repo` closed from `from` to `to` (ISO 8601, inclusive), with their duplicates. */
  closedBetween(repo: string, from: string, to: string): Promise<IssueNode[]>;
  /** Bodies of every comment on issue `number`. */
  comments(repo: string, number: number): Promise<string[]>;
  comment(repo: string, number: number, body: string): Promise<void>;
}

/** The highest release tag below `tag` (SemVer order, so `1.5.0-rc.1 < 1.5.0`): the release this one follows. */
export function previousTag(tags: string[], tag: string): string | undefined {
  return tags
    .filter((t) => /^v\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/.test(t) && Bun.semver.order(t.slice(1), tag.slice(1)) < 0)
    .sort((a, b) => Bun.semver.order(b.slice(1), a.slice(1)))[0];
}

/** Who gets a comment: the issues closed as completed (unless labelled as not shipped), then the reports marked as
 *  duplicates of them. Each issue once, in order, and never the issue itself as its own duplicate. */
export function targets(issues: IssueNode[]): Target[] {
  const fixed = issues.filter((i) => i.stateReason === "COMPLETED" && !i.labels.some((l) => SKIP_LABELS.includes(l)));
  const out: Target[] = fixed.map((i) => ({ number: i.number, kind: "closed" }));
  const seen = new Set(out.map((t) => t.number));
  for (const i of fixed) {
    for (const d of i.duplicates) {
      if (seen.has(d)) continue;
      seen.add(d);
      out.push({ number: d, kind: "duplicate", of: i.number });
    }
  }
  return out;
}

const CHANNEL_NAME: Record<Channel, string> = { alpha: "alpha", beta: "beta", stable: "Stable" };

/** What a reporter reads. The channel decides who has it now: a build of alpha reaches only the people who follow alpha. */
export function commentBody(tag: string, target: Target): string {
  const channel = channelOfTag(tag);
  const name = channel ? CHANNEL_NAME[channel] : "unknown";
  const lead =
    target.kind === "duplicate"
      ? `#${target.of}, which this report was marked as a duplicate of, is **fixed in ${tag}** (${name} channel).`
      : `**Fixed in ${tag}** (${name} channel).`;
  const who =
    channel === "stable"
      ? "Everyone on Stable has it after `atlas update`."
      : channel === "beta"
        ? "Run `atlas update` if you follow the beta channel; Stable gets it when a beta is promoted."
        : "Run `atlas update` if you follow the alpha channel (`atlas update channel` shows yours); Beta and Stable get it when it is promoted.";
  return [
    lead,
    "",
    `${who} Atlas Desktop offers the same update.`,
    "",
    "If it still happens on this version, comment here with the details from **Report issue** (or `atlas doctor`) and it will be reopened.",
    "",
    `<!-- ${MARKER}${tag} -->`,
  ].join("\n");
}

/** Whether one of `bodies` is a comment this script wrote (for any release). */
export function carriesMarker(bodies: string[]): boolean {
  return bodies.some((b) => b.includes(`<!-- ${MARKER}`));
}

export interface Outcome {
  commented: number[];
  skipped: { number: number; reason: string }[];
}

/** Comment on every target of `tag` that has not been told yet; with `dry`, only print what would be written. */
export async function notify(o: {
  repo: string;
  tag: string;
  from: string;
  to: string;
  dry: boolean;
  gh: Gh;
  log?: (line: string) => void;
}): Promise<Outcome> {
  const log = o.log ?? console.log;
  const issues = await o.gh.closedBetween(o.repo, o.from, o.to);
  const wanted = targets(issues);
  const out: Outcome = { commented: [], skipped: [] };
  const shown = new Set<Target["kind"]>();
  for (const t of wanted) {
    if (carriesMarker(await o.gh.comments(o.repo, t.number))) {
      out.skipped.push({ number: t.number, reason: "already told" });
      log(`- #${t.number}: already has a "Fixed in" comment, left alone`);
      continue;
    }
    const body = commentBody(o.tag, t);
    if (o.dry) {
      // The full text once per kind of comment; every later one is the same but for its first line.
      const first = shown.has(t.kind) ? body.split("\n")[0] : `\n${body.replace(/^/gm, "    ")}`;
      shown.add(t.kind);
      log(`+ comment on #${t.number}: ${first}`);
    } else {
      await o.gh.comment(o.repo, t.number, body);
      log(`✓ commented on #${t.number}`);
    }
    out.commented.push(t.number);
  }
  const notWork = issues.length - wanted.filter((t) => t.kind === "closed").length;
  if (notWork) log(`- ${notWork} issue(s) closed in the window were not completed work (not planned, duplicate, question, wontfix)`);
  return out;
}

export const SEARCH = `query($q: String!, $after: String) {
  search(query: $q, type: ISSUE, first: 50, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes { ... on Issue {
      number stateReason
      labels(first: 30) { nodes { name } }
      timelineItems(first: 100, itemTypes: [MARKED_AS_DUPLICATE_EVENT, CROSS_REFERENCED_EVENT]) { nodes {
        __typename
        ... on MarkedAsDuplicateEvent { canonical { ... on Issue { number } } duplicate { ... on Issue { number } } }
        ... on CrossReferencedEvent { source { ... on Issue { number stateReason } } }
      } }
    } }
  }
}`;

interface TimelineNode {
  __typename: string;
  canonical?: { number?: number } | null;
  duplicate?: { number?: number } | null;
  source?: { number?: number; stateReason?: string | null } | null;
}
interface SearchNode {
  number?: number;
  stateReason?: string | null;
  labels: { nodes: { name: string }[] };
  timelineItems: { nodes: TimelineNode[] };
}

/** The issue as `notify` sees it, from the GraphQL node (`SEARCH`). A duplicate is either an event that names this issue as
 *  canonical, or an issue that was closed as a duplicate and mentions this one. */
export function issueOf(n: SearchNode): IssueNode | null {
  if (n.number === undefined) return null; // a pull request in the results
  const dups = new Set<number>();
  for (const e of n.timelineItems.nodes) {
    if (e.__typename === "MarkedAsDuplicateEvent" && e.canonical?.number === n.number && e.duplicate?.number !== undefined) dups.add(e.duplicate.number);
    if (e.__typename === "CrossReferencedEvent" && e.source?.number !== undefined && e.source.stateReason === "DUPLICATE") dups.add(e.source.number);
  }
  dups.delete(n.number);
  return { number: n.number, stateReason: n.stateReason ?? null, labels: n.labels.nodes.map((l) => l.name), duplicates: [...dups] };
}

/** `gh`, for real. */
export function realGh(): Gh {
  return {
    async closedBetween(repo, from, to) {
      const issues: IssueNode[] = [];
      let after: string | null = null;
      do {
        const args = ["api", "graphql", "-f", `query=${SEARCH}`, "-f", `q=repo:${repo} is:issue is:closed closed:${from}..${to}`];
        if (after) args.push("-f", `after=${after}`);
        const res = JSON.parse(await Bun.$`gh ${args}`.quiet().text()) as {
          data: { search: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: SearchNode[] } };
        };
        for (const n of res.data.search.nodes) {
          const i = issueOf(n);
          if (i) issues.push(i);
        }
        const { hasNextPage, endCursor } = res.data.search.pageInfo;
        after = hasNextPage ? endCursor : null;
      } while (after);
      return issues;
    },
    async comments(repo, number) {
      const pages = JSON.parse(await Bun.$`gh api repos/${repo}/issues/${number}/comments --paginate --slurp`.quiet().text()) as { body: string }[][];
      return pages.flat().map((c) => c.body);
    },
    async comment(repo, number, body) {
      await Bun.$`gh issue comment ${number} --repo ${repo} --body-file - < ${new Response(body)}`.quiet();
    },
  };
}

const git = async (...args: string[]) => (await Bun.$`git ${args}`.quiet().text()).trim();

/** A tag's commit time as the UTC instant GitHub's `closed:` qualifier reads. */
async function instantOf(tag: string): Promise<string> {
  return new Date(await git("log", "-1", "--format=%cI", tag)).toISOString().replace(/\.\d+Z$/, "Z");
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  const tag = argv.find((a) => a.startsWith("v"));
  const repoAt = argv.indexOf("--repo");
  const repo = repoAt >= 0 ? argv[repoAt + 1] : (process.env.GITHUB_REPOSITORY ?? "codeatlasdev/atlas");
  if (!tag || !/^v\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/.test(tag) || !repo) {
    console.error("usage: bun scripts/release-notify.ts vX.Y.Z[-pre.N] [--dry-run] [--repo owner/name]");
    process.exit(2);
  }
  // A tag that is only on origin (`release.sh --publish` on another machine) is fetched first.
  if ((await Bun.$`git rev-parse -q --verify ${`refs/tags/${tag}`}`.quiet().nothrow()).exitCode !== 0) await Bun.$`git fetch --quiet --tags origin`.quiet();
  const before = previousTag((await git("tag", "--list", "v[0-9]*")).split("\n").filter(Boolean), tag);
  if (!before) {
    console.error(`✗ no release tag before ${tag}: nothing to compare with`);
    process.exit(1);
  }
  const [from, to] = [await instantOf(before), await instantOf(tag)];
  console.log(`▶ issues closed in ${repo} between ${before} (${from}) and ${tag} (${to})`);
  const out = await notify({ repo, tag, from, to, dry: argv.includes("--dry-run"), gh: realGh() });
  console.log(`${argv.includes("--dry-run") ? "would comment on" : "commented on"} ${out.commented.length} issue(s), left ${out.skipped.length} alone`);
}
