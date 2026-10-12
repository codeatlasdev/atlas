// 19 s: three fake agents in worktrees working / blocked / done, shown in the sidebar and the Agents view.
import type { Scene } from "../ctx.ts";
import { createWorktrees, launchAgents } from "../fixtures.ts";

const scene: Scene = {
	id: "agents",
	seconds: 19,
	async setup(ctx) {
		const ids = await createWorktrees(ctx);
		await ctx.worktree("orbit-shop");
		await ctx.sidebar("show");
		await ctx.startStack("orbit-shop", 5, 90_000);
		// recording starts right after launch: agents are still reading files when the camera rolls
		await launchAgents(ctx, ids, [2, 60, 7]);
		await ctx.eval(`const a = window.__atlas; const w = a.ws.doc.worktrees.find(w => w.name === "checkout-shipping"); a.store.selected = w.id; a.ws.select(w.id); return 1`);
	},
	async body(ctx) {
		await ctx.sleep(3500);
		await ctx.view("agents");
		await ctx.sleep(4500);
		await ctx.worktree("orbit-shop", "fix-stripe-timeout");
		await ctx.sleep(4500);
		await ctx.view("agents");
		await ctx.sleep(3500);
		await ctx.worktree("orbit-shop", "cart-empty-state");
		await ctx.sleep(3000);
	},
};
export default scene;
