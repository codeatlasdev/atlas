// What the desktop frontend loads at startup, and a budget for it (#288).
//
// `bun scripts/bundle-budget.ts` builds apps/desktop exactly like `vite build` (it adds one plugin that reads the
// finished chunks), takes the static import closure of the entry chunks and of the main route's nodes (what the page
// has to download and run before the shell shows), and checks it against apps/desktop/bundle-budget.json:
//
//  - the gzip size of that JS and CSS may not exceed the budget;
//  - none of the heavy packages in `forbidden` (CodeMirror, xterm, yaml, marked, prettier, echarts) may be reachable from
//    it, except what `known` lists: the violations that existed when the check was added, each with the reason. `known`
//    only shrinks: an entry that no longer matches anything fails the check until it is removed;
//  - a budget more than `slack` above the measured size fails too, so an improvement is written down by lowering it
//    (`bun scripts/bundle-budget.ts --ratchet` does that, and never raises anything).
//
// docs/desktop.md ("Bundle") says heavy views are lazy; this is what keeps it true.
import { gzipSync } from "node:zlib";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

export interface Chunk {
	file: string;
	name: string;
	/** Static imports (chunk files). */
	imports: string[];
	/** Module ids bundled into it (JS and CSS). */
	modules: string[];
	/** Stylesheets it pulls in (asset files). */
	css: string[];
	size: number;
	gzip: number;
}

export interface Asset {
	file: string;
	gzip: number;
}

export interface Budget {
	/** Gzip bytes of the JS an app start downloads. */
	jsGzip: number;
	cssGzip: number;
	/** The budget may be this much above the measured size before the check asks to lower it. */
	slack: number;
	/** Package names (`yaml`, `@codemirror/*`) that must not be reachable from startup. */
	forbidden: string[];
	/** Reachable today, with the reason; shrinks only. */
	known: { package: string; why: string }[];
}

export interface Report {
	jsGzip: number;
	cssGzip: number;
	jsRaw: number;
	chunks: { file: string; name: string; gzip: number; size: number }[];
	/** Package name -> chunk names it is bundled into. */
	packages: Record<string, string[]>;
}

/** `@codemirror/state` for `/x/node_modules/@codemirror/state/dist/index.js`; null for the app's own modules. */
export function packageOf(id: string): string | null {
	const at = id.lastIndexOf("/node_modules/");
	if (at < 0) return null;
	const [a, b] = id
		.slice(at + "/node_modules/".length)
		.split("?")[0]
		.split("/");
	return a.startsWith("@") ? `${a}/${b}` : a;
}

/** `yaml` matches itself; `@codemirror/*` matches every package under the scope. */
export function matches(pattern: string, pkg: string): boolean {
	return pattern.endsWith("/*")
		? pkg.startsWith(pattern.slice(0, -1))
		: pkg === pattern;
}

/**
 * The startup closure: `roots` are chunk names (`entry/start`, `nodes/2`, ...), followed through static imports only
 * (a dynamic `import()` is a lazy chunk and not part of it).
 */
export function analyze(
	chunks: Chunk[],
	assets: Asset[],
	roots: string[],
): Report {
	const byFile = new Map(chunks.map((c) => [c.file, c]));
	const byName = new Map(chunks.map((c) => [c.name, c]));
	const seen = new Set<string>();
	const stack: string[] = [];
	for (const r of roots) {
		const c = byName.get(r);
		if (!c)
			throw new Error(
				`no chunk named "${r}" in the build (have: ${[...byName.keys()].slice(0, 12).join(", ")}...)`,
			);
		stack.push(c.file);
	}
	while (stack.length) {
		const f = stack.pop()!;
		if (seen.has(f)) continue;
		seen.add(f);
		const c = byFile.get(f);
		if (!c) throw new Error(`chunk ${f} is imported but not in the build`);
		stack.push(...c.imports);
	}
	const inside = [...seen].map((f) => byFile.get(f)!);
	const css = new Set(inside.flatMap((c) => c.css));
	const sheets = new Map(assets.map((a) => [a.file, a.gzip]));
	const packages: Record<string, string[]> = {};
	for (const c of inside)
		for (const m of c.modules) {
			const p = packageOf(m);
			if (!p) continue;
			const names = (packages[p] ??= []);
			if (!names.includes(c.name)) names.push(c.name);
		}
	return {
		jsGzip: inside.reduce((n, c) => n + c.gzip, 0),
		jsRaw: inside.reduce((n, c) => n + c.size, 0),
		cssGzip: [...css].reduce((n, f) => n + (sheets.get(f) ?? 0), 0),
		chunks: inside
			.map((c) => ({ file: c.file, name: c.name, gzip: c.gzip, size: c.size }))
			.sort((a, b) => b.gzip - a.gzip),
		packages,
	};
}

