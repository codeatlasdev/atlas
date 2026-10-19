import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { host } from "#lib/host.ts";
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
	"number,title,state,url,body,author,reviews,comments,statusCheckRollup,mergeable,headRefName,headRefOid,baseRefName,isDraft";

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
	auth = $state<"ok" | "missing" | "unknown">("unknown");

	private dir(worktreeId: string) {
		return ws.worktree(worktreeId)?.path ?? "/";
	}

	async gh(worktreeId: string, args: string[], timeout_ms = 60_000) {
		return host.exec(this.dir(worktreeId), ["gh", ...args], { timeout_ms });
	}

	async checkAuth(worktreeId: string) {
		const r = await this.gh(worktreeId, ["auth", "status"], 15_000).catch(() => null);
		this.auth = r?.code === 0 ? "ok" : "missing";
		return this.auth === "ok";
	}

	async view(worktreeId: string): Promise<PrView | null> {
		const n = ws.worktree(worktreeId)?.linkedPr;
		const r = await this.gh(worktreeId, ["pr", "view", ...(n ? [String(n)] : []), "--json", PR_FIELDS]).catch(
			() => null,
		);
		if (!r || r.code !== 0) return null;
		const v = JSON.parse(r.stdout) as PrView;
		this.views[worktreeId] = v;
		const w = ws.worktree(worktreeId);
		if (w && w.linkedPr !== v.number) w.linkedPr = v.number;
		return v;
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
			toast.error(push.stderr || "git push failed");
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
			await this.view(worktreeId);
		} catch (e) {
			toast.error(errText(e));
		}
	}
}

export const pr = new Prs();
