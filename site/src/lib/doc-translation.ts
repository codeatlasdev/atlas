// Docs translation tracking, pure (no `astro:` imports). A translation in docs/pt-BR/<slug>.md names the English file it
// translates (`source`) and the git BLOB sha of that file at translation time (`source_sha`). A blob sha is a content hash:
// it survives squash merges and needs no history. `git diff <source_sha> <current blob>` shows what changed when the old blob exists.
import { createHash } from "node:crypto";

export const SOURCE_SHA = /^[0-9a-f]{40}$/;
export const SOURCE_FILE = /^[a-z0-9-]+\.md$/;
/** `translation: machine`: the page was translated by a model and no person reviewed it. */
export const MACHINE = "machine";
/** `translation: pending` (with an `issue` link): the English changed and the translation will follow. The page is outdated until
 * `source_sha` matches again, so it shows the outdated notice; a pending flag on a current page changes nothing. */
export const PENDING = "pending";
export const ISSUE_URL = /^https:\/\/github\.com\/[\w.-]+\/[\w.-]+\/issues\/\d+$/;

/** Equal to `git hash-object --no-filters <file>`: sha1 of "blob <byte length>\0" + bytes. */
export function gitBlobSha(bytes: Uint8Array): string {
	return createHash("sha1").update(`blob ${bytes.byteLength}\0`).update(bytes).digest("hex");
}

/**
 * `missing`: no translation. `ok`: translated from the current English file and reviewed. `outdated`: the English file changed
 * since. `machine`: current but `translation: machine` (AI-translated, not reviewed). Outdated wins over machine: the page needs
 * work first, and its banner already says details may be wrong.
 */
export type DocState = "missing" | "ok" | "outdated" | "machine";

export function docState(translation: { source_sha: string; translation?: typeof MACHINE | typeof PENDING } | undefined, currentSha: string): DocState {
	if (!translation) return "missing";
	if (translation.source_sha !== currentSha) return "outdated";
	return translation.translation === MACHINE ? "machine" : "ok";
}

const REPO = "https://github.com/codeatlasdev/atlas";
/** GitHub has no link to a blob-to-blob diff; the history of the English file is the closest: pick the commits since the translation. */
export const sourceHistoryUrl = (slug: string): string => `${REPO}/commits/main/docs/${slug}.md`;
