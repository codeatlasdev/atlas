import { describe, expect, test } from "bun:test";
import { BODY_LIMIT, releaseBody, sectionOf } from "./release-notes.ts";

const CHANGELOG = [
  "# Changelog",
  "",
  "## [Unreleased]",
  "",
  "## [1.2.0] — 2026-01-02",
  "",
  "**The big one.** Intro paragraph.",
  "",
  "### Added",
  "",
  "- **Thing one.** " + "a".repeat(400),
  "- **Thing two ([#9](https://github.com/o/r/issues/9)).** " + "b".repeat(400),
  "",
  "### Fixed",
  "",
  "- **A fix.** " + "c".repeat(400),
  "",
  "## [1.1.0] — 2026-01-01",
  "",
  "- **Old.** never shown",
].join("\n");

const HEADER = "> alpha note\n\n";
const FOOTER = "\n## Install\n\ncurl … | bash\n\n## Update\n\natlas update\n";

describe("releaseBody", () => {
  test("a section that fits is the CHANGELOG text verbatim, between the header and the footer", () => {
    const body = releaseBody(CHANGELOG, "1.2.0", HEADER, FOOTER);
    expect(body).toBe(HEADER + sectionOf(CHANGELOG, "1.2.0") + FOOTER);
    expect(body).not.toContain("Old.");
    expect(body).toContain("a".repeat(400));
  });

  // Incident (1.5.0-alpha.26): GitHub cut a 154,762-character body mid-entry and dropped the Install footer.
  test("a section too long for GitHub keeps the intro, every heading and title, the link and the footer", () => {
    const body = releaseBody(CHANGELOG, "1.2.0", HEADER, FOOTER, { repo: "o/r", limit: 1000 });
    expect(body.length).toBeLessThanOrEqual(1000);
    expect(body.startsWith(HEADER)).toBe(true);
    expect(body.endsWith(FOOTER)).toBe(true);
    for (const kept of ["**The big one.** Intro paragraph.", "### Added", "### Fixed", "- **Thing one.**", "- **A fix.**"])
      expect(body).toContain(kept);
    expect(body).toContain("- **Thing two ([#9](https://github.com/o/r/issues/9)).**");
    expect(body).toContain("https://github.com/o/r/blob/v1.2.0/CHANGELOG.md");
    expect(body).not.toContain("a".repeat(400));
  });

  test("when even the titles do not fit, the body is the pointer and the footer, never over the limit", () => {
    const body = releaseBody(CHANGELOG, "1.2.0", HEADER, FOOTER, { repo: "o/r", limit: 520 });
    expect(body.length).toBeLessThanOrEqual(520);
    expect(body).toContain("https://github.com/o/r/blob/v1.2.0/CHANGELOG.md");
    expect(body.endsWith(FOOTER)).toBe(true);
  });

  test("the repository's real 1.5.0-alpha.26 notes fit GitHub with their footer", async () => {
    const real = await Bun.file(`${import.meta.dir}/../CHANGELOG.md`).text();
    const body = releaseBody(real, "1.5.0-alpha.26", HEADER, FOOTER);
    expect(body.length).toBeLessThanOrEqual(BODY_LIMIT);
    expect(body.endsWith(FOOTER)).toBe(true);
    expect(body).toContain("Re-run `atlas crew install` on every crew server");
  });
});
