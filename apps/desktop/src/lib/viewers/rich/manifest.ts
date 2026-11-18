/** Package manifests (package.json, Cargo.toml, pyproject.toml, go.mod, composer.json) as one display model. */
import {
	asRecord,
	asString,
	asStringArray,
	isRecord,
	parseData,
	type Parsed,
} from "#lib/viewers/rich/data.ts";
import { m } from "#lib/i18n/m.ts";

export interface Dep {
	name: string;
	range: string;
	/** Extra qualifiers: features, `optional`, `path`, `git`, `indirect`. */
	note: string;
}

export interface DepGroup {
	title: string;
	deps: Dep[];
}

export interface Script {
	name: string;
	command: string;
}

export interface Manifest {
	kind: "npm" | "cargo" | "python" | "go" | "composer";
	name: string;
	version: string;
	description: string;
	license: string;
	/** Header facts: engines, edition, requires-python… Their labels follow the display language: never look one up by it. */
	facts: { label: string; value: string }[];
	/** The `packageManager` field of a package.json (`pnpm@9.1.0`), the one fact code reads. */
	packageManager?: string;
	links: { label: string; url: string }[];
	scripts: Script[];
	groups: DepGroup[];
	/** Workspace members / globs. */
	workspaces: string[];
	/** Registry page for a dependency, null when unknown. */
	registry: string;
}

const deps = (v: unknown): Dep[] =>
	Object.entries(asRecord(v)).map(([name, r]) => {
		if (typeof r === "string") return { name, range: r, note: "" };
		const o = asRecord(r);
		const note = [
			Array.isArray(o.features) ? `features: ${asStringArray(o.features).join(", ")}` : "",
			o.optional ? "optional" : "",
			o.path ? `path ${String(o.path)}` : "",
			o.git ? "git" : "",
			o.workspace ? "workspace" : "",
		]
			.filter(Boolean)
			.join(" · ");
		return { name, range: asString(o.version) ?? (o.workspace ? "workspace" : o.path ? "local" : "*"), note };
	});

const group = (title: string, v: unknown): DepGroup[] => {
	const d = deps(v);
	return d.length ? [{ title, deps: d }] : [];
};

const base = (kind: Manifest["kind"], registry: string): Manifest => ({
	kind,
	name: "",
	version: "",
	description: "",
	license: "",
	facts: [],
	links: [],
	scripts: [],
	groups: [],
	workspaces: [],
	registry,
});

const fact = (label: string, v: unknown): { label: string; value: string }[] =>
	v === undefined || v === null || v === ""
		? []
		: [{ label, value: typeof v === "object" ? JSON.stringify(v) : String(v) }];

export function npmManifest(v: unknown): Manifest {
	const r = asRecord(v);
	const model = base("npm", "https://www.npmjs.com/package/");
	const ws = Array.isArray(r.workspaces)
		? asStringArray(r.workspaces)
		: asStringArray(asRecord(r.workspaces).packages);
	const engines = Object.entries(asRecord(r.engines)).map(([k, x]) => `${k} ${String(x)}`);
	const repo = isRecord(r.repository) ? asString(r.repository.url) : asString(r.repository);
	return {
		...model,
		name: asString(r.name) ?? "",
		version: asString(r.version) ?? "",
		description: asString(r.description) ?? "",
		license: typeof r.license === "string" ? r.license : "",
		packageManager: typeof r.packageManager === "string" ? r.packageManager : undefined,
		facts: [
			...fact(m.viewers_reference_manifest_engines(), engines.join(", ")),
			...fact(m.viewers_reference_manifest_package_manager(), r.packageManager),
			...fact(m.viewers_reference_manifest_type(), r.type),
			...fact(m.viewers_reference_manifest_main(), r.main ?? r.module),
			...fact(m.viewers_reference_manifest_private(), r.private ? m.viewers_reference_manifest_yes() : ""),
		],
		links: [
			...(asString(r.homepage)
				? [{ label: m.viewers_reference_manifest_homepage(), url: asString(r.homepage)! }]
				: []),
			...(repo
				? [
						{
							label: m.viewers_reference_manifest_repository(),
							url: repo.replace(/^git\+/, "").replace(/\.git$/, ""),
						},
					]
				: []),
		],
		scripts: Object.entries(asRecord(r.scripts)).map(([name, command]) => ({
			name,
			command: String(command),
		})),
		groups: [
			...group("dependencies", r.dependencies),
			...group("devDependencies", r.devDependencies),
			...group("peerDependencies", r.peerDependencies),
			...group("optionalDependencies", r.optionalDependencies),
		],
		workspaces: ws,
	};
}

