// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { CREW_ANCHORS, CREW_DOCS_URL, fixDocs, signInDocs, stepDocs } from "./docsLinks.ts";

// The links of the onboarding are only as good as the headings they point at: read docs/crew.md and slug its
// headings the way GitHub (and the site's renderer) does.
const md = readFileSync(join(import.meta.dir, "../../../../../../docs/crew.md"), "utf8");
function slug(h: string) {
	return h
		.trim()
		.toLowerCase()
		.replace(/`/g, "")
		.replace(/[^\p{L}\p{N}\- _]/gu, "")
		.replace(/ /g, "-");
}
const headings = new Set(
	md
		.split("\n")
		.filter((l) => /^#{1,6} /.test(l))
		.map((l) => slug(l.replace(/^#+ /, ""))),
);

describe("onboarding docs links", () => {
	test("every anchor of the map is a heading of docs/crew.md", () => {
		for (const [name, anchor] of Object.entries(CREW_ANCHORS)) {
			expect(headings.has(anchor), `${name} -> #${anchor}`).toBe(true);
		}
	});

	test("every step has a link, on the crew docs page", () => {
		for (const s of ["intro", "how", "connect", "engine", "ready"]) {
			expect(stepDocs(s).startsWith(CREW_DOCS_URL), s).toBe(true);
		}
		expect(stepDocs("connect")).toBe(`${CREW_DOCS_URL}#install-a-crew-server`);
		expect(stepDocs("engine")).toBe(`${CREW_DOCS_URL}#sign-agents-in`);
		expect(stepDocs("ready")).toBe(`${CREW_DOCS_URL}#crewmates`);
		expect(fixDocs()).toContain("#add-a-crew-server-fails");
		expect(signInDocs()).toContain("#add-a-crew-server-in-atlas-desktop");
	});

	// The troubleshooting section answers each failure the screen can name (the kinds of `ConnectErrorKind`).
	test("the troubleshooting section has a row for every failure the form explains", () => {
		const section = md.slice(md.indexOf("### Add a crew server fails"));
		for (const needle of [
			"refused the key",
			"does not accept passwords",
			"asks for a password",
			"is masked",
			"systemd",
			"OpenSSH Client",
			"host key",
		]) {
			expect(section.includes(needle), needle).toBe(true);
		}
	});
});
