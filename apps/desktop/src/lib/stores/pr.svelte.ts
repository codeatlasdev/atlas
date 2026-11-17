import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { gitToast } from "#lib/gitToast.ts";
import { host } from "#lib/host.ts";
import {
	classifyGhView,
	ghAuth,
	probeRepo,
	type GhAuth,
	type GhLookup,
	type RepoProbe,
	type Run,
} from "#lib/repoState.ts";
import { promptDialog } from "#lib/stores/ask.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

export interface Check {
	name: string;
	state: string;
	link: string;
	workflow: string;
	description: string;
}

export interface PrView {
	number: number;
	title: string;
	state: string;
	url: string;
	body: string;
	author: { login: string };
	isDraft: boolean;
	mergeable: string;
	/** `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or "" when the repository asks for no review. */
	reviewDecision: string;
	headRefName: string;
	/** Head commit sha; auto clean up only treats commits up to it as pushed. */
	headRefOid: string;
	baseRefName: string;
	reviews: { author: { login: string }; state: string; body: string; submittedAt: string }[];
	comments: { author: { login: string }; body: string; createdAt: string }[];
	statusCheckRollup: {
		name?: string;
		context?: string;
		conclusion?: string;
		state?: string;
		status?: string;
	}[];
}

const PR_FIELDS =
	"number,title,state,url,body,author,reviews,comments,statusCheckRollup,mergeable,reviewDecision,headRefName,headRefOid,baseRefName,isDraft";

/** Failed/pending/ok summary of a check rollup. */
export function rollup(p: Pick<PrView, "statusCheckRollup">): "failed" | "pending" | "ok" | "none" {
	const c = p.statusCheckRollup ?? [];
	if (!c.length) return "none";
	if (c.some((x) => ["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT"].includes(x.conclusion ?? x.state ?? "")))
		return "failed";
	if (c.some((x) => x.status && x.status !== "COMPLETED")) return "pending";
	return "ok";
}

class Prs {
	/** Last fetched PR per worktree (sidebar chip). */
	views = $state<Record<string, PrView>>({});
	createFor = $state<string | null>(null);
	auth = $state<GhAuth>("unknown");

	private dir(worktreeId: string) {
		return ws.worktree(worktreeId)?.path ?? "/";
	}

	async gh(worktreeId: string, args: string[], timeout_ms = 60_000) {
		return host.exec(this.dir(worktreeId), ["gh", ...args], { timeout_ms });
	}

	/** gh is installed and signed in; false for "not installed" and "signed out" alike (see `auth` for which). */
	get ghUnusable() {
		return this.auth === "signed-out" || this.auth === "not-installed";
	}

	/** `argv` in the worktree's folder through its host (git and gh alike). */
	runner(worktreeId: string, timeout_ms = 60_000): Run {
		return (argv) => host.exec(this.dir(worktreeId), argv, { timeout_ms });
	}

	async checkAuth(worktreeId: string) {
		this.auth = await ghAuth(this.runner(worktreeId, 15_000));
		return this.auth === "ok";
	}

	/** Remotes, first commit and default branch of the folder (see `repoState.ts`). */
	probe(worktreeId: string): Promise<RepoProbe> {
		return probeRepo(this.runner(worktreeId, 15_000));
	}

	/** What gh knows about the PR of this worktree's branch (or of the PR it is linked to). */
	async lookup(worktreeId: string): Promise<GhLookup> {
		const n = ws.worktree(worktreeId)?.linkedPr;
		let l: GhLookup;
		try {
			const args = ["gh", "pr", "view", ...(n ? [String(n)] : []), "--json", PR_FIELDS];
			l = classifyGhView(await this.runner(worktreeId)(args));
		} catch (e) {
			l = { kind: "error", message: errText(e) };
		}
		if (l.kind === "pr") {
			this.views[worktreeId] = l.pr;
			const w = ws.worktree(worktreeId);
			if (w && w.linkedPr !== l.pr.number) w.linkedPr = l.pr.number;
		}
		return l;
	}

