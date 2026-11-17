// bun run check [--dry-run] [--json]
// The gate: every check of scripts/checks/map.ts that can run on this machine, in order, stopping at the first failure.
// --json prints what it would run and the checks only CI can run (nothing is executed); --dry-run lists the commands.
// `bun run check:changed` is the same map restricted to what your changes affect.
import { dirname } from "node:path";
import { ciOnly, plan } from "./checks/plan.ts";
import { run } from "./checks/run.ts";

const args = process.argv.slice(2);
const p = plan(undefined);
if (args.includes("--json")) {
	console.log(JSON.stringify({ commands: p.commands.map(({ id, area, cmd }) => ({ id, area, cmd })), ciOnly: ciOnly() }, null, 2));
	process.exit(0);
}
const code = await run(p.commands, args.includes("--dry-run"), dirname(import.meta.dir));
if (code === 0) {
	console.log(`CI also runs, on machines this one is not: ${ciOnly().map((k) => k.id).join(", ")} (bun run check --json says why)`);
}
process.exit(code);
