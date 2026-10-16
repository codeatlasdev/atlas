import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { git, errText } from "#lib/api.ts";
import {
	applyEvent,
	idle,
	parseEvent,
	startState,
	type CommitRun,
	type ProviderState,
} from "#lib/commitMessage.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

/** Per-worktree run of "generate the commit message"; the commit box binds `message` to `run.text` while one is live. */
class CommitMessages {
	runs = $state<Record<string, CommitRun>>({});
	provider = $state<ProviderState>({ state: "ready" });
	/** Called with the streamed text so the owner of the box can write it into its textarea. */
	private sinks = new Map<string, (text: string) => void>();

	run(worktreeId: string): CommitRun {
		return this.runs[worktreeId] ?? idle();
	}

	/** The commit box of `worktreeId` receives every update of the message while it is mounted. */
	attach(worktreeId: string, sink: (text: string) => void) {
		this.sinks.set(worktreeId, sink);
		return () => this.sinks.delete(worktreeId);
	}

	async loadProvider() {
		this.provider = await git.commitMsgProvider().catch(() => this.provider);
	}

	/** Start (or restart) a run. With no provider, opens Settings → Git instead of failing. */
	async generate(worktreeId: string, instruction?: string) {
		if (this.run(worktreeId).phase === "running") return;
		await this.loadProvider();
		if (this.provider.state === "none") {
			toast.info("Pick an agent for commit messages in Settings → Git");
			updates.show("git");
			return;
		}
		let request: string;
		try {
			request = await git.commitMsgGenerate(worktreeId, instruction);
		} catch (e) {
			const m = errText(e);
			if (m === "no_provider") {
				this.provider = { state: "none" };
				toast.info("Pick an agent for commit messages in Settings → Git");
				updates.show("git");
			} else toast.error(m);
			return;
		}
		this.runs[worktreeId] = startState(request);
		const off = await listen<unknown>(`commit-msg://${request}`, (e) => {
			let ev;
			try {
				ev = parseEvent(e.payload);
			} catch (err) {
				toast.error(errText(err));
				return;
			}
			const next = applyEvent(this.run(worktreeId), ev, request);
			this.runs[worktreeId] = next;
			if (ev.kind === "chunk" || ev.kind === "done") this.sinks.get(worktreeId)?.(next.text);
			if (ev.kind === "error") toast.error(ev.message);
			if (next.phase === "idle") off();
		});
	}

	async cancel(worktreeId: string) {
		const r = this.run(worktreeId);
		if (r.phase === "running") await git.commitMsgCancel(r.request);
	}
}

export const commitMessages = new CommitMessages();

/** The `git::GenerateCommitMessage` command: the selected worktree, from wherever it is invoked. */
export function generateForSelected() {
	const w = ws.selected;
	if (w) void commitMessages.generate(w.id);
}
