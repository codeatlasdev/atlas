// The git-level backstop: whatever tool runs `git push`, git calls the pre-push hook with one line per ref on stdin
// (`<local ref> <local sha> <remote ref> <remote sha>`, githooks(5)). Same rules and same override as the tool guards.
import { type Ctx, OVERRIDE_ENV } from "./context";
import type { Action, Verdict } from "./decide";

export interface PushRef {
	localSha: string;
	remoteRef: string;
	remoteSha: string;
}

const isZero = (sha: string) => /^0+$/.test(sha);

export function parsePushLines(stdin: string): PushRef[] {
	return stdin
		.split("\n")
		.map((l) => l.trim().split(/\s+/))
		.filter((f) => f.length === 4)
		.map(([, localSha, remoteRef, remoteSha]) => ({ localSha, remoteRef, remoteSha }));
}

/** `isAncestor(a, b)`: commit `a` is reachable from `b`. */
export function decidePush(refs: PushRef[], ctx: Ctx, isAncestor: (a: string, b: string) => boolean): Verdict {
	const refuse = (action: Action, message: string): Verdict | undefined =>
		ctx.override.has("all") || ctx.override.has(action)
			? undefined
			: { allow: false, action, message: `${message}\nMaintainer override (the maintainer sets it for the one command): ${OVERRIDE_ENV}=${action} git push …` };
	for (const r of refs) {
		const branch = r.remoteRef.startsWith("refs/heads/") ? r.remoteRef.slice("refs/heads/".length) : undefined;
		const tag = r.remoteRef.startsWith("refs/tags/");
		let v: Verdict | undefined;
		if (branch !== undefined && (/^(main|master)$/.test(branch) || branch.startsWith("release/"))) {
			v = refuse("push", `refused: push to ${branch}. main and release/* change only through a reviewed pull request: push a branch (git push -u origin <type/short-desc>) and open a draft PR (gh pr create --draft).`);
		} else if (isZero(r.localSha)) {
			v = refuse("delete", `refused: deleting ${r.remoteRef} on the remote is a maintainer action.`);
		} else if (tag && !isZero(r.remoteSha) && r.remoteSha !== r.localSha) {
			v = refuse("delete", `refused: moving the tag ${r.remoteRef.slice("refs/tags/".length)}; v* tags are immutable, create a new tag.`);
		} else if (branch !== undefined && !isZero(r.remoteSha) && r.remoteSha !== r.localSha && !isAncestor(r.remoteSha, r.localSha)) {
			v = refuse("force-push", `refused: ${branch} on the remote has commits your push would drop (this is a force-push, or your clone is behind: git fetch and rebase first). Do not rewrite remote history.`);
		}
		if (v) return v;
	}
	return { allow: true };
}
