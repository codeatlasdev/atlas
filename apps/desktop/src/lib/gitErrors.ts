/**
 * What a failed git command means, in a sentence. Git's own answer to a wrong state is a wall of text (a bare
 * `git diff` outside a repository prints the whole `--no-index` usage, 100+ lines); the app showed it as a toast
 * (#505). Every failure that reaches the user goes through `classifyGitError`: a known error becomes one sentence from
 * the catalog; an unknown one gets one generic sentence. Git's own output is never the sentence, it stays available as
 * `details` (what the "Details" button shows). Pure: no Svelte, no IPC.
 */
import { m } from "#lib/i18n/m.ts";

export type GitErrorKind =
	| "not-repo"
	| "no-commits"
	| "no-remote"
	| "no-upstream"
	| "remote-access"
	| "rejected"
	| "conflict"
	| "local-changes"
	| "nothing-to-commit"
	| "locked"
	| "other";

export interface GitProblem {
	kind: GitErrorKind;
	/** One sentence from the catalog, safe for a toast or a panel. */
	message: string;
	/** Git's own output without the `git <args>:` prefix the host adds; empty when git printed nothing. */
	details: string;
}

/** Real git messages (git 2.x, English: the host runs git with `LC_ALL=C`), checked in order. */
const RULES: [Exclude<GitErrorKind, "other">, RegExp, () => string][] = [
	[
		"not-repo",
		/not a git repository/i, // "fatal: not a git repository …" and "warning: Not a git repository. Use --no-index …"
		() => m.git_errors_not_repo(),
	],
	[
		"no-commits",
		/does not have any commits yet|ambiguous argument 'HEAD'|bad revision 'HEAD'|src refspec \S+ does not match any|Needed a single revision/i,
		() => m.git_errors_no_commits(),
	],
	[
		"no-remote",
		/no configured push destination|no such remote|no remote repository specified|no git remotes found/i,
		() => m.git_errors_no_remote(),
	],
	[
		"no-upstream",
		/has no upstream branch|no tracking information for the current branch/i,
		() => m.git_errors_no_upstream(),
	],
	[
		"remote-access",
		/could not read from remote repository|authentication failed|could not read (?:username|password)|permission denied \(publickey\)|terminal prompts disabled|repository not found/i,
		() => m.git_errors_remote_access(),
	],
	[
		"rejected",
		/\[rejected\]|updates were rejected|failed to push some refs|non-fast-forward|fetch first/i,
		() => m.git_errors_rejected(),
	],
	[
		"conflict",
		/^CONFLICT \(|automatic merge failed|could not apply [0-9a-f]+|merge conflict in|you have unmerged paths|unresolved conflict/im,
		() => m.git_errors_conflict(),
	],
	[
		"local-changes",
		/local changes to the following files would be overwritten|your local changes would be overwritten|untracked working tree files would be overwritten|cannot pull with rebase: you have unstaged changes|please commit your changes or stash them/i,
		() => m.git_errors_local_changes(),
	],
	[
		"nothing-to-commit",
		/nothing (?:added )?to commit|no changes added to commit/i,
		() => m.git_errors_nothing_to_commit(),
	],
	["locked", /unable to create '[^']*\.lock': file exists/i, () => m.git_errors_locked()],
];

/** The host reports `git <args>: <stderr>`; the arguments are noise for a reader (a `diff` carries a dozen flags). */
function stderrOf(raw: string): string {
	const text = raw.trim();
	if (!text.startsWith("git ")) return text;
	const cut = text.search(/: (?=(?:fatal|error|warning|usage|hint|remote):)/i);
	return (cut < 0 ? text : text.slice(cut + 2)).trim();
}

export function classifyGitError(raw: string): GitProblem {
	const body = stderrOf(raw);
	for (const [kind, re, message] of RULES) if (re.test(body)) return { kind, message: message(), details: body };
	return { kind: "other", message: m.git_errors_other(), details: body };
}
