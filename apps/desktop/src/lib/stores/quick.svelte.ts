import { api } from "#lib/api.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { uid, ws } from "#lib/stores/workspace.svelte.ts";

export interface QuickCommand {
	id: string;
	label: string;
	command: string;
	/** `terminal`: run in a new terminal; `prompt`: send to the focused agent. */
	kind: "terminal" | "prompt";
	/** `global` or a repo id. */
	scope: string;
}

class Quick {
	list = $state<QuickCommand[]>([]);

	async init() {
		const s = await api.settings();
		if (Array.isArray(s.quickCommands)) this.list = s.quickCommands as QuickCommand[];
	}

	async save() {
		await api.settingsPatch({ quickCommands: this.list });
	}

	add() {
		this.list.push({ id: uid(), label: "New command", command: "", kind: "terminal", scope: "global" });
	}

	for(repoId: string | undefined) {
		return this.list.filter((q) => q.command && (q.scope === "global" || q.scope === repoId));
	}

	async run(q: QuickCommand, worktreeId: string, groupId?: string) {
		if (q.kind === "terminal") {
			await ws.newTerminal(worktreeId, { argv: ["sh", "-lc", q.command], label: q.label, groupId });
			return;
		}
		const focused = ws.tab(ws.focusedGroup(worktreeId).activeTab);
		const target = focused?.agent ? focused : agents.tabsIn(worktreeId)[0];
		if (target) await agents.send(target.id, q.command);
		else await agents.launch(worktreeId, agents.defaultAgent, { prompt: q.command, groupId });
	}
}

export const quick = new Quick();
