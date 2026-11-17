import { describe, expect, test } from "bun:test";
import { coAuthorsOf, type CommitInfo, contributorsOf, contributorsSection, loginOf } from "./release-contributors.ts";

const config = { always: ["FabioHAraujo", "Richards0nd"], aliases: { "first@example.com": "mvilacad" } };
const commit = (over: Partial<CommitInfo>): CommitInfo => ({
  sha: "0123456789abcdef",
  login: null,
  author: { name: "Someone", email: "someone@example.org" },
  coAuthors: [],
  prPeople: [],
  ...over,
});

describe("who worked on a release", () => {
  test("a noreply address names its account, with or without the numeric id", () => {
    expect(loginOf({ name: "M", email: "34039851+mvilacad@users.noreply.github.com" }, {})).toBe("mvilacad");
    expect(loginOf({ name: "L", email: "luancoleto@users.noreply.github.com" }, {})).toBe("luancoleto");
    expect(loginOf({ name: "X", email: "x@example.org" }, {})).toBeNull();
  });

  // Incident (1.5.0-alpha.26): two commits on main were authored "First User <first@example.com>" by an agent.
  test("a commit nobody can be tied to is reported, never dropped silently", () => {
    const { unmapped } = contributorsOf([commit({ author: { name: "First User", email: "first@other.dev" } })], config);
    expect(unmapped).toEqual(["01234567 author First User <first@other.dev>"]);
  });

  test("an alias ties an agent identity to its person", () => {
    const { logins, unmapped } = contributorsOf(
      [commit({ author: { name: "First User", email: "FIRST@example.com" } })],
      config,
    );
    expect(unmapped).toEqual([]);
    expect(logins).toEqual(["mvilacad"]);
  });

  test("co-authors and the PR's author and reviewers count; bots and duplicates do not", () => {
    const { logins, unmapped } = contributorsOf(
      [
        commit({
          login: "mvilacad",
          coAuthors: [
            { name: "Luan", email: "1+luancoleto@users.noreply.github.com" },
            { name: "dependabot[bot]", email: "49699333+dependabot[bot]@users.noreply.github.com" },
          ],
          prPeople: ["Richards0nd", "MVilacad", "github-actions[bot]"],
        }),
        commit({ login: "dependabot[bot]" }),
      ],
      config,
    );
    expect(unmapped).toEqual([]);
    expect(logins).toEqual(["luancoleto", "mvilacad", "Richards0nd"]);
  });

  // Incident (alpha.26 run): a Co-authored-by with the maintainer's own email (no noreply) was reported unmapped,
  // although GitHub linked that same email to his account on his other commits in the range.
  test("a co-author email GitHub already linked to an account on another commit resolves to it", () => {
    const { logins, unmapped } = contributorsOf(
      [
        commit({ login: "mvilacad", author: { name: "Matheus", email: "matheus@codeatlas.com.br" } }),
        commit({
          login: "luancoleto",
          coAuthors: [{ name: "Matheus Vilaça de Jesus", email: "Matheus@CodeAtlas.com.br" }],
        }),
      ],
      config,
    );
    expect(unmapped).toEqual([]);
    expect(logins).toEqual(["luancoleto", "mvilacad"]);
  });

  test("a co-author with no account is reported like an author", () => {
    const { unmapped } = contributorsOf(
      [commit({ login: "mvilacad", coAuthors: [{ name: "Pair", email: "pair@example.org" }] })],
      config,
    );
    expect(unmapped).toEqual(["01234567 co-author Pair <pair@example.org>"]);
  });

  test("Co-authored-by trailers are read from the message", () => {
    expect(coAuthorsOf("fix: x\n\nbody\n\nCo-authored-by: A B <a@b.c>\nco-authored-by: C <c@d.e>\nSigned-off-by: Z <z@z.z>")).toEqual(
      [
        { name: "A B", email: "a@b.c" },
        { name: "C", email: "c@d.e" },
      ],
    );
  });
});

describe("the section", () => {
  test("names everyone with an @mention and always thanks the special contributors, once each", () => {
    const md = contributorsSection(["luancoleto", "mvilacad", "Richards0nd"], config.always);
    expect(md).toContain("## Contributors");
    expect(md).toContain("@luancoleto and @mvilacad worked on this release.");
    expect(md).toContain("Special thanks to @FabioHAraujo and @Richards0nd.");
    expect(md.match(/@Richards0nd/g)?.length).toBe(1);
  });

  test("the special thanks are there even when nobody else is", () => {
    const md = contributorsSection([], config.always);
    expect(md).not.toContain("worked on this release");
    expect(md).toContain("Special thanks to @FabioHAraujo and @Richards0nd.");
  });
});
