/**
 * "Report issue" looks for an open issue with the error's fingerprint before it opens a new one (#320). The search is
 * the backend's (`crates/atlas-desktop/src/issue_search.rs`: the page's CSP bars `api.github.com`); this reads its
 * answer defensively and turns a found issue into the "me too" link. Any answer but a found issue means "open the
 * new-issue form", which is what the button always did: a search that fails never blocks a report.
 */
import { REPO_URL } from "./report.ts";

export interface FoundIssue {
	kind: "found";
	number: number;
	url: string;
	title: string;
}
export type Lookup = FoundIssue | { kind: "none" } | { kind: "unavailable" };

const ISSUE_URL = new RegExp(`^${REPO_URL.replaceAll(".", "\\.")}/issues/\\d+$`);

/** The lookup out of whatever the command returned; anything malformed is `unavailable`, and so is a link that is not an issue of this repo. */
export function parseLookup(raw: unknown): Lookup {
	const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
	if (r.kind === "none") return { kind: "none" };
	if (
		r.kind === "found" &&
		typeof r.number === "number" &&
		Number.isSafeInteger(r.number) &&
		typeof r.title === "string" &&
		typeof r.url === "string" &&
		ISSUE_URL.test(r.url)
	)
		return { kind: "found", number: r.number, url: r.url, title: r.title };
	return { kind: "unavailable" };
}

/** Ask `search` (the `issue_search` command) for `fingerprint`; never throws. */
export async function lookupIssue(
	fingerprint: string,
	search: (fp: string) => Promise<unknown>,
): Promise<Lookup> {
	try {
		return parseLookup(await search(fingerprint));
	} catch {
		return { kind: "unavailable" };
	}
}

/**
 * The issue at its comment box. GitHub has no link that prefills a comment, so the report is copied to the clipboard
 * when this is opened (`ErrorPanel`) and the person pastes it under "Add a comment".
 */
export const meTooUrl = (issue: FoundIssue) => `${issue.url}#issuecomment-new`;
