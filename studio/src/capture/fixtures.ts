// Scene building blocks shared by several scenes: the film's three orbit-shop worktrees with their fake agents, and
// the log history/stack warm-up. Everything is deterministic (fixed names, scripted agents, seeded memhog).

import type { Ctx } from "./ctx.ts";
import { ORBIT_WORKTREES } from "./app.ts";

/** Creates orbit-shop's worktrees through the app (same path as `atlas wt create`), without agents or setup terminals. */
export async function createWorktrees(ctx: Ctx): Promise<string[]> {
	const ids: string[] = [];
	for (const w of ORBIT_WORKTREES) {
		const id = await ctx.eval<string | null>(
			`const a = window.__atlas; const repo = a.store.projects.find(p => !p.parent && p.name === "orbit-shop");
			 return (await a.worktrees.create({ repoId: repo.id, name: ${JSON.stringify(w.name)}, runSetup: false, agent: "blank" })) ?? null`
		);
		if (!id) throw new Error(`worktree ${w.name} was not created (see the app log)`);
		// "blank" opens a plain terminal tab; the scenes want only what they open themselves. Let its native GPU view
		// mount first, select it so it is the visible tab, then close it: closing a never-mounted tab leaves an orphan
		// native view painted over the whole pane.
		await ctx.sleep(1500);
		await ctx.eval(`const a = window.__atlas; for (const t of a.ws.tabsOf(${JSON.stringify(id)}).filter(t => t.kind === "terminal")) a.ws.closeTab(t.id, true, true); return 1`);
		await ctx.sleep(700);
		ids.push(id);
	}
	return ids;
}

/**
 * One fake `omp` agent per worktree, staggered: the first reaches its approval prompt quickly ("blocked") and goes on to
 * finish, the second stays blocked for a minute, the third asks a few seconds later. `askSeconds` per agent.
 */
export async function launchAgents(ctx: Ctx, ids: string[], askSeconds: number[]): Promise<void> {
	for (const [i, w] of ORBIT_WORKTREES.entries()) {
		const id = ids[i]!;
		await ctx.eval(
			`const a = window.__atlas, ws = a.ws, ag = a.agents; const worktreeId = ${JSON.stringify(id)};
			 const tabId = crypto.randomUUID();
			 const t = await ws.newTerminal(worktreeId, { tabId, argv: [${JSON.stringify(`${ctx.env.fakeAgentDir}/omp`)}, ${JSON.stringify(w.prompt)}],
			   env: { ATLAS_PANE_KEY: tabId, ATLAS_AGENT: "omp", ATLAS_LAUNCH_TOKEN: "demo", DEMO_DELAY: "${i * 3}", DEMO_ASK: "${askSeconds[i] ?? 6}", HOME: ${JSON.stringify(ctx.env.home)} },
			   agent: "omp", label: "omp" });
			 ag.status[tabId] = { state: "idle", agent: "omp", session_id: null, session_file: null, model: null, prompt: ${JSON.stringify(w.prompt)}, last_message: null,
			   updated_at: Date.now(), state_started_at: Date.now(), worktree_id: worktreeId, tab_id: tabId };
			 try { await window.__TAURI_INTERNALS__.invoke("agent_bind", { pane: tabId, agent: "omp", cwd: ws.worktree(worktreeId).path }); } catch (e) {}
			 return t.id`
		);
	}
}