export function composerManifest(v: unknown): Manifest {
	const r = asRecord(v);
	return {
		...base("composer", "https://packagist.org/packages/"),
		name: asString(r.name) ?? "",
		version: asString(r.version) ?? "",
		description: asString(r.description) ?? "",
		license: Array.isArray(r.license) ? asStringArray(r.license).join(", ") : (asString(r.license) ?? ""),
		facts: [
			...fact(m.viewers_reference_manifest_type(), r.type),
			...fact(m.viewers_reference_manifest_minimum_stability(), r["minimum-stability"]),
		],
		scripts: Object.entries(asRecord(r.scripts)).map(([name, c]) => ({
			name,
			command: Array.isArray(c) ? c.join(" && ") : String(c),
		})),
		groups: [...group("require", r.require), ...group("require-dev", r["require-dev"])],
	};
}

export function cargoManifest(v: unknown): Manifest {
	const r = asRecord(v);
	const p = { ...asRecord(r.workspace && asRecord(r.workspace).package), ...asRecord(r.package) };
	const ws = asRecord(r.workspace);
	const wsDeps = asRecord(ws.dependencies);
	const target = Object.entries(asRecord(r.target)).flatMap(([t, c]) =>
		group(`target ${t}`, asRecord(c).dependencies),
	);
	return {
		...base("cargo", "https://crates.io/crates/"),
		name: asString(p.name) ?? "",
		version: typeof p.version === "string" ? p.version : isRecord(p.version) ? "workspace" : "",
		description: typeof p.description === "string" ? p.description : "",
		license: typeof p.license === "string" ? p.license : "",
		facts: [...fact(m.viewers_reference_manifest_edition(), p.edition), ...fact("Rust", p["rust-version"])],
		links: [
			...(typeof p.repository === "string"
				? [{ label: m.viewers_reference_manifest_repository(), url: p.repository }]
				: []),
			...(typeof p.homepage === "string"
				? [{ label: m.viewers_reference_manifest_homepage(), url: p.homepage }]
				: []),
		],
		groups: [
			...group("dependencies", r.dependencies),
			...group("dev-dependencies", r["dev-dependencies"]),
			...group("build-dependencies", r["build-dependencies"]),
			...group("workspace.dependencies", wsDeps),
			...target,
		],
		workspaces: asStringArray(ws.members),
		scripts: Object.keys(asRecord(r.features)).map((f) => ({
			name: f,
			command: asStringArray(asRecord(r.features)[f]).join(", "),
		})),
	};
}

/** PEP 508 requirement `name[extra]>=1.0; marker` → dependency row. */
export function pepDep(s: string): Dep {
	const req = /^\s*([A-Za-z0-9_.-]+)\s*(\[[^\]]*\])?\s*([^;]*)(;.*)?$/.exec(s);
	if (!req) return { name: s, range: "", note: "" };
	return {
		name: req[1],
		range: req[3].trim() || "*",
		note: [req[2]?.slice(1, -1) && `extras: ${req[2].slice(1, -1)}`, req[4]?.slice(1).trim()]
			.filter(Boolean)
			.join(" · "),
	};
}

