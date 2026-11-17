import { git, errText } from "#lib/api.ts";
import { gitToast } from "#lib/gitToast.ts";
import { gitStatus } from "#lib/stores/gitStatus.ts";

/** `git init` in the worktree's folder; every panel that shows "not a git repository" offers it. True when it worked. */
export async function initRepository(worktreeId: string): Promise<boolean> {
	try {
		const r = await git.run(worktreeId, ["init"]);
		if (r.code !== 0) {
			gitToast(r.stderr || r.stdout);
			return false;
		}
		return true;
	} catch (e) {
		gitToast(errText(e));
		return false;
	} finally {
		await gitStatus.refresh(worktreeId);
	}
}