const kib = (n: number) => `${(n / 1024).toFixed(1)} KiB`;

/** What is wrong with `report` under `budget` (empty: fine). */
export function evaluate(report: Report, budget: Budget): string[] {
	const out: string[] = [];
	if (report.jsGzip > budget.jsGzip)
		out.push(
			`startup JS is ${kib(report.jsGzip)} gzip, over the budget of ${kib(budget.jsGzip)}: load what grew lazily (components/Lazy.svelte). ` +
				`If it has to grow, raise jsGzip in apps/desktop/bundle-budget.json in this PR and say why.`,
		);
	if (report.cssGzip > budget.cssGzip)
		out.push(
			`startup CSS is ${kib(report.cssGzip)} gzip, over the budget of ${kib(budget.cssGzip)}.`,
		);
	const reachable = Object.keys(report.packages);
	for (const p of reachable) {
		if (!budget.forbidden.some((f) => matches(f, p))) continue;
		if (budget.known.some((k) => matches(k.package, p))) continue;
		out.push(
			`${p} is reachable from startup (in ${report.packages[p].join(", ")}): import it only from a lazily loaded view (docs/desktop.md, "Bundle").`,
		);
	}
	for (const k of budget.known)
		if (!reachable.some((p) => matches(k.package, p)))
			out.push(
				`known violation "${k.package}" is gone from startup: delete it from "known" in apps/desktop/bundle-budget.json.`,
			);
	for (const [what, have, want] of [
		["jsGzip", report.jsGzip, budget.jsGzip],
		["cssGzip", report.cssGzip, budget.cssGzip],
	] as const)
		if (want - have > budget.slack)
			out.push(
				`${what} is ${kib(have)} but the budget still says ${kib(want)}: lower it (bun scripts/bundle-budget.ts --ratchet) so the gain stays.`,
			);
	return out;
}

/** The budget after an improvement: lower sizes (rounded up to a KiB), stale `known` dropped; nothing is ever raised. */
export function ratchet(report: Report, budget: Budget): Budget {
	const up = (n: number) => Math.ceil(n / 1024) * 1024;
	const reachable = Object.keys(report.packages);
	return {
		...budget,
		jsGzip: Math.min(budget.jsGzip, up(report.jsGzip)),
		cssGzip: Math.min(budget.cssGzip, up(report.cssGzip)),
		known: budget.known.filter((k) =>
			reachable.some((p) => matches(k.package, p)),
		),
	};
}

/** The nodes `/` loads: the root layout, the layouts of the `(app)` group, then its page (from SvelteKit's generated manifest). */
export function mainRouteNodes(appJs: string): string[] {
	const m = /"\/\(app\)":\s*\[(\d+)\s*,\s*\[([\d,\s]*)\]\s*\]/.exec(appJs);
	if (!m)
		throw new Error(
			'the generated client manifest has no "/(app)" route: update mainRouteNodes()',
		);
	const layouts = m[2]
		.split(",")
		.map((s) => s.trim())
		.filter(Boolean);
	return ["0", ...layouts, m[1]].map((n) => `nodes/${n}`);
}

/** The parts of a rollup output bundle this script reads (the build's own types live in apps/desktop's vite). */
type OutputItem =
	| {
			type: "chunk";
			name: string;
			code: string;
			imports: string[];
			modules: Record<string, unknown>;
			viteMetadata?: { importedCss?: Set<string> };
	  }
	| { type: "asset"; source: string | Uint8Array };

