// Runs a plan's commands one after another and stops at the first failure, saying which check failed and the exact
// command to rerun. Also: the crate graph of the workspace (cargo metadata), which only a Rust-scoped plan needs.
import { $ } from "bun";
import { bash } from "../sh.ts";
import type { Graph, Planned } from "./plan.ts";

export async function workspaceGraph(): Promise<Graph> {
	const meta = JSON.parse(await $`cargo metadata --no-deps --format-version 1`.quiet().text());
	const names = new Set<string>(meta.packages.map((p: { name: string }) => p.name));
	return Object.fromEntries(
		meta.packages.map((p: { name: string; dependencies: { name: string }[] }) => [
			p.name,
			[...new Set(p.dependencies.map((d) => d.name).filter((d) => names.has(d)))],
		]),
	);
}

/** Runs `cmds` (printing each; with `dry` only printing) and returns the exit code: 0, or the failing command's. */
export async function run(cmds: Planned[], dry: boolean, cwd: string): Promise<number> {
	const started = Date.now();
	for (const c of cmds) {
		console.log(`▶ ${c.id}: ${c.cmd}`);
		if (dry) continue;
		const t = Date.now();
		const p = Bun.spawn([bash(), "-c", c.cmd], { cwd, stdio: ["inherit", "inherit", "inherit"] });
		const code = await p.exited;
		if (code !== 0) {
			console.error(`✗ ${c.id} failed (exit ${code}) after ${Math.round((Date.now() - t) / 1000)}s. Rerun only it: ${c.cmd}`);
			return code;
		}
	}
	if (!dry && cmds.length > 0) {
		console.log(`✓ ${cmds.length} check(s) passed in ${Math.round((Date.now() - started) / 1000)}s: ${cmds.map((c) => c.id).join(", ")}`);
	}
	return 0;
}
