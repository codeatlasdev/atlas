// 16 s: the Logs tab: find with highlights, regex, level filter, time range.
import type { Scene } from "../ctx.ts";
import { createWorktrees } from "../fixtures.ts";

const scene: Scene = {
	id: "logs",
	seconds: 16,
	async setup(ctx) {
		await createWorktrees(ctx);
		await ctx.worktree("orbit-shop");
		await ctx.sidebar("hide");
		await ctx.startStack("orbit-shop", 5, 90_000);
		await ctx.sleep(35_000); // enough history for api's stripe errors and worker's queue-lag warnings
		await ctx.openTab("logs");
	},
	async body(ctx) {
		const search = 'input[aria-label="Search logs"]';
		await ctx.sleep(1300);
		await ctx.typeInto(search, "stripe", 110);
		await ctx.sleep(1800);
		await ctx.key("enter");
		await ctx.sleep(900);
		await ctx.clearInput(search);
		await ctx.sleep(500);
		await ctx.click('button[aria-label="Regular expression"]');
		await ctx.typeInto(search, "stripe|timeout", 80);
		await ctx.sleep(1700);
		await ctx.clearInput(search);
		await ctx.click('button[aria-label="Regular expression"]');
		await ctx.sleep(500);
		await ctx.clickText("warn+");
		await ctx.sleep(2000);
		await ctx.clickText("error+");
		await ctx.sleep(1800);
		await ctx.clickText("all levels");
		await ctx.sleep(900);
		await ctx.click('button[aria-label^="Time range"]');
		await ctx.sleep(1000);
		await ctx.clickText("Last 1 min");
		await ctx.sleep(1500);
	},
};
export default scene;
