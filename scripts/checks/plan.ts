// From changed paths to the checks that must run, using the one map (./map.ts). Pure: no git, no cargo, no I/O.
import { AREAS, CHECKS, CI_RUNS_ALL, EMBEDS, IGNORE, OVERLAY } from "./map.ts";

export type Graph = Record<string, string[]>; // crate -> workspace crates it depends on

const globs = new Map<string, Bun.Glob>();
/** Whether `file` (forward slashes, relative to the repo root) matches `pattern`. */
export function matches(pattern: string, file: string): boolean {
	let g = globs.get(pattern);
	if (!g) globs.set(pattern, (g = new Bun.Glob(pattern)));
	return g.match(file);
}

const anyMatch = (patterns: string[], file: string) => patterns.find((p) => matches(p, file));

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

export interface Area {
	on: boolean;
	/** Why, as `glob ← file (+N more)`; the paths that turned it on. */
	because: string[];
}

export interface Classified {
	/** Set when every area is on regardless of the paths: an unknown path, or no list of changed files. */
	everything: string | null;
	areas: Record<string, Area>;
	/** Changed files that affect no check (docs, metadata). */
	ignored: string[];
	/** Per area, the changed files that turned it on. */
	hits: Record<string, string[]>;
}

/** `local` is the edit loop and the gate; `ci` also reruns every job when a workflow or action changed. */
export type Mode = "local" | "ci";

function describe(byGlob: Map<string, string[]>): string[] {
	return [...byGlob].map(([g, fs]) => `${g} ← ${fs[0]}${fs.length > 1 ? ` (+${fs.length - 1} more)` : ""}`);
}

/** Which areas the changed `files` turn on. `files` undefined means the diff is unknown: everything runs. */
export function classify(files: string[] | undefined, mode: Mode = "local"): Classified {
	const names = Object.keys(AREAS);
	const by: Record<string, Map<string, string[]>> = Object.fromEntries(names.map((a) => [a, new Map()]));
	const hits: Record<string, string[]> = Object.fromEntries(names.map((a) => [a, []]));
	const ignored: string[] = [];
	let everything: string | null = files === undefined ? "no changed-file list (no diff base): every check runs" : null;
	const add = (area: string, glob: string, file: string) => {
		const m = by[area]!;
		m.set(glob, [...(m.get(glob) ?? []), file]);
		if (!hits[area]!.includes(file)) hits[area]!.push(file);
	};
	for (const file of files ?? []) {
		let claimed = false;
		for (const [area, patterns] of Object.entries(AREAS)) {
			const g = anyMatch(patterns, file);
			if (g) {
				add(area, g, file);
				claimed = true;
			}
		}
		for (const [area, patterns] of Object.entries(OVERLAY)) {
			const g = anyMatch(patterns, file);
			if (g) add(area, g, file);
		}
		const all = mode === "ci" && anyMatch(CI_RUNS_ALL, file);
		if (all) everything ??= `${file} changed (${all}): a job definition changed, so every job runs`;
		if (claimed) continue;
		if (anyMatch(IGNORE, file)) ignored.push(file);
		else everything ??= `${file} matches no area of scripts/checks/map.ts: an unknown path runs everything`;
	}
	const areas = Object.fromEntries(
		names.map((a) => [a, { on: everything !== null || by[a]!.size > 0, because: describe(by[a]!) }]),
	);
	return { everything, areas, ignored, hits };
}

export interface Planned {
	id: string;
	area: string;
	cmd: string;
	why: string[];
}

export interface Plan extends Classified {
	/** The changed files, or undefined when the diff is unknown. */
	files: string[] | undefined;
	/** Rust: the whole workspace (a root manifest or toolchain changed, or everything runs). */
	workspace: boolean;
	/** Rust: the crates to check, with their dependents (empty when `workspace`). */
	crates: string[];
	/** What to run, in the order `check` runs it. Checks CI runs and a dev machine cannot are not here. */
	commands: Planned[];
}

export interface Opts {
	/** The workspace packages that have a library target (`workspaceMeta`). Unknown (undefined): assume all do. */
	libs?: string[];
	/** Also lint the Windows target through cargo-xwin (a machine catching what only the Windows CI job used to:
	 * `cfg(windows)` code, dead code there). */
	windows?: boolean;
}

/** The crate scope of the Rust files in `files`: the crates they belong to, or the whole workspace. */
function scope(files: string[], graph: Graph): { workspace: boolean; crates: string[] } {
	const touched = new Set<string>();
	let workspace = false;
	for (const f of files) {
		const embed = EMBEDS.find(([g]) => matches(g, f))?.[1];
		const own = /^crates\/([^/]+)\//.exec(f)?.[1];
		const crate = embed ?? own;
		if (crate && graph[crate]) touched.add(crate);
		else workspace = true; // Cargo.lock, deny.toml, a crate this workspace no longer has...
	}
	if (!workspace && touched.size === 0) workspace = true;
	return { workspace, crates: workspace ? [] : [...dependents(graph, touched)].sort() };
}

/** The plan for `files` (undefined: the diff is unknown, everything runs). `graph` is the workspace's crate graph,
 * read only when a change is confined to some crates. */
export function plan(files: string[] | undefined, graph: Graph = {}, opts: Opts = {}): Plan {
	const c = classify(files, "local");
	const rust = c.areas.rust!.on;
	const { workspace, crates } = rust
		? c.everything !== null
			? { workspace: true, crates: [] }
			: scope(c.hits.rust!, graph)
		: { workspace: false, crates: [] as string[] };
	const cargo = workspace ? "--workspace" : crates.map((x) => `-p ${x}`).join(" ");
	// Only packages with a library have doctests: a `-p` list of bin-only crates would make cargo fail.
	const libCrates = opts.libs ? crates.filter((x) => opts.libs!.includes(x)) : crates;
	const cargoLib = workspace ? "--workspace" : libCrates.map((x) => `-p ${x}`).join(" ");
	const commands: Planned[] = [];
	for (const k of CHECKS) {
		if (!k.cmd || !c.areas[k.area]!.on) continue;
		if (k.cmd.includes("{cargo-lib}") && cargoLib === "") continue;
		let why = c.everything ? [c.everything] : c.areas[k.area]!.because;
		if (k.only && c.everything === null) {
			const hit = (files ?? []).filter((f) => anyMatch(k.only!, f));
			if (hit.length === 0) continue;
			why = [`${k.only.join(" | ")} ← ${hit[0]}${hit.length > 1 ? ` (+${hit.length - 1} more)` : ""}`];
		}
		commands.push({ id: k.id, area: k.area, cmd: k.cmd.replaceAll("{cargo-lib}", cargoLib).replaceAll("{cargo}", cargo), why });
		// Own target dir: a Windows build would otherwise evict the host's from the shared cache.
		if (k.id === "nextest" && opts.windows) {
			commands.push({
				id: "clippy-windows",
				area: "rust",
				cmd: `CARGO_TARGET_DIR=target/xwin cargo xwin clippy ${cargo} --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings`,
				why,
			});
		}
	}
	return { ...c, files, workspace, crates, commands };
}

/** The checks CI runs that a dev machine cannot, for the areas in `p` (all of them when `p` is undefined). */
export function ciOnly(p?: Plan) {
	return CHECKS.filter((k) => !k.cmd && (!p || p.areas[k.area]!.on)).map((k) => ({
		id: k.id,
		area: k.area,
		ci: k.ci,
		why: k.why ?? "",
	}));
}
