#!/usr/bin/env bun
// Version, branch and soak rules for `bun run release` (docs/release.md, docs/versioning.md).
// `nextVersion` and `diffAllowed` are pure and unit-tested (release-version.test.ts);
// the CLI at the bottom only reads git state and prints their result for scripts/release.sh.

export type Channel = "stable" | "beta" | "alpha";

export interface NextInput {
  /** alpha | beta | rc | stable | patch | minor | major | X.Y.Z[-label.N] */
  request: string;
  /** Version in crates/atlas-cli/Cargo.toml. */
  current: string;
  /** Every `v*` tag, local and remote. */
  tags: string[];
  /** Current git branch. */
  branch: string;
  /** Every local and remote branch name (without `origin/`). */
  branches: string[];
  now: Date;
  /** Creation date of the newest prerelease tag of the base being promoted (ISO 8601). */
  lastPrereleaseTagDate?: string;
  forceSoak?: boolean;
  /** `major`: alpha/beta start a new major line. */
  bump?: "major";
}

export interface NextResult {
  version?: string;
  tag?: string;
  channel?: Channel;
  /** First beta on main: create this branch from HEAD. */
  createBranch?: string;
  /** Lines that must be written into the release notes (e.g. a forced soak). */
  notes?: string[];
  errors: string[];
}

/** The default channel recorded in `crates/atlas-core/data/channels.json` (atlas-core pins it to `#[default]`). Throws on anything
 * else: a release must never guess which channel is "Latest". */
export function readDefaultChannel(channelsJson: { default?: unknown }): Channel {
  const d = channelsJson.default;
  if (d === "stable" || d === "beta" || d === "alpha") return d;
  throw new Error(`channels.json: "default" must be stable, beta or alpha (got ${JSON.stringify(d)})`);
}

/** Which channel a tag was cut for. `null` for a label Atlas does not ship. */
export function channelOfTag(tag: string): Channel | null {
  const p = parse(tag.replace(/^v/, ""));
  if (!p) return null;
  return p.label === undefined ? "stable" : p.label === "alpha" ? "alpha" : "beta"; // rc is offered on beta
}

/**
 * The GitHub Release flags for `tag`. A build of the DEFAULT channel is a normal release and "Latest", so GitHub's Latest badge
 * and `/releases/latest` show what a user who never chose gets; a stable is always Latest. Every other prerelease label (a beta
 * while alpha is the default, an alpha while beta is) stays a prerelease. At GA the default is stable and this is the old rule:
 * any `-` tag is a prerelease. Clients never rely on `/releases/latest` for Stable (docs/versioning.md): they read the feed.
 */
export function githubFlags(tag: string, defaultChannel: Channel): { prerelease: boolean; makeLatest: boolean } {
  const of = channelOfTag(tag);
  const latest = of === "stable" || (of !== null && of === defaultChannel);
  return { prerelease: !latest, makeLatest: latest };
}

/** Minimum time a beta/rc must have been out before it can become stable. */
export const SOAK_HOURS = 48;

const LABELS = ["alpha", "beta", "rc"] as const;
type Label = (typeof LABELS)[number];
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.(0|[1-9]\d*))?$/;

interface Parsed {
  major: number;
  minor: number;
  patch: number;
  label?: Label;
  n?: number;
}

function parse(v: string): Parsed | undefined {
  const m = SEMVER.exec(v);
  if (!m) return undefined;
  return { major: +m[1], minor: +m[2], patch: +m[3], label: m[4] as Label | undefined, n: m[5] === undefined ? undefined : +m[5] };
}

const baseOf = (p: Parsed) => `${p.major}.${p.minor}.${p.patch}`;
const fmt = (base: string, label?: Label, n?: number) => (label ? `${base}-${label}.${n}` : base);
const lineOf = (base: string) => base.split(".").slice(0, 2).join(".");

