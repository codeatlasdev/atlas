// 15 s: switch spaces (the strip at the bottom of the sidebar, then ⌥⌘←/→), hide the sidebar and peek it back.
import type { Scene } from "../ctx.ts";
import { createWorktrees } from "../fixtures.ts";

const scene: Scene = {
	id: "spaces",
	seconds: 15,
	async setup(ctx) {
		await createWorktrees(ctx);
		// every stack up (green dots, n/m counts in the sidebar), as in the film
		for (const [project, up] of [["lumen-app", 1], ["nova-docs", 1], ["pulse-api", 1], ["orbit-shop", 5]] as const) await ctx.startStack(project, up, 90_000);
		await ctx.worktree("orbit-shop");
		await ctx.sidebar("show");
		// "Other projects" is collapsed by default: open it so the whole project list is on screen
		await ctx.eval(`const b = document.querySelector('.sidebar-slot button.section[aria-expanded="false"]'); b?.click(); return !!b`);
		await ctx.sleep(1500);
	},
	async body(ctx) {
		await ctx.sleep(1200);
		await ctx.space("nova-docs");
		await ctx.sleep(1400);
		await ctx.space("pulse-api");
		await ctx.sleep(1400);
		await ctx.key("alt-cmd-left");
		await ctx.sleep(1300);
		await ctx.key("alt-cmd-left");
		await ctx.sleep(1300);
		await ctx.space("orbit-shop");
		await ctx.sleep(1500);
		await ctx.sidebar("hide");
		await ctx.sleep(1800);
		await ctx.sidebar("show");
		await ctx.sleep(1500);
		await ctx.key("alt-cmd-right");
		await ctx.sleep(1300);
	},
};
export default scene;
