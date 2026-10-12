// 11 s: the live project panel: health pills, per-service CPU/memory sparklines, resource charts. The stack is started
// before recording and given 25 s to produce a believable chart history.
import type { Scene } from "../ctx.ts";
import { createWorktrees } from "../fixtures.ts";

const scene: Scene = {
	id: "panel",
	seconds: 11,
	async setup(ctx) {
		await createWorktrees(ctx);
		await ctx.worktree("orbit-shop");
		await ctx.sidebar("hide");
		await ctx.openTab("services");
		await ctx.startStack("orbit-shop", 5, 90_000);
		await ctx.sleep(25_000);
	},
	async body(ctx) {
		await ctx.sleep(2500);
		await ctx.smoothScroll(".scroll", 380, 2200);
		await ctx.sleep(2500);
		await ctx.smoothScroll(".scroll", 0, 1800);
		await ctx.sleep(1500);
	},
};
export default scene;