/** Why an explicit version string is invalid, with the footgun called out. */
function explainInvalid(v: string): string {
  if (/^v/.test(v)) return `invalid version ${v}: drop the leading "v" (the tag gets it)`;
  if (/\+/.test(v)) return `invalid version ${v}: build metadata (+...) is never used`;
  const glued = /-(alpha|beta|rc)(\d+)$/.exec(v);
  if (glued) return `invalid version ${v}: put a dot before the counter (${v.replace(/(alpha|beta|rc)(\d+)$/, "$1.$2")}); "${glued[1]}${glued[2]}" would compare as text`;
  if (/-(alpha|beta|rc)$/.test(v)) return `invalid version ${v}: a prerelease needs a counter (${v}.1)`;
  if (/-/.test(v) && !/-(alpha|beta|rc)\./.test(v)) return `invalid version ${v}: allowed prerelease labels are alpha, beta, rc`;
  return `invalid version ${v}: expected X.Y.Z or X.Y.Z-(alpha|beta|rc).N without leading zeros`;
}

/** Parsed `v`-tags that follow our grammar; anything else (v0.x oddities, moving tags) is ignored. */
function parsedTags(tags: string[]): { v: string; p: Parsed }[] {
  const out: { v: string; p: Parsed }[] = [];
  for (const t of tags) {
    const v = t.startsWith("v") ? t.slice(1) : "";
    const p = parse(v);
    if (p) out.push({ v, p });
  }
  return out;
}

const fail = (...errors: string[]): NextResult => ({ errors });

const channelOf = (label?: Label): Channel => (label === "alpha" ? "alpha" : label ? "beta" : "stable");