export function pyprojectManifest(v: unknown): Manifest {
	const r = asRecord(v);
	const proj = asRecord(r.project);
	const poetry = asRecord(asRecord(r.tool).poetry);
	const lic = proj.license;
	const toDeps = (list: unknown): Dep[] => asStringArray(list).map(pepDep);
	const poetryDeps = (o: unknown) =>
		deps(Object.fromEntries(Object.entries(asRecord(o)).filter(([k]) => k !== "python")));
	const opt = Object.entries(asRecord(proj["optional-dependencies"])).map(([k, l]) => ({
		title: `optional: ${k}`,
		deps: toDeps(l),
	}));
	const groups: DepGroup[] = [
		{ title: "dependencies", deps: [...toDeps(proj.dependencies), ...poetryDeps(poetry.dependencies)] },
		...opt,
		...Object.entries(asRecord(r["dependency-groups"])).map(([k, l]) => ({
			title: `group: ${k}`,
			deps: toDeps(l),
		})),
		{
			title: "dev-dependencies",
			deps: poetryDeps(asRecord(poetry.group && asRecord(poetry.group).dev).dependencies),
		},
	].filter((g) => g.deps.length);
	return {
		...base("python", "https://pypi.org/project/"),
		name: asString(proj.name) ?? asString(poetry.name) ?? "",
		version: asString(proj.version) ?? asString(poetry.version) ?? "",
		description: asString(proj.description) ?? asString(poetry.description) ?? "",
		license: typeof lic === "string" ? lic : (asString(asRecord(lic).text) ?? asString(poetry.license) ?? ""),
		facts: [
			...fact("Python", proj["requires-python"] ?? asRecord(poetry.dependencies).python),
			...fact(m.viewers_reference_manifest_build_backend(), asRecord(r["build-system"])["build-backend"]),
		],
		scripts: Object.entries({ ...asRecord(proj.scripts), ...asRecord(poetry.scripts) }).map(
			([name, command]) => ({ name, command: String(command) }),
		),
		groups,
	};
}

/** go.mod is not TOML/JSON: parse the directive blocks. */
export function goMod(text: string): Manifest {
	const model = base("go", "https://pkg.go.dev/");
	const groups: DepGroup[] = [
		{ title: "require", deps: [] },
		{ title: "indirect", deps: [] },
	];
	const facts: Manifest["facts"] = [];
	let block = "";
	for (const raw of text.replace(/\r\n?/g, "\n").split("\n")) {
		const l = raw.replace(/\/\/(?!\s*indirect).*$/, "").trim();
		if (!l) continue;
		if (block) {
			if (l === ")") {
				block = "";
				continue;
			}
			addGo(block, l, groups, facts);
			continue;
		}
		const b = /^(\w+)\s*\($/.exec(l);
		if (b) {
			block = b[1];
			continue;
		}
		const d = /^(\w+)\s+(.*)$/.exec(l);
		if (!d) continue;
		if (d[1] === "module") model.name = d[2];
		else if (d[1] === "go") facts.push({ label: "Go", value: d[2] });
		else if (d[1] === "toolchain") facts.push({ label: "Toolchain", value: d[2] });
		else addGo(d[1], d[2], groups, facts);
	}
	return { ...model, facts, groups: groups.filter((g) => g.deps.length) };
}

function addGo(kind: string, l: string, groups: DepGroup[], facts: Manifest["facts"]) {
	const p = l.split(/\s+/);
	if (kind === "require")
		(l.includes("// indirect") ? groups[1] : groups[0]).deps.push({
			name: p[0],
			range: p[1] ?? "",
			note: "",
		});
	else if (kind === "replace") facts.push({ label: m.viewers_reference_manifest_replace(), value: l });
	else if (kind === "exclude" || kind === "retract") facts.push({ label: kind, value: l });
}

/** Choose the model from the file name; `null` when the text does not parse. */
export function manifestFor(
	id: string,
	text: string,
): { manifest: Manifest } | { error: string; line?: number } {
	if (id === "go-mod") return { manifest: goMod(text) };
	const parsed: Parsed = parseData(text, id === "composer" ? "json" : "toml");
	if (!parsed.ok) return { error: parsed.error, line: parsed.line };
	return {
		manifest:
			id === "composer"
				? composerManifest(parsed.value)
				: id === "cargo"
					? cargoManifest(parsed.value)
					: pyprojectManifest(parsed.value),
	};
}

/** Package manager from the lockfiles next to a package.json (`bun`, `pnpm`, `yarn`, `npm`). */
export function detectPackageManager(siblings: string[], declared?: string): "bun" | "pnpm" | "yarn" | "npm" {
	const d = declared?.split("@")[0];
	if (d === "bun" || d === "pnpm" || d === "yarn" || d === "npm") return d;
	if (siblings.some((f) => f === "bun.lock" || f === "bun.lockb")) return "bun";
	if (siblings.includes("pnpm-lock.yaml")) return "pnpm";
	if (siblings.includes("yarn.lock")) return "yarn";
	return "npm";
}

/** Argv for running a script (`bun run dev`, `yarn dev`, `npm run dev`). */
export function runScriptArgv(pm: string, script: string): string[] {
	return pm === "yarn" ? ["yarn", script] : [pm, "run", script];
}
