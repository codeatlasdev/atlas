// Paraglide / inlang message-format catalogs (apps/desktop, site, ...): every `**/project.inlang/settings.json`
// is a surface named after its app directory. Rules: missing, outdated, unstamped, orphan, unused, key, placeholder,
// verbatim, plural, description, dnt, glossary, pt-pt, format, lock, config, catalog, and the hardcoded-text ratchet.
import { join } from "node:path";
import { checkSurface, statusSurface } from "../engine.ts";
import { loadParaglide, loadPlatformKeys, PLATFORM_KEYS_FILE } from "../formats/paraglide.ts";
import { SVELTE, runRatchet } from "../ratchets.ts";
import type { Checker, Finding } from "../types.ts";
import { isUsed, scanRefs } from "../unused.ts";

const checker: Checker = {
	name: "paraglide-json",

	async run(ctx) {
		const out: Finding[] = [];
		const projects = loadParaglide(ctx);
		const platformKeys = loadPlatformKeys(ctx.root);
		out.push(...platformKeys.problems);
		if (!ctx.surface) {
			for (const key of platformKeys.map.keys()) {
				if (!projects.some((p) => p.surface.entries.get(p.surface.base)?.has(key))) {
					out.push({ rule: "platform-keys", severity: "error", file: PLATFORM_KEYS_FILE, key, message: "flags a key that is not in any English catalog" });
				}
			}
		}
		for (const p of projects) {
			if (ctx.surface && ctx.surface !== p.surface.name) continue;
			out.push(...checkSurface(ctx, p.surface));
			const base = p.surface.entries.get(p.surface.base);
			if (base?.size) {
				const refs = scanRefs(join(p.dir, "src"));
				for (const e of base.values()) {
					if (!isUsed(refs, e.key)) {
						out.push({ rule: "unused", severity: "error", file: e.file, line: e.line, key: e.key, message: "no `m.key(` / `m[\"key\"]` / `messages_` reference in src; delete it, or annotate dynamic use with `// i18n-keys: prefix_*`" });
					}
				}
			}
		}
		out.push(...runRatchet(ctx, SVELTE));
		return out;
	},

	async status(ctx) {
		return loadParaglide(ctx)
			.filter((p) => !ctx.surface || ctx.surface === p.surface.name)
			.flatMap((p) => statusSurface(ctx, p.surface));
	},
};

export default checker;
