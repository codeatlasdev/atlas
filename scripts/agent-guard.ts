// Agent guards entry point (docs/contributing.md "Agent guards"). One decision function, three callers:
//   bun scripts/agent-guard.ts hook [--json]            Claude Code / Codex PreToolUse hook: tool call JSON on stdin;
//                                                       denial = exit 2 + the reason on stderr (the agent reads it)
//   bun scripts/agent-guard.ts pre-push <remote> <url>  git pre-push hook (scripts/git-hooks/pre-push): refs on stdin;
//                                                       denial = exit 1 + the reason on stderr
// omp loads the same decision function in-process (.omp/extensions/agent-guard.ts).
// `--json` prints the verdict as `{"allow":true}` or `{"allow":false,"action":…,"message":…}` on stdout.
import { isAncestor, realCtx } from "./lib/agent-guard/context";
import { decide, type Verdict } from "./lib/agent-guard/decide";
import { fromHook } from "./lib/agent-guard/payload";
import { decidePush, parsePushLines } from "./lib/agent-guard/prepush";

const [mode, ...rest] = process.argv.slice(2);
const json = rest.includes("--json");

function finish(v: Verdict, denyCode: number): never {
	if (json) console.log(JSON.stringify(v));
	if (!v.allow) {
		console.error(v.message);
		process.exit(denyCode);
	}
	process.exit(0);
}

async function main(): Promise<never> {
	if (mode === "hook") {
		let payload: { cwd?: unknown } | undefined;
		try {
			payload = JSON.parse(await Bun.stdin.text());
		} catch {
			// Not a hook payload: no opinion.
		}
		const call = fromHook(payload);
		if (!call) return finish({ allow: true }, 2);
		const cwd = typeof payload?.cwd === "string" && payload.cwd ? payload.cwd : process.cwd();
		return finish(decide(call, realCtx(cwd)), 2);
	}
	if (mode === "pre-push") {
		const ctx = realCtx(process.cwd());
		return finish(decidePush(parsePushLines(await Bun.stdin.text()), ctx, (a, b) => isAncestor(process.cwd(), a, b)), 1);
	}
	console.error("usage: bun scripts/agent-guard.ts hook [--json] | pre-push <remote> <url>");
	process.exit(64);
}

try {
	await main();
} catch (e) {
	// A bug in the guard must not lock the agent out of every tool: say so loudly and let the call through.
	console.error(`agent-guard: internal error, allowing this call: ${e instanceof Error ? e.stack : e}`);
	process.exit(mode === "pre-push" ? 1 : 0);
}
