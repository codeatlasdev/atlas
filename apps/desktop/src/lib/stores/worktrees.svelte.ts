import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { api, errText, type ProjectView, type WorktreeProgress } from "#lib/api.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

export interface Creation {
	id: string;
	repoId: string;
	name: string;
	step: WorktreeProgress["step"];
	message: string;
}

export interface CreateRequest {
	repoId: string;
	name?: string;
	base?: string;
	branch?: string;
	runSetup?: boolean;
	pr?: number;
	/** Agent to launch in the new worktree (`blank` = plain terminal). */
	agent?: string;
	prompt?: string;
}

/** Called after a worktree is created with an agent requested (wired by the agents module). */
export const hooks: {
	launchAgent?: (worktreeId: string, agent: string, prompt?: string) => Promise<unknown>;
} = {};

class Worktrees {
	creations = $state<Record<string, Creation>>({});
	newDialog = $state<{ open: boolean; repoId: string | null }>({ open: false, repoId: null });

	/** Mirror registry entries into the workspace model. */
	sync(projects: ProjectView[]) {
		const ids = new Set(projects.map((p) => p.id));
		for (const p of projects) {
			const prev = ws.worktree(p.id);
			ws.upsertWorktree({
				id: p.id,
				repoId: p.parent ?? p.id,
				path: p.root,
				branch: p.branch ?? prev?.branch ?? "",
				baseRef: prev?.baseRef ?? "",
				name: p.parent ? (prev?.name ?? p.root.split("/").pop() ?? p.id) : p.name,
				isMain: !p.parent,
				archived: prev?.archived ?? false,
				pinned: p.pinned,
				unread: prev?.unread ?? false,
				portOffset: p.port_offset,
				createdAt: prev?.createdAt ?? Date.now(),
				lastActivityAt: prev?.lastActivityAt ?? Date.now(),
				linkedPr: prev?.linkedPr,
				agent: prev?.agent,
				hostId: p.host ?? undefined,
			});
		}
		for (const w of [...ws.doc.worktrees]) if (!ids.has(w.id)) ws.removeWorktree(w.id);
	}

	/** Create and select a worktree; resolves with its id once git is done. */
	async create(req: CreateRequest): Promise<string | undefined> {
		const { promise, resolve } = Promise.withResolvers<string | undefined>();
		let creationId = "";
		const early: WorktreeProgress[] = [];
		const handle = async (p: WorktreeProgress) => {
			const c = this.creations[creationId];
			if (c) {
				c.step = p.step;
				c.message = p.message ?? "";
			}
			if (p.step === "error") {
				off();
				delete this.creations[creationId];
				toast.error(p.message ?? "worktree creation failed");
				resolve(undefined);
			} else if (p.step === "done" && p.worktree) {
				off();
				delete this.creations[creationId];
				resolve(await this.finish(p, req));
			}
		};
		const off = await listen<WorktreeProgress>("worktree://progress", (e) => {
			if (!creationId) early.push(e.payload);
			else if (e.payload.creation_id === creationId) void handle(e.payload);
		});
		try {
			creationId = (await api.worktreeCreate(req)).creation_id;
		} catch (e) {
			off();
			toast.error(errText(e));
			return undefined;
		}
		this.creations[creationId] = {
			id: creationId,
			repoId: req.repoId,
			name: req.name || "new worktree",
			step: "fetch",
			message: "",
		};
		early.filter((p) => p.creation_id === creationId).forEach((p) => void handle(p));
		return promise;
	}

	private async finish(p: WorktreeProgress, req: CreateRequest): Promise<string> {
		const w = p.worktree!;
		await store.refresh();
		this.sync(store.projects);
		const wt = ws.worktree(w.id);
		if (wt) {
			wt.name = w.name;
			wt.baseRef = w.base;
			wt.linkedPr = w.linkedPr ?? undefined;
			wt.agent = req.agent && req.agent !== "blank" ? req.agent : undefined;
		}
		store.selected = w.id;
		ws.select(w.id);
		if (p.setup) await ws.newTerminal(w.id, { argv: p.setup.argv, env: p.setup.env, label: "Setup" });
		if (req.agent && req.agent !== "blank" && hooks.launchAgent)
			await hooks.launchAgent(w.id, req.agent, req.prompt);
		else if (!p.setup) await ws.newTerminal(w.id);
		return w.id;
	}

	async remove(id: string, force = false) {
		try {
			const r = await api.worktreeRemove(id, force);
			ws.removeWorktree(id);
			if (store.selected === id) store.selected = null;
			await store.refresh();
			if (r.preserved_branch) toast.info(`Branch preserved: ${r.preserved_branch} (not merged)`);
			else toast.success("Worktree deleted");
		} catch (e) {
			toast.error(errText(e));
		}
	}
}

export const worktrees = new Worktrees();