	/** Pushes the branch and sets its upstream, the step "Create pull request" needs first. True when it worked. */
	async pushBranch(worktreeId: string, remote: string, branch: string): Promise<boolean> {
		const r = await host.exec(this.dir(worktreeId), ["git", "push", "--set-upstream", remote, branch], {
			timeout_ms: 120_000,
		});
		if (r.code !== 0) gitToast(r.stderr || r.stdout);
		else toast.success(`Pushed ${branch} to ${remote}`);
		return r.code === 0;
	}

	/** Asks for the address of the GitHub repository and adds it as a remote. True when one was added. */
	async addRemote(worktreeId: string, name: string): Promise<boolean> {
		const url = await promptDialog("Add a remote", {
			description: "The address of the repository on GitHub, e.g. https://github.com/owner/repo.git",
			placeholder: "https://github.com/owner/repo.git",
			confirmLabel: "Add remote",
		});
		if (!url) return false;
		const r = await host.exec(this.dir(worktreeId), ["git", "remote", "add", name, url]);
		if (r.code !== 0) gitToast(r.stderr || r.stdout);
		return r.code === 0;
	}

	/** `gh auth login` in a terminal of the worktree: it asks questions and opens the browser. */
	signIn(worktreeId: string) {
		void ws.newTerminal(worktreeId, { argv: ["gh", "auth", "login"], label: "gh auth" });
	}

	async checks(worktreeId: string, n: number): Promise<Check[]> {
		const r = await this.gh(worktreeId, [
			"pr",
			"checks",
			String(n),
			"--json",
			"name,state,link,workflow,description",
		]).catch(() => null);
		return r?.stdout.trim() ? (JSON.parse(r.stdout) as Check[]) : [];
	}

	/** `--log-failed` of the run behind a check link (…/runs/<id>/…). */
	async failedLog(worktreeId: string, c: Check): Promise<string> {
		const id = c.link.match(/\/runs\/(\d+)/)?.[1];
		// Commit statuses (not Actions) have no log: give their description and link.
		if (!id) return `${c.name}: ${c.description || c.state}${c.link ? `\n${c.link}` : ""}`;
		const r = await this.gh(worktreeId, ["run", "view", id, "--log-failed"], 120_000);
		return (r.stdout || r.stderr).slice(-30_000);
	}

	openCreate(worktreeId: string) {
		this.createFor = worktreeId;
	}

	async create(worktreeId: string, a: { title: string; body: string; base: string; draft: boolean }) {
		const push = await host.exec(this.dir(worktreeId), ["git", "push"], { timeout_ms: 120_000 });
		if (push.code !== 0) {
			gitToast(push.stderr || "git push failed");
			return;
		}
		const args = ["pr", "create", "--title", a.title, "--body", a.body];
		if (a.base) args.push("--base", a.base);
		if (a.draft) args.push("--draft");
		const r = await this.gh(worktreeId, args, 120_000);
		if (r.code !== 0) {
			toast.error(r.stderr || "gh pr create failed");
			return;
		}
		const n = Number(r.stdout.trim().match(/\/pull\/(\d+)/)?.[1]);
		const w = ws.worktree(worktreeId);
		if (w && n) w.linkedPr = n;
		toast.success(`PR #${n} created`);
		this.openTab(worktreeId);
	}

	openTab(worktreeId: string) {
		ws.reveal(worktreeId, (t) => t.kind === "pr", { kind: "pr", label: "Pull request" });
	}

	async run(worktreeId: string, args: string[], ok: string) {
		try {
			const r = await this.gh(worktreeId, args, 120_000);
			if (r.code !== 0) throw r.stderr || "gh failed";
			toast.success(ok);
			await this.lookup(worktreeId);
		} catch (e) {
			toast.error(errText(e));
		}
	}
}

export const pr = new Prs();
