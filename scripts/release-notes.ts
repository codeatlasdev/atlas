// The GitHub release body: a header, this version's CHANGELOG.md section, and the install/update footer.
//
// GitHub cuts a release body at 125,000 characters without a word. Incident (1.5.0-alpha.26, 2026-11-16): the
// section was 154,762 characters, the published page stopped mid-sentence in the Fixed list and lost the whole
// Install / Update footer. When the full body does not fit, the section is shortened to its intro, its headings and
// each entry's bold title, with a link to the full section in CHANGELOG.md at the tag. The app's What's new reads
// CHANGELOG.md at the tag (atlas-core github.rs), so nothing is lost there.

/** Under GitHub's 125,000-character cap, with room for what GitHub itself adds. */
export const BODY_LIMIT = 120_000;

/** The lines under `## [version]` up to the next `## ` heading, each ending in a newline: byte for byte what
 *  release.yml's `awk '/^## /{p = index($0, "[" v "]") > 0; next} p'` printed. */
export function sectionOf(changelog: string, version: string): string {
  const out: string[] = [];
  let inside = false;
  for (const line of changelog.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n")) {
    if (line.startsWith("## ")) {
      inside = line.includes(`[${version}]`);
      continue;
    }
    if (inside) out.push(line);
  }
  return out.map((l) => `${l}\n`).join("");
}

/** The section with every entry reduced to its bold title, and a pointer to the full text. */
function shortened(section: string, repo: string, version: string): string {
  const out: string[] = [];
  let entry = false;
  for (const line of section.split("\n")) {
    if (line.startsWith("### ")) {
      entry = false;
      out.push("", line, "");
    } else if (/^- /.test(line)) {
      entry = true;
      const title = /^- (\*\*.+?\*\*)/.exec(line)?.[1];
      out.push(`- ${title ?? line.slice(2, 120)}`);
    } else if (!entry && !out.some((l) => l.startsWith("### "))) {
      out.push(line); // the intro, before the first heading
    }
  }
  const link = `https://github.com/${repo}/blob/v${version}/CHANGELOG.md`;
  const note = `> These notes are too long for a GitHub release, so each entry shows its title only. The full text of every entry: [CHANGELOG.md at v${version}](${link}) (Atlas Desktop's What's new shows it too).`;
  return [note, "", ...out].join("\n").replace(/\n{3,}/g, "\n\n");
}

/** header + section + footer, within [limit]; the section is shortened when the whole would not fit. */
export function releaseBody(
  changelog: string,
  version: string,
  header: string,
  footer: string,
  { repo = "codeatlasdev/atlas", limit = BODY_LIMIT } = {},
): string {
  const section = sectionOf(changelog, version);
  const full = header + section + footer;
  if (full.length <= limit) return full;
  const short = header + shortened(section, repo, version) + footer;
  if (short.length <= limit) return short;
  // Hundreds of entries: the pointer alone, never a body GitHub would cut.
  return `${header}${shortened("", repo, version)}${footer}`;
}

if (import.meta.main) {
  const [version, headerPath, footerPath] = process.argv.slice(2);
  if (!version || !headerPath || !footerPath) {
    console.error("usage: bun scripts/release-notes.ts <version> <header.md> <footer.md>");
    process.exit(2);
  }
  const changelog = await Bun.file("CHANGELOG.md").text();
  if (!sectionOf(changelog, version).trim()) {
    console.error(`CHANGELOG.md has no [${version}] section`);
    process.exit(1);
  }
  const [header, footer] = await Promise.all([Bun.file(headerPath).text(), Bun.file(footerPath).text()]);
  process.stdout.write(releaseBody(changelog, version, header, footer));
}
