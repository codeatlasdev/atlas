/**
 * The identity a commit needs (`user.name` + `user.email`). A first user has none, and `git commit` answers with
 * "Author identity unknown *** Please tell me who you are": Source Control asks for it instead. Everything goes through
 * one `run` (`git -C <worktree> …` in hostd), so SSH checkouts resolve it on the host that will commit. Pure: no
 * Svelte, no IPC.
 */
export interface GitOut {
	code: number | null;
	stdout: string;
	stderr: string;
}
/** `git <args>` in the worktree. */
export type GitRun = (args: string[]) => Promise<GitOut>;

export interface Identity {
	name: string;
	email: string;
}
/** `local`: this repository's config; `global`: `~/.gitconfig`, so every repository follows. */
export type IdentityScope = "local" | "global";

/** What git prints when it has no identity to commit with (author or committer; the e-mail could not be guessed). */
const UNKNOWN = /(?:author|committer) identity unknown|unable to auto-detect email address|empty ident name/i;

export const isIdentityError = (text: string): boolean => UNKNOWN.test(text);

/**
 * Whether `git commit` would stop for a missing identity. `git var` resolves it exactly like commit does (config,
 * `GIT_AUTHOR_*` and `EMAIL` in hostd's environment, the guess from the account and host name) and fails when git
 * would refuse.
 */
export async function identityMissing(run: GitRun): Promise<boolean> {
	for (const v of ["GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT"]) {
		const r = await run(["var", v]);
		if (r.code !== 0) return true;
	}
	return false;
}

async function configValue(run: GitRun, key: string): Promise<string> {
	const r = await run(["config", "--get", key]);
	return r.code === 0 ? r.stdout.trim() : "";
}

/** Whatever part of the identity git already has (one of the two can be set without the other). */
export async function readIdentity(run: GitRun): Promise<Partial<Identity>> {
	const [name, email] = await Promise.all([configValue(run, "user.name"), configValue(run, "user.email")]);
	return { ...(name && { name }), ...(email && { email }) };
}

/** Why `id` cannot be saved, or null. An e-mail is only checked for the shape git needs: something before and after one `@`. */
export function identityProblem(id: Identity): string | null {
	if (!id.name.trim()) return "Enter your name.";
	if (!/^[^\s@<>]+@[^\s@<>]+$/.test(id.email.trim())) return "Enter an e-mail address like you@example.com.";
	return null;
}

/** Writes the identity into the chosen config; throws git's own message when it cannot (a read-only `~/.gitconfig`). */
export async function saveIdentity(run: GitRun, id: Identity, scope: IdentityScope): Promise<void> {
	const problem = identityProblem(id);
	if (problem) throw new Error(problem);
	const flag = scope === "global" ? ["--global"] : [];
	for (const [key, value] of [
		["user.name", id.name.trim()],
		["user.email", id.email.trim()],
	]) {
		const r = await run(["config", ...flag, key, value]);
		if (r.code !== 0) throw new Error((r.stderr || r.stdout).trim() || `git config ${key} failed`);
	}
}
