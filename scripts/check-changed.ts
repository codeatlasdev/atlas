// bun run check:changed [--base <ref>] [--dry-run]
// The edit-loop gate: runs only the checks the changed files can affect (a crate, every crate that depends on it,
// the desktop app, mobile, scripts, skills, workflows). `bun run check` stays the gate before a push.
// Changed = committed since the merge base with --base (default origin/main), plus staged, unstaged and untracked.
import { $ } from "bun";

export type Graph = Record<string, string[]>; // crate -> workspace crates it depends on

/** Every crate that depends on one of `crates`, transitively, plus the crates themselves. */
export function dependents(graph: Graph, crates: Set<string>): Set<string> {
	const out = new Set(crates);
	let grew = true;
	while (grew) {
		grew = false;
		for (const [c, deps] of Object.entries(graph)) {
			if (!out.has(c) && deps.some((d) => out.has(d))) {
				out.add(c);
				grew = true;
			}
		}
	}
	return out;
}

export interface Plan {
	crates: string[]; // cargo clippy/nextest -p for each
	workspace: boolean; // Cargo.toml/Cargo.lock/toolchain changed: everything Rust
	steps: string[]; // non-Rust check commands
}

const STEPS: [RegExp, string][] = [
	[/^apps\/desktop\//, "bun run desktop:check"],
	[/^apps\/mobile\//, "scripts/mobile-check.sh"],
	[/^services\/push-relay\//, "scripts/push-relay-check.sh"],
	[/^skills\//, "scripts/check-skills.sh"],
	[/^\.github\/workflows\//, "scripts/lint-workflows.sh"],
	[/^scripts\//, "bun test scripts"],
];

export function plan(files: string[], graph: Graph): Plan {
	const touched = new Set<string>();
	let workspace = false;
	const steps = new Set<string>();
	for (const f of files) {
		if (/^(Cargo\.(toml|lock)|rust-toolchain\.toml|deny\.toml|\.cargo\/)/.test(f)) workspace = true;
		const m = /^crates\/([^/]+)\//.exec(f);
		if (m && graph[m[1]]) touched.add(m[1]);
		// The desktop shell embeds the frontend build; the CLI embeds the skills.
		if (/^apps\/desktop\//.test(f) && graph["atlas-desktop"]) touched.add("atlas-desktop");
		if (/^skills\//.test(f) && graph["atlas-skills"]) touched.add("atlas-skills");
		for (const [re, cmd] of STEPS) if (re.test(f)) steps.add(cmd);
	}
	const crates = workspace ? [] : [...dependents(graph, touched)].sort();
	return { crates, workspace, steps: [...steps].sort() };
}

async function main() {
	const args = process.argv.slice(2);
	const base = args.includes("--base") ? args[args.indexOf("--base") + 1] : "origin/main";
	const dry = args.includes("--dry-run");
	const mb = (await $`git merge-base ${base} HEAD`.nothrow().quiet().text()).trim();
	const lines = [
		mb ? await $`git diff --name-only ${mb}`.quiet().text() : "",
		await $`git ls-files --others --exclude-standard`.quiet().text(),
	];
	const files = [...new Set(lines.join("\n").split("\n").filter(Boolean))];
	const meta = JSON.parse(await $`cargo metadata --no-deps --format-version 1`.quiet().text());
	const names = new Set<string>(meta.packages.map((p: { name: string }) => p.name));
	const graph: Graph = Object.fromEntries(
		meta.packages.map((p: { name: string; dependencies: { name: string }[] }) => [
			p.name,
			[...new Set(p.dependencies.map((d) => d.name).filter((d) => names.has(d)))],
		]),
	);
	const p = plan(files, graph);
	const cmds: string[] = [];
	const rust = p.workspace || p.crates.length > 0;
	if (rust) cmds.push("cargo fmt --all -- --check");
	const pk = p.workspace ? "--workspace" : p.crates.map((c) => `-p ${c}`).join(" ");
	if (rust) cmds.push(`cargo clippy ${pk} --all-targets --locked -- -D warnings`, `cargo nextest run ${pk} --locked`);
	cmds.push(...p.steps);
	console.log(`${files.length} changed file(s) since ${base}`);
	if (cmds.length === 0) return console.log("nothing to check");
	for (const c of cmds) {
		console.log(`▶ ${c}`);
		if (dry) continue;
		const r = await $`sh -c ${c}`.nothrow();
		if (r.exitCode !== 0) {
			console.error(`✗ ${c} (exit ${r.exitCode})`);
			process.exit(r.exitCode);
		}
	}
	if (!dry) console.log("✓ changed checks passed (bun run check is still the gate before a push)");
}

if (import.meta.main) await main();
