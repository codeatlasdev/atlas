// The languages must be listed identically everywhere one is listed. i18n/locales.json is the truth; these must agree:
//   crates/atlas-core/src/lang.rs      `pub const SUPPORTED: &[&str] = &[...]`
//   crates/atlas-desktop/Info.plist    CFBundleLocalizations
//   <app>/project.inlang/settings.json `locales` of each Paraglide surface
// A file that does not exist (a tree without Rust, say) is skipped. `bun run i18n:add-locale <tag>` edits all of them.
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { loadSurfacesConfig } from "../source.ts";
import type { Checker, Finding } from "../types.ts";
import { LOCALES_FILE } from "../types.ts";
import { readText } from "../util.ts";

const LANG_RS = "crates/atlas-core/src/lang.rs";
const PLIST = "crates/atlas-desktop/Info.plist";

const tags = (s: string) => [...s.matchAll(/"([^"]+)"/g)].map((m) => m[1]);

/** The language list of one place, or undefined when the place does not exist or has no such list. */
export function supportedIn(root: string): { file: string; what: string; tags: string[] }[] {
	const out: { file: string; what: string; tags: string[] }[] = [];
	const lang = join(root, LANG_RS);
	if (existsSync(lang)) {
		const m = /pub const SUPPORTED: &\[&str\] = &\[([^\]]*)\]/.exec(readText(lang));
		out.push({ file: LANG_RS, what: "SUPPORTED", tags: m ? tags(m[1]) : [] });
	}
	const plist = join(root, PLIST);
	if (existsSync(plist)) {
		const m = /<key>CFBundleLocalizations<\/key>\s*<array>([\s\S]*?)<\/array>/.exec(readText(plist));
		out.push({ file: PLIST, what: "CFBundleLocalizations", tags: m ? [...m[1].matchAll(/<string>([^<]+)<\/string>/g)].map((x) => x[1]) : [] });
	}
	for (const def of Object.values(loadSurfacesConfig(root).config.surfaces)) {
		if (!def.paraglide) continue;
		const file = `${dirname(def.paraglide)}/project.inlang/settings.json`;
		if (!existsSync(join(root, file))) continue;
		try {
			const j = JSON.parse(readText(join(root, file)));
			out.push({ file, what: "locales", tags: Array.isArray(j.locales) ? j.locales.map(String) : [] });
		} catch {
			out.push({ file, what: "locales", tags: [] });
		}
	}
	return out;
}

const checker: Checker = {
	name: "supported-sync",
	async run(ctx) {
		if (ctx.surface) return [];
		const want = Object.keys(ctx.locales.locales);
		const out: Finding[] = [];
		for (const p of supportedIn(ctx.root)) {
			const missing = want.filter((t) => !p.tags.includes(t));
			const extra = p.tags.filter((t) => !want.includes(t));
			if (!missing.length && !extra.length) continue;
			const fix = missing.length ? `add ${missing.join(", ")} here, or add the language everywhere with bun run i18n:add-locale ${missing[0]}` : `remove ${extra.join(", ")} from ${p.file}, or add the language to ${LOCALES_FILE} with bun run i18n:add-locale ${extra[0]}`;
			out.push({ rule: "supported-sync", severity: "error", file: p.file, message: `${p.what} is [${p.tags.join(", ")}] but ${LOCALES_FILE} has [${want.join(", ")}]${missing.length ? `; missing ${missing.join(", ")}` : ""}${extra.length ? `; extra ${extra.join(", ")}` : ""}: ${fix}` });
		}
		return out;
	},
};

export default checker;