export function nextVersion(input: NextInput): NextResult {
  const { request, current, tags, branch, branches, now, lastPrereleaseTagDate, forceSoak, bump } = input;
  const known = parsedTags(tags);
  const has = (v: string) => known.some((t) => t.v === v);

  const cur = parse(current);
  if (!cur) return fail(`cannot read the current version "${current}" from Cargo.toml`);

  const onMain = branch === "main";
  const relMatch = /^release\/(\d+\.\d+)$/.exec(branch);
  const relLine = relMatch?.[1];

  // Highest N of `base-label.N` among existing tags.
  const highest = (base: string, label: Label) =>
    known.reduce((max, t) => (baseOf(t.p) === base && t.p.label === label ? Math.max(max, t.p.n ?? 0) : max), 0);
  const hasLabel = (base: string, ...labels: Label[]) => known.some((t) => baseOf(t.p) === base && t.p.label && labels.includes(t.p.label));
  const stableExists = (base: string) => has(base);

  /** The base `main` is working towards: the line that has not branched yet. */
  const mainBase = (): string => {
    let { major, minor } = cur;
    // A prerelease Cargo version already names its base; a stable one means the next minor.
    let patch = cur.patch;
    if (!cur.label) {
      minor += 1;
      patch = 0;
    }
    if (bump === "major") {
      major = cur.major + 1;
      minor = 0;
      patch = 0;
    }
    for (;;) {
      const base = `${major}.${minor}.${patch}`;
      const branched = branches.includes(`release/${major}.${minor}`) || hasLabel(base, "beta", "rc") || stableExists(base);
      if (!branched) return base;
      minor += 1;
      patch = 0;
    }
  };

  const finish = (version: string, extra: Partial<NextResult> = {}): NextResult => {
    const p = parse(version)!;
    if (has(version)) return fail(`tag v${version} already exists`);
    const newer = known.filter((t) => Bun.semver.order(t.v, version) >= 0);
    // Counters are derived from tags, so this only fires for explicit versions and hotfixes out of order.
    if (newer.length > 0 && !extra.createBranch && !(p.label === undefined && p.patch > 0)) {
      return fail(`${version} must be greater than every existing tag (newest is v${newer.map((t) => t.v).sort(Bun.semver.order).at(-1)})`);
    }
    return { version, tag: `v${version}`, channel: channelOf(p.label), errors: [], ...extra };
  };

  const needMainForAlpha = `alpha releases are cut from main (on ${branch})`;
  const needRelease = (what: string) => `${what} needs a release/X.Y branch (on ${branch}); the first beta from main creates it`;

  // --- explicit version: same rules as its channel -----------------------------------------------------
  const explicit = /^\d/.test(request) || request.startsWith("v") ? request : undefined;
  if (explicit !== undefined) {
    const p = parse(explicit);
    if (!p) return fail(explicit.startsWith("v") || /^\d/.test(explicit) ? explainInvalid(explicit) : `unknown request "${request}"`);
    const base = baseOf(p);
    if (has(explicit)) return fail(`tag v${explicit} already exists`);
    const newest = known.map((t) => t.v).sort(Bun.semver.order).at(-1);
    const isHotfix = !p.label && p.patch > 0;
    if (!isHotfix && newest && Bun.semver.order(explicit, newest) <= 0) {
      return fail(`${explicit} must be greater than every existing tag (newest is v${newest})`);
    }
    if (p.label === "alpha") {
      if (!onMain) return fail(needMainForAlpha);
      return { version: explicit, tag: `v${explicit}`, channel: "alpha", errors: [] };
    }
    if (p.label) {
      if (onMain) {
        if (p.label !== "beta" || p.n !== 1) return fail(`${explicit}: from main only the first beta (X.Y.0-beta.1) is allowed; later betas and rc come from release/X.Y`);
        if (branches.includes(`release/${lineOf(base)}`)) return fail(`release/${lineOf(base)} already exists; cut ${explicit} from it`);
        return { version: explicit, tag: `v${explicit}`, channel: "beta", createBranch: `release/${lineOf(base)}`, errors: [] };
      }
      if (!relLine) return fail(needRelease(`${explicit}`));
      if (lineOf(base) !== relLine) return fail(`${explicit} does not belong to ${branch}: expected ${relLine}.x`);
      return { version: explicit, tag: `v${explicit}`, channel: "beta", errors: [] };
    }
    // explicit stable / hotfix
    if (!relLine) return fail(`stable releases are cut from release/${lineOf(base)} (on ${branch})`);
    if (lineOf(base) !== relLine) return fail(`${explicit} does not belong to ${branch}: expected ${relLine}.x`);
    return stable(explicit, p);
  }

  // --- named requests ------------------------------------------------------------------------------------
  switch (request) {
    case "alpha": {
      if (!onMain) return fail(needMainForAlpha);
      const base = mainBase();
      return finish(fmt(base, "alpha", highest(base, "alpha") + 1));
    }
    case "beta": {
      if (onMain) {
        const base = mainBase();
        return finish(fmt(base, "beta", 1), { createBranch: `release/${lineOf(base)}` });
      }
      if (!relLine) return fail(`beta releases are cut from main (first beta) or release/X.Y (on ${branch})`);
      const base = relBase(relLine);
      if (hasLabel(base, "rc")) return fail(`${base} already has a release candidate (rc); continue with rc or stable, not beta`);
      return finish(fmt(base, "beta", highest(base, "beta") + 1));
    }
    case "rc": {
      if (!relLine) return fail(`release candidates (rc) are cut from release/X.Y (on ${branch})`);
      const base = relBase(relLine);
      if (!hasLabel(base, "beta")) return fail(`${base} has no beta yet: cut a beta before an rc`);
      return finish(fmt(base, "rc", highest(base, "rc") + 1));
    }
    case "stable": {
      if (!relLine) return fail(`stable releases are cut from release/X.Y, promoting a soaked beta (on ${branch})`);
      const base = relBase(relLine);
      return stable(base, parse(base)!);
    }
    case "patch": {
      if (!relLine) return fail(`patch is a hotfix and is cut from release/X.Y, not ${branch}: fix on main, git cherry-pick -x onto release/X.Y, then bun run release patch there. For new work use alpha / beta`);
      const [ma, mi] = relLine.split(".").map(Number);
      const zero = `${ma}.${mi}.0`;
      if (!stableExists(zero)) return fail(`release/${relLine} has no stable ${zero} yet: promote the beta with "bun run release stable" first`);
      const top = known.filter((t) => !t.p.label && t.p.major === ma && t.p.minor === mi).reduce((m, t) => Math.max(m, t.p.patch), 0);
      return finish(`${ma}.${mi}.${top + 1}`);
    }
    case "minor":
    case "major":
      return fail(`"${request}" is gone: versions now move through channels. Use "alpha" (next alpha from main), "beta" (first beta branches release/X.Y), then "stable" on release/X.Y${request === "major" ? '; to start a new major line use "alpha --bump major"' : ""}`);
    default:
      return fail(`unknown request "${request}": use alpha, beta, rc, stable, patch or an explicit X.Y.Z[-(alpha|beta|rc).N]`);
  }

  /** `X.Y.0` of a release branch; patch releases never start a line. */
  function relBase(line: string): string {
    return `${line}.0`;
  }

  /** Stable promotion (X.Y.0) or hotfix (X.Y.Z, Z>0) on release/X.Y. */
  function stable(version: string, p: Parsed): NextResult {
    if (p.patch > 0) {
      // An explicit hotfix version: the same preconditions as `patch`.
      if (!stableExists(`${p.major}.${p.minor}.0`)) return fail(`release/${p.major}.${p.minor} has no stable ${p.major}.${p.minor}.0 yet`);
      return finish(version);
    }
    const base = version;
    if (!hasLabel(base, "beta", "rc")) {
      return fail(hasLabel(base, "alpha") ? `${base} only has alpha builds: cut a beta (and soak it) before stable` : `no prerelease tag of ${base} (beta or rc) to promote: cut a beta first`);
    }
    if (stableExists(base)) return fail(`tag v${base} already exists`);
    const notes: string[] = [];
    if (!lastPrereleaseTagDate) {
      if (!forceSoak) return fail(`cannot tell when the last prerelease of ${base} was tagged: soak cannot be checked (override with --force-soak)`);
      notes.push(`Soak not verified: the date of the last ${base} prerelease was unavailable (forced with --force-soak).`);
    } else {
      const hours = (now.getTime() - new Date(lastPrereleaseTagDate).getTime()) / 3600_000;
      if (hours < SOAK_HOURS) {
        const shown = Math.max(0, Math.floor(hours));
        if (!forceSoak) return fail(`soak not met: the last ${base} prerelease is ${shown}h old, stable needs at least ${SOAK_HOURS}h (override with --force-soak; it is recorded in the release notes)`);
        notes.push(`Soak override: promoted after ${shown}h on beta/rc, below the ${SOAK_HOURS}h minimum (forced with --force-soak).`);
      }
    }
    const r = finish(base);
    return r.errors.length ? r : { ...r, notes };
  }
}

