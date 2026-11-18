/** tsconfig: the `extends` chain resolved into effective compiler options, each with its doc line. */
import { asRecord, asString, asStringArray, parseData } from "#lib/viewers/rich/data.ts";
import { m } from "#lib/i18n/m.ts";

export interface TsLayer {
	/** Path relative to the worktree (or the package specifier when it lives in node_modules). */
	path: string;
	options: Record<string, unknown>;
	missing: boolean;
}

export interface TsConfigModel {
	/** Base-first. The last layer is the file itself. */
	chain: TsLayer[];
	options: { key: string; value: unknown; from: string }[];
	include: string[];
	exclude: string[];
	files: string[];
	references: string[];
	error: string | null;
}

/** Short doc of a compiler option people actually look up, in the display language; empty for any other option. */
export function tsDoc(option: string): string {
	switch (option) {
		case "target":
			return m.viewers_reference_tsconfig_target();
		case "module":
			return m.viewers_reference_tsconfig_module();
		case "moduleResolution":
			return m.viewers_reference_tsconfig_module_resolution();
		case "lib":
			return m.viewers_reference_tsconfig_lib();
		case "jsx":
			return m.viewers_reference_tsconfig_jsx();
		case "strict":
			return m.viewers_reference_tsconfig_strict();
		case "noEmit":
			return m.viewers_reference_tsconfig_no_emit();
		case "outDir":
			return m.viewers_reference_tsconfig_out_dir();
		case "rootDir":
			return m.viewers_reference_tsconfig_root_dir();
		case "baseUrl":
			return m.viewers_reference_tsconfig_base_url();
		case "paths":
			return m.viewers_reference_tsconfig_paths();
		case "esModuleInterop":
			return m.viewers_reference_tsconfig_es_module_interop();
		case "skipLibCheck":
			return m.viewers_reference_tsconfig_skip_lib_check();
		case "declaration":
			return m.viewers_reference_tsconfig_declaration();
		case "sourceMap":
			return m.viewers_reference_tsconfig_source_map();
		case "resolveJsonModule":
			return m.viewers_reference_tsconfig_resolve_json_module();
		case "isolatedModules":
			return m.viewers_reference_tsconfig_isolated_modules();
		case "allowJs":
			return m.viewers_reference_tsconfig_allow_js();
		case "checkJs":
			return m.viewers_reference_tsconfig_check_js();
		case "incremental":
			return m.viewers_reference_tsconfig_incremental();
		case "composite":
			return m.viewers_reference_tsconfig_composite();
		case "types":
			return m.viewers_reference_tsconfig_types();
		case "noUncheckedIndexedAccess":
			return m.viewers_reference_tsconfig_no_unchecked_indexed_access();
		case "verbatimModuleSyntax":
			return m.viewers_reference_tsconfig_verbatim_module_syntax();
		case "allowImportingTsExtensions":
			return m.viewers_reference_tsconfig_allow_importing_ts_extensions();
		case "forceConsistentCasingInFileNames":
			return m.viewers_reference_tsconfig_force_consistent_casing();
		case "useDefineForClassFields":
			return m.viewers_reference_tsconfig_use_define_for_class_fields();
		case "experimentalDecorators":
			return m.viewers_reference_tsconfig_experimental_decorators();
		case "noImplicitAny":
			return m.viewers_reference_tsconfig_no_implicit_any();
		default:
			return "";
	}
}

/**
 * Resolve the chain starting at `path`. `read` returns a file's text or null; `resolve` turns an `extends`
 * specifier into a candidate path (relative to the file, or into node_modules).
 */
export async function resolveTsconfig(
	path: string,
	read: (path: string) => Promise<string | null>,
	resolve: (from: string, spec: string) => string[],
): Promise<TsConfigModel> {
	const chain: TsLayer[] = [];
	const seen = new Set<string>();
	let include: string[] = [];
	let exclude: string[] = [];
	let files: string[] = [];
	let references: string[] = [];
	let error: string | null = null;
	let cur: string | null = path;
	let first = true;
	while (cur && !seen.has(cur) && chain.length < 12) {
		seen.add(cur);
		const text: string | null = await read(cur);
		if (text === null) {
			chain.unshift({ path: cur, options: {}, missing: true });
			break;
		}
		const parsed = parseData(text, "json");
		if (!parsed.ok) {
			if (first) error = parsed.error;
			chain.unshift({ path: cur, options: {}, missing: false });
			break;
		}
		const r = asRecord(parsed.value);
		chain.unshift({ path: cur, options: asRecord(r.compilerOptions), missing: false });
		if (first) {
			include = asStringArray(r.include);
			exclude = asStringArray(r.exclude);
			files = asStringArray(r.files);
			references = (Array.isArray(r.references) ? r.references : [])
				.map((x) => asString(asRecord(x).path) ?? "")
				.filter(Boolean);
		}
		first = false;
		const ext = Array.isArray(r.extends) ? asString(r.extends[r.extends.length - 1]) : asString(r.extends);
		cur = null;
		if (ext) {
			for (const cand of resolve(chain[0].path, ext)) {
				if ((await read(cand)) !== null) {
					cur = cand;
					break;
				}
			}
			if (!cur) chain.unshift({ path: ext, options: {}, missing: true });
		}
	}
	const merged = new Map<string, { value: unknown; from: string }>();
	for (const l of chain)
		for (const [k, v] of Object.entries(l.options)) merged.set(k, { value: v, from: l.path });
	return {
		chain,
		options: [...merged].map(([key, x]) => ({ key, ...x })).sort((a, b) => a.key.localeCompare(b.key)),
		include,
		exclude,
		files,
		references,
		error,
	};
}

/** Candidate paths for `extends: spec` from `from` (a worktree-relative path). */
export function extendsCandidates(from: string, spec: string): string[] {
	const dir = from.split("/").slice(0, -1);
	const join = (base: string[], rel: string) => {
		const out = [...base];
		for (const seg of rel.split("/")) {
			if (seg === "..") out.pop();
			else if (seg !== "." && seg) out.push(seg);
		}
		return out.join("/");
	};
	if (spec.startsWith(".")) {
		const p = join(dir, spec);
		return p.endsWith(".json") ? [p] : [`${p}.json`, `${p}/tsconfig.json`];
	}
	const up: string[] = [];
	const cands: string[] = [];
	for (let d = [...dir]; ; d.pop()) {
		const nm = [...d, "node_modules", spec].join("/");
		cands.push(nm.endsWith(".json") ? nm : `${nm}.json`, `${nm}/tsconfig.json`);
		if (!d.length) break;
	}
	return [...up, ...cands];
}
