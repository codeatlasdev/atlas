// omp loads this from <cwd>/.omp/extensions (docs: omp extension-loading). Same decision function as the Claude Code and
// Codex hooks (scripts/agent-guard.ts): see docs/contributing.md "Agent guards".
import { realCtx } from "../../scripts/lib/agent-guard/context";
import { decide } from "../../scripts/lib/agent-guard/decide";
import { fromOmp } from "../../scripts/lib/agent-guard/payload";

// The slice of omp's ExtensionAPI used here.
interface Api {
	on(event: "tool_call", handler: (event: unknown, ctx: { cwd?: string }) => Promise<{ block: true; reason: string } | undefined>): void;
}

export default function agentGuard(pi: Api): void {
	pi.on("tool_call", async (event, ctx) => {
		try {
			const call = fromOmp(event);
			if (!call) return undefined;
			const verdict = decide(call, realCtx(ctx.cwd ?? process.cwd()));
			return verdict.allow ? undefined : { block: true, reason: verdict.message };
		} catch (e) {
			// A bug in the guard must not lock the agent out of every tool.
			console.error(`agent-guard: internal error, allowing this call: ${e}`);
			return undefined;
		}
	});
}