/** Files a promotion commit may change relative to the prerelease it promotes. Returns the offending ones. */
export function diffAllowed(files: string[]): string[] {
  const ok = /^(Cargo\.toml|Cargo\.lock|CHANGELOG\.md|crates\/[^/]+\/Cargo\.toml)$/;
  return files.filter((f) => !ok.test(f));
}

// --- CLI -----------------------------------------------------------------------------------------------------

async function git(...args: string[]): Promise<string> {
  const p = Bun.spawn(["git", ...args], { stdout: "pipe", stderr: "pipe" });
  const [out, code] = await Promise.all([new Response(p.stdout).text(), p.exited]);
  if (code !== 0) throw new Error(`git ${args.join(" ")} failed: ${(await new Response(p.stderr).text()).trim()}`);
  return out.trim();
}

const lines = (s: string) => s.split("\n").filter(Boolean);

async function readState(request: string, flags: { forceSoak: boolean; bump?: "major" }): Promise<NextInput> {
  const cargo = await Bun.file("crates/atlas-cli/Cargo.toml").text();
  const current = /^version = "(.*)"/m.exec(cargo)?.[1] ?? "";
  const remoteTags = lines(await git("ls-remote", "--tags", "--refs", "origin", "refs/tags/v*").catch(() => "")).map((l) => l.split("refs/tags/")[1]);
  const tags = [...new Set([...lines(await git("tag", "-l", "v*")), ...remoteTags])];
  const branch = await git("rev-parse", "--abbrev-ref", "HEAD");
  const heads = lines(await git("for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes/origin"));
  const branches = [...new Set(heads.map((b) => b.replace(/^origin\//, "")))];
  const state: NextInput = { request, current, tags, branch, branches, now: new Date(), forceSoak: flags.forceSoak, bump: flags.bump };
  // Soak date: newest beta/rc tag of the base being promoted (a release branch names it).
  const line = /^release\/(\d+\.\d+)$/.exec(branch)?.[1];
  if (line) {
    const newest = parsedTags(tags)
      .filter((t) => lineOf(baseOf(t.p)) === line && (t.p.label === "beta" || t.p.label === "rc"))
      .map((t) => `v${t.v}`)
      .sort((a, b) => Bun.semver.order(a.slice(1), b.slice(1)))
      .at(-1);
    if (newest) state.lastPrereleaseTagDate = (await git("for-each-ref", "--format=%(creatordate:iso-strict)", `refs/tags/${newest}`)) || undefined;
  }
  return state;
}

function arg(argv: string[], name: string): string | undefined {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : undefined;
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  const [cmd] = argv;
  const json = argv.includes("--json");
  if (cmd === "next") {
    const request = arg(argv, "--request") ?? "";
    const bump = arg(argv, "--bump");
    if (bump !== undefined && bump !== "major") {
      console.error(`✗ --bump only accepts "major" (got "${bump}")`);
      process.exit(2);
    }
    const result = nextVersion(await readState(request, { forceSoak: argv.includes("--force-soak"), bump: bump === "major" ? "major" : undefined }));
    if (json) console.log(JSON.stringify(result));
    else if (result.errors.length) for (const e of result.errors) console.error(`✗ ${e}`);
    else console.log(result.version);
    process.exit(result.errors.length ? 1 : 0);
  } else if (cmd === "diff-guard") {
    const from = arg(argv, "--from");
    if (!from) {
      console.error("usage: release-version.ts diff-guard --from <tag>");
      process.exit(2);
    }
    const offending = diffAllowed(lines(await git("diff", "--name-only", `${from}..HEAD`)));
    if (json) console.log(JSON.stringify({ from, offending }));
    else for (const f of offending) console.error(f);
    process.exit(offending.length ? 1 : 0);
  } else if (cmd === "github-flags") {
    // `release-version.ts github-flags --tag v1.5.0-alpha.4` -> prerelease=false / make_latest=true (GITHUB_OUTPUT lines).
    const tag = arg(argv, "--tag");
    if (!tag) {
      console.error("usage: release-version.ts github-flags --tag <tag>");
      process.exit(2);
    }
    const channels = await Bun.file(new URL("../crates/atlas-core/data/channels.json", import.meta.url)).json();
    const flags = githubFlags(tag, readDefaultChannel(channels));
    console.log(`prerelease=${flags.prerelease}`);
    console.log(`make_latest=${flags.makeLatest}`);
  } else if (cmd === "last-prerelease") {
    // The newest beta/rc tag of the line being promoted, for the diff guard and ATLAS_PROMOTED_FROM.
    const state = await readState("stable", { forceSoak: false });
    const line = /^release\/(\d+\.\d+)$/.exec(state.branch)?.[1];
    const tag = parsedTags(state.tags)
      .filter((t) => line && lineOf(baseOf(t.p)) === line && (t.p.label === "beta" || t.p.label === "rc"))
      .map((t) => `v${t.v}`)
      .sort((a, b) => Bun.semver.order(a.slice(1), b.slice(1)))
      .at(-1);
    if (!tag) process.exit(1);
    console.log(tag);
  } else {
    console.error("usage: release-version.ts next --request <req> [--json] [--force-soak] [--bump major] | diff-guard --from <tag> [--json] | last-prerelease | github-flags --tag <tag>");
    process.exit(2);
  }
}
