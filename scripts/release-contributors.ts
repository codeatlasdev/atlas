// Who worked on a release, for the "## Contributors" section of its GitHub page (GitHub shows the avatars of the
// people a release body @mentions).
//
// Nobody may be left out, so this fails closed: every commit's author and every `Co-authored-by` must resolve to a
// GitHub login (the commit's own account, a users.noreply address, or an alias in .github/release-contributors.json),
// else it exits 1 naming the commit. Incident (1.5.0-alpha.26): the release page listed nobody, and two commits on
// main were authored "First User <first@example.com>" by an agent session, with no account behind them.
//
//   bun scripts/release-contributors.ts <version> <ref> [--check]
//   <ref>: the tag (release workflow) or the commit about to be tagged (release.sh, already pushed).
//   prints the markdown section; --check prints nothing on success.

export interface Person {
  name: string;
  email: string;
}

export interface CommitInfo {
  sha: string;
  /** GitHub login of the commit author, when GitHub linked one. */
  login: string | null;
  author: Person;
  coAuthors: Person[];
  /** Authors and reviewers of the pull requests the commit came from. */
  prPeople: string[];
}

export interface Config {
  always: string[];
  aliases: Record<string, string>;
}

const isBot = (login: string) => /\[bot\]$/i.test(login) || /^(github-actions|web-flow|dependabot)$/i.test(login);

/** A GitHub login for an identity, or null when nothing ties it to an account. */
export function loginOf(p: Person, aliases: Record<string, string>): string | null {
  const email = p.email.trim().toLowerCase();
  const alias = Object.entries(aliases).find(([k]) => k.toLowerCase() === email)?.[1];
  if (alias) return alias;
  const noreply = /^(?:\d+\+)?([^@]+)@users\.noreply\.github\.com$/.exec(email);
  return noreply ? noreply[1] : null;
}

/** The people of a commit range, without bots, and the identities that resolve to nobody. */
export function contributorsOf(commits: CommitInfo[], config: Config): { logins: string[]; unmapped: string[] } {
  const seen = new Map<string, string>(); // lowercased -> as written
  const unmapped: string[] = [];
  const add = (login: string) => {
    if (!isBot(login) && !seen.has(login.toLowerCase())) seen.set(login.toLowerCase(), login);
  };
  // Emails GitHub linked to an account on some commit of the range: the same email in a trailer is that person.
  const linked = { ...config.aliases };
  for (const c of commits) if (c.login) linked[c.author.email.toLowerCase()] ??= c.login;
  for (const c of commits) {
    const author = c.login ?? loginOf(c.author, linked);
    if (author) add(author);
    else unmapped.push(`${c.sha.slice(0, 8)} author ${c.author.name} <${c.author.email}>`);
    for (const co of c.coAuthors) {
      const login = loginOf(co, linked);
      if (login) add(login);
      else if (!/\[bot\]/i.test(co.name)) unmapped.push(`${c.sha.slice(0, 8)} co-author ${co.name} <${co.email}>`);
    }
    for (const p of c.prPeople) add(p);
  }
  const logins = [...seen.values()].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
  return { logins, unmapped };
}

const list = (xs: string[]) =>
  xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`;

/** The markdown section: everyone who worked on it, then the people credited in every release. */
export function contributorsSection(logins: string[], always: string[]): string {
  const special = new Set(always.map((a) => a.toLowerCase()));
  const team = logins.filter((l) => !special.has(l.toLowerCase())).map((l) => `@${l}`);
  const lines = ["", "## Contributors", ""];
  if (team.length) lines.push(`${list(team)} worked on this release.`, "");
  if (always.length) lines.push(`Special thanks to ${list(always.map((a) => `@${a}`))}.`, "");
  return lines.join("\n");
}

/** `Co-authored-by: Name <email>` trailers of a commit message. */
export function coAuthorsOf(message: string): Person[] {
  return [...message.matchAll(/^Co-authored-by:\s*(.+?)\s*<([^>]+)>\s*$/gim)].map((m) => ({ name: m[1], email: m[2] }));
}

async function gh(path: string): Promise<unknown[]> {
  const out = await Bun.$`gh api ${path} --paginate --slurp`.quiet().text();
  return (JSON.parse(out) as unknown[]).flat();
}

/** The highest `vX.Y.Z[-pre.N]` tag below [version]: the release this one follows. */
async function previousTag(repo: string, version: string): Promise<string> {
  const tags = (await gh(`repos/${repo}/tags?per_page=100`)) as { name: string }[];
  const versions = tags
    .map((t) => t.name)
    .filter((n) => /^v\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/.test(n) && Bun.semver.order(n.slice(1), version) < 0)
    .sort((a, b) => Bun.semver.order(b.slice(1), a.slice(1)));
  if (!versions[0]) throw new Error(`no release tag before ${version}`);
  return versions[0];
}

async function commitsBetween(repo: string, from: string, to: string): Promise<CommitInfo[]> {
  const pages = (await gh(`repos/${repo}/compare/${from}...${to}?per_page=100`)) as {
    commits: { sha: string; author: { login: string } | null; commit: { author: Person; message: string } }[];
  }[];
  const raw = pages.flatMap((p) => p.commits);
  return Promise.all(
    raw.map(async (c) => {
      const prs = (await gh(`repos/${repo}/commits/${c.sha}/pulls`)) as { number: number; user: { login: string } }[];
      const prPeople: string[] = [];
      for (const pr of prs) {
        prPeople.push(pr.user.login);
        const reviews = (await gh(`repos/${repo}/pulls/${pr.number}/reviews`)) as { user: { login: string } | null }[];
        for (const r of reviews) if (r.user) prPeople.push(r.user.login);
      }
      return {
        sha: c.sha,
        login: c.author?.login ?? null,
        author: c.commit.author,
        coAuthors: coAuthorsOf(c.commit.message),
        prPeople,
      };
    }),
  );
}

if (import.meta.main) {
  const [version, ref, flag] = process.argv.slice(2);
  if (!version || !ref) {
    console.error("usage: bun scripts/release-contributors.ts <version> <ref> [--check]");
    process.exit(2);
  }
  const repo = process.env.GITHUB_REPOSITORY ?? "codeatlasdev/atlas";
  const config = (await Bun.file(`${import.meta.dir}/../.github/release-contributors.json`).json()) as Config;
  const from = await previousTag(repo, version);
  const { logins, unmapped } = contributorsOf(await commitsBetween(repo, from, ref), config);
  if (unmapped.length) {
    console.error(
      `✗ ${from}..${ref}: these commit identities have no GitHub account, so their people would not be credited:\n` +
        unmapped.map((u) => `  ${u}`).join("\n") +
        "\nAdd the person's login under \"aliases\" in .github/release-contributors.json.",
    );
    process.exit(1);
  }
  if (flag === "--check") {
    console.error(`✓ contributors ${from}..${ref}: ${logins.join(", ") || "(none)"}`);
  } else {
    process.stdout.write(contributorsSection(logins, config.always));
  }
}
