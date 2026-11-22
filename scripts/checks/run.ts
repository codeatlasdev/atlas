// Runs a plan's commands one after another and stops at the first failure, saying which check failed and the exact
// command to rerun. Also: the crate graph of the workspace (cargo metadata), which only a Rust-scoped plan needs.
import { $ } from "bun";
import { bash } from "../sh.ts";
import type { Graph, Planned } from "./plan.ts";

interface Package {
	name: string;
	dependencies: { name: string }[];
	targets: { kind: string[] }[];
}

/** Target kinds that are not a library: a package with only these has no doctests. */
const NOT_LIB = new Set(["bin", "example", "test", "bench", "custom-build"]);

/** The crate graph of the workspace and which packages have a library target (a `lib`, `rlib`, `proc-macro`... kind;
 * a package with both a lib and a bin counts). From `cargo metadata --no-deps` run in `cwd`. */
export async function workspaceMeta(cwd?: string): Promise<{ graph: Graph; libs: Set<string> }> {
	const meta = JSON.parse(await $`cargo metadata --no-deps --format-version 1`.cwd(cwd ?? process.cwd()).quiet().text());
	const packages: Package[] = meta.packages;
	const names = new Set(packages.map((p) => p.name));
	return {
		graph: Object.fromEntries(packages.map((p) => [p.name, [...new Set(p.dependencies.map((d) => d.name).filter((d) => names.has(d)))]])),
		libs: new Set(packages.filter((p) => p.targets.some((t) => t.kind.some((k) => !NOT_LIB.has(k)))).map((p) => p.name)),
	};
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
