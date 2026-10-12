// 17 s: a terminal running mcp-demo.py: the real `atlas mcp` over stdio, the agent side scripted.
import type { Scene } from "../ctx.ts";
import { createWorktrees } from "../fixtures.ts";

const scene: Scene = {
	id: "mcp",
	seconds: 17,
	async setup(ctx) {
		await createWorktrees(ctx);
		await ctx.worktree("orbit-shop");
		await ctx.sidebar("hide");
		await ctx.startStack("orbit-shop", 5, 90_000);
		await ctx.sleep(30_000); // stripe timeouts must already be in the log for the search step
	},
	async body(ctx) {
		await ctx.terminal(["python3", `${ctx.env.assets}/mcp-demo.py`], {
			label: "omp",
			env: { ATLAS_BIN: ctx.env.atlasBin, MCP_DEMO_DIR: ctx.env.projects["orbit-shop"]!, MCP_DEMO_HOLD: "20" },
		});
		await ctx.sleep(16_500);
	},
};
export default scene;