async function main() {
	const root = resolve(import.meta.dir, "..");
	const desktop = join(root, "apps/desktop");
	const budgetPath = join(desktop, "bundle-budget.json");
	process.chdir(desktop);

	const chunks: Chunk[] = [];
	const assets: Asset[] = [];
	let roots: string[] = [];
	const probe = {
		name: "atlas:bundle-budget",
		apply: "build",
		generateBundle(
			this: { environment?: { name: string } },
			_options: unknown,
			bundle: Record<string, OutputItem>,
		) {
			if (this.environment?.name !== "client") return;
			for (const [file, c] of Object.entries(bundle)) {
				if (c.type === "chunk")
					chunks.push({
						file,
						name: c.name,
						imports: c.imports,
						modules: Object.keys(c.modules),
						css: [...(c.viteMetadata?.importedCss ?? [])],
						size: Buffer.byteLength(c.code),
						gzip: gzipSync(c.code).length,
					});
				else assets.push({ file, gzip: gzipSync(c.source).length });
			}
			const generated = join(
				desktop,
				".svelte-kit/generated/build/client-optimized/app.js",
			);
			roots = [
				"entry/start",
				"entry/app",
				...mainRouteNodes(readFileSync(generated, "utf8")),
			];
			if (chunks.some((c) => c.name === "entry/payload"))
				roots.push("entry/payload");
		},
	};
	const icons = Bun.spawnSync(["bun", "scripts/gen-icons.ts"], {
		cwd: desktop,
		stdout: "inherit",
		stderr: "inherit",
	});
	if (icons.exitCode !== 0) process.exit(icons.exitCode);
	// Vite is apps/desktop's dependency, not the root's: the specifier is resolved from there at run time.
	const { build } = (await import(Bun.resolveSync("vite", desktop))) as {
		build: (config: Record<string, unknown>) => Promise<unknown>;
	};
	await build({
		root: desktop,
		configFile: join(desktop, "vite.config.ts"),
		plugins: [probe],
		logLevel: "warn",
	});

	const report = analyze(chunks, assets, roots);
	const budget: Budget = JSON.parse(readFileSync(budgetPath, "utf8"));
	writeFileSync(
		join(desktop, ".svelte-kit/bundle-report.json"),
		JSON.stringify(report, null, 1),
	);

	const lines = [
		`startup JS ${kib(report.jsGzip)} gzip (${kib(report.jsRaw)} raw) in ${report.chunks.length} chunks, CSS ${kib(report.cssGzip)} gzip; budget ${kib(budget.jsGzip)} / ${kib(budget.cssGzip)}`,
		"largest chunks (gzip):",
		...report.chunks
			.slice(0, 12)
			.map((c) => `  ${kib(c.gzip).padStart(10)}  ${c.name}  ${c.file}`),
		"forbidden packages reachable (all must be in `known`):",
		...Object.keys(report.packages)
			.filter((p) => budget.forbidden.some((f) => matches(f, p)))
			.map((p) => `  ${p}  in ${report.packages[p].join(", ")}`),
	];
	console.log(lines.join("\n"));
	if (process.env.GITHUB_STEP_SUMMARY)
		writeFileSync(
			process.env.GITHUB_STEP_SUMMARY,
			`### Startup bundle\n\n\`\`\`\n${lines.join("\n")}\n\`\`\`\n`,
			{ flag: "a" },
		);

	if (process.argv.includes("--ratchet")) {
		const next = ratchet(report, budget);
		writeFileSync(budgetPath, `${JSON.stringify(next, null, "\t")}\n`);
		console.log(
			`budget now ${kib(next.jsGzip)} JS / ${kib(next.cssGzip)} CSS, ${next.known.length} known violations`,
		);
		return;
	}
	const problems = evaluate(report, budget);
	for (const p of problems) console.error(`bundle budget: ${p}`);
	if (problems.length) process.exit(1);
}

if (import.meta.main) await main();
