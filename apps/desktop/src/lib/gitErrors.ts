/**
 * What a failed git command means, in a sentence. Git's own answer to a wrong state is a wall of text (a bare
 * `git diff` outside a repository prints the whole `--no-index` usage, 100+ lines); the app showed it as a toast
 * (#505). Every failure that reaches the user goes through `classifyGitError`: a known error becomes one sentence, an
 * unknown one is cut to its first line, and git's full output stays available as `details`. Pure: no Svelte, no IPC.
 */
export type GitErrorKind =
	"not-repo" | "no-commits" | "no-remote" | "no-upstream" | "remote-access" | "other";

export interface GitProblem {
	kind: GitErrorKind;
	/** One sentence, safe for a toast or a panel. */
	message: string;
	/** Git's own output without the `git <args>:` prefix the host adds; empty when `message` already says it all. */
	details: string;
}

/** Real git messages (git 2.x, English), checked in order. */
const RULES: [GitErrorKind, RegExp, string][] = [
	[
		"not-repo",
		/not a git repository/i, // "fatal: not a git repository …" and "warning: Not a git repository. Use --no-index …"
		"This folder is not a git repository.",
	],
	[
		"no-commits",
		/does not have any commits yet|ambiguous argument 'HEAD'|bad revision 'HEAD'|src refspec \S+ does not match any|Needed a single revision/i,
		"This repository has no commits yet. Make a first commit, then try again.",
	],
	[
		"no-remote",
		/no configured push destination|no such remote|no remote repository specified|no git remotes found/i,
		"This repository has no remote. Add one that points to GitHub, then try again.",
	],
	[
		"no-upstream",
		/has no upstream branch|no tracking information for the current branch/i,
		"This branch has not been pushed yet, so it has nothing to compare with on the remote.",
	],
	[
		"remote-access",
		/could not read from remote repository|authentication failed|could not read (?:username|password)|permission denied \(publickey\)|terminal prompts disabled|repository not found/i,
		"Git could not reach the remote or sign in to it. Check the address and your credentials.",
	],
];

/** The host reports `git <args>: <stderr>`; the arguments are noise for a reader (a `diff` carries a dozen flags). */
function stderrOf(raw: string): string {
	const text = raw.trim();
	if (!text.startsWith("git ")) return text;
	const cut = text.search(/: (?=(?:fatal|error|warning|usage|hint|remote):)/i);
	return (cut < 0 ? text : text.slice(cut + 2)).trim();
}

const FIRST_LINE_MAX = 200;

export function classifyGitError(raw: string): GitProblem {
	const body = stderrOf(raw);
	for (const [kind, re, message] of RULES) if (re.test(body)) return { kind, message, details: body };
	// Unknown: the first line is the cause, a usage block is never one.
	const lines = body.split("\n");
	const usage = lines.findIndex((l) => /^usage:/i.test(l));
	const head = (usage < 0 ? lines : lines.slice(0, usage)).find((l) => l.trim()) ?? "";
	const first = head
		.trim()
		.replace(/^(?:fatal|error|warning):\s*/i, "")
		.slice(0, FIRST_LINE_MAX);
	const message = first || "git failed";
	return { kind: "other", message, details: body === message ? "" : body };
}
