import { describe, expect, test } from "bun:test";
import { changelogProblems } from "./changelog-lint.ts";

const lines = (s: string) => changelogProblems(s).map((p) => p.line);

describe("changelogProblems", () => {
  // Incident (1.5.0-alpha.26 notes): the union merge driver left three versions of one entry in Unreleased.
  test("an entry kept twice by a merge is flagged where the copy is", () => {
    const text = [
      "## [Unreleased]",
      "### Fixed",
      "- **Isolation is on.** The newest text, ten fixes.",
      "- **Other.** body",
      "- **Isolation is on.** An older text, six fixes.",
    ].join("\n");
    expect(lines(text)).toEqual([5]);
  });

  test("the same title in two versions is not a duplicate", () => {
    const text = ["## [1.1.0]", "- **Faster start.** a", "## [1.0.0]", "- **Faster start.** b"].join("\n");
    expect(changelogProblems(text)).toEqual([]);
  });

  test("a heading repeated inside one version is flagged", () => {
    const text = ["## [1.0.0]", "### Fixed", "- **A.** a", "### Fixed", "- **B.** b"].join("\n");
    expect(lines(text)).toEqual([4]);
  });

  test("a link labelled with one number that points to another is flagged", () => {
    const text = [
      "## [1.0.0]",
      "- **A.** ([#437](https://github.com/o/r/issues/443)), ([#12](https://github.com/o/r/pull/12))",
    ].join("\n");
    expect(changelogProblems(text)).toEqual([{ line: 2, message: "link labelled #437 points to #443" }]);
  });

  test("the repository's CHANGELOG.md is clean", async () => {
    const problems = changelogProblems(await Bun.file(`${import.meta.dir}/../CHANGELOG.md`).text());
    expect(problems.map((p) => `CHANGELOG.md:${p.line}: ${p.message}`)).toEqual([]);
  });
});
