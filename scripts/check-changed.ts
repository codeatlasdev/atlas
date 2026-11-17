// bun run check:changed [--base <ref>] [--dry-run] [--json] [--stdin] [--no-windows]
// The edit-loop gate: runs only the checks the changed files can affect, from the same map as `bun run check` and CI
// (scripts/checks/map.ts). It is not the gate: `bun run check` is.
// Changed = committed since the merge base (scripts/lib/base.ts: --base, default origin/main), plus staged, unstaged and
// untracked. No usable base, or a path no area claims, means everything runs (the reason is printed).
//
//   --dry-run   print the commands and why each runs; run nothing
//   --json      print the plan as JSON (what would run and why); run nothing. Also the `affected:` plugin of atlas.yaml
//               (MCP tool atlas_affected): `tasks`, `tests`, `routes`, `builds` are its contract.
//   --stdin     the changed files come from stdin, one per line, instead of git (what the plugin passes)
import { dirname } from "node:path";
import { type DiffBase, resolveBase } from "./lib/base.ts";
import { ciOnly, classify, plan } from "./checks/plan.ts";
import { run, workspaceGraph } from "./checks/run.ts";

const root = dirname(import.meta.dir);
const args = process.argv.slice(2);
const flag = (name: string) => args.includes(name);

let files: string[] | undefined;
let base: DiffBase | undefined;
if (flag("--stdin")) {
	files = (await Bun.stdin.text()).split(/\r?\n/).filter(Boolean);
} else {
	try {
		base = resolveBase(root, process.env, args.includes("--base") ? args[args.indexOf("--base") + 1] : undefined);
	} catch (e) {
		console.error(`✗ ${(e as Error).message}`);
		process.exit(2);
	}
	files = base?.touched;
}

// Only a change confined to some crates needs the crate graph.
const c = classify(files);
const graph = c.areas.rust!.on && c.everything === null ? await workspaceGraph() : {};
// cargo-xwin present and not on Windows (which runs the real thing): also lint the Windows target.
const windows = process.platform !== "win32" && Bun.which("cargo-xwin") !== null && !flag("--no-windows");
const p = plan(files, graph, { windows });

if (flag("--json")) {
	const rust = p.commands.filter((x) => x.area === "rust");
	console.log(
		JSON.stringify(
			{
				base: base ? { ref: base.ref, mode: base.mode, mergeBase: base.mb } : null,
				files: files ?? null,
				everything: p.everything,
				areas: p.areas,
				ignored: p.ignored,
				workspace: p.workspace,
				crates: p.crates,
				commands: p.commands,
				ciOnly: ciOnly(p),
				// The atlas_affected contract.
				tasks: [],
				tests: p.commands.map((x) => x.cmd),
				routes: [],
				builds: p.crates,
				steps: p.commands.filter((x) => !rust.includes(x)).map((x) => x.cmd),
			},
			null,
			2,
		),
	);
	process.exit(0);
}

const dry = flag("--dry-run");
if (process.platform !== "win32" && !windows && !flag("--no-windows")) {
	console.log("(no Windows lint: install cargo-xwin, plus clang and lld, to catch Windows-only errors here)");
}
console.log(files ? `${files.length} changed file(s) since ${base ? `${base.ref} (${base.mode})` : "stdin"}` : "changed files unknown");
if (p.everything) console.log(`everything runs: ${p.everything}${files ? "" : " (pass --base <ref>, or git fetch origin main)"}`);
if (p.commands.length === 0) {
	console.log("nothing to check");
	process.exit(0);
}
for (const x of p.commands) if (dry) console.log(`  ${x.id}: ${x.why.join("; ")}`);
const code = await run(p.commands, dry, root);
if (code === 0 && !dry) console.log("✓ changed checks passed (bun run check is still the gate before a push)");
process.exit(code);
