// 12 s: the GPU terminal demo (ligatures, emoji, truecolor, text styles, a kitty-protocol image).
import type { Scene } from "../ctx.ts";

const scene: Scene = {
	id: "terminal",
	seconds: 12,
	async setup(ctx) {
		await ctx.worktree("orbit-shop");
		await ctx.sidebar("hide");
		await ctx.openTab("services");
	},
	async body(ctx) {
		// tab opens full-window and the script types its own prompt, like the film
		await ctx.terminal(["bash", `${ctx.env.assets}/term-demo.sh`], {
			label: "Terminal",
			env: { TERM_DEMO_IMAGE: `${ctx.env.assets}/orbit-small.png`, TERM_DEMO_HOLD: "20" },
		});
		await ctx.sleep(11_500);
	},
};
export default scene;
