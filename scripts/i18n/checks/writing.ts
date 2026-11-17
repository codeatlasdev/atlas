// The writing lint (i18n/style/flagged-terms.yaml): em dashes, emoji, hype and AI-tell words, pt-BR calques.
//   catalog values (scope ui)    every locale of every surface, at the severity the YAML gives
//   docs/<locale>/**             new by policy: the YAML severity
//   English docs, README, skills, site content, CHANGELOG   existing prose: error-level hits are counted against the
//                                baseline (they may not grow); everything else is info
// Waive one hit with a reason: `<!-- writing-lint: allow RULE_ID: reason -->` on the line before (Markdown),
// the same text in the key's description (meta.json); it covers every locale of the key.
import { join } from "node:path";
import type { Entry } from "../model.ts";
import { paraglideTexts } from "../shape.ts";
import { loadSource } from "../source.ts";
import { WRITING, runRatchet, scanRatchet } from "../ratchets.ts";
import type { Checker, Finding } from "../types.ts";
import { allowsIn, docFiles, hitMessage, lintMarkdown, lintText, loadRules, uiProse } from "../writing.ts";
import { readText } from "../util.ts";

const checker: Checker = {
	name: "writing",

	async run(ctx) {
		if (ctx.surface && ctx.surface !== "writing") return [];
		const { rules, problems } = loadRules(ctx.root);
		const out: Finding[] = [...problems];
		if (rules.length === 0) return out;
		const all = { ...ctx, surface: undefined };

		for (const area of loadSource(all).areas.values()) {
			const base = area.entries.get(area.base);
			for (const [locale, entries] of area.entries) {
				for (const e of entries.values()) out.push(...uiFindings(rules, e, locale, base?.get(e.key)?.description ?? ""));
			}
		}

		const { strict, existing } = docFiles(ctx.root, Object.keys(ctx.locales.locales).filter((l) => l !== ctx.locales.default));
		for (const { file, lang } of strict) {
			const r = lintMarkdown(rules, readText(join(ctx.root, file)), lang);
			for (const n of r.noReason) out.push({ rule: "writing-allow", severity: "error", file, line: n.line, message: `allow ${n.id} needs a reason: <!-- writing-lint: allow ${n.id}: why -->` });
			for (const h of r.hits) out.push({ rule: `writing-${h.rule.id}`, severity: h.rule.severity, file, line: h.line, message: hitMessage(h) });
		}
		for (const file of existing) {
			const r = lintMarkdown(rules, readText(join(ctx.root, file)), "en");
			for (const n of r.noReason) out.push({ rule: "writing-allow", severity: "error", file, line: n.line, message: `allow ${n.id} needs a reason: <!-- writing-lint: allow ${n.id}: why -->` });
			const soft = r.hits.filter((h) => h.rule.severity === "warn");
			if (soft.length) out.push({ rule: "writing", severity: "info", file, message: `${soft.length} warning-level hit(s) in existing prose (${[...new Set(soft.map((h) => h.rule.id))].join(", ")})` });
		}
		const { found } = scanRatchet(all, WRITING);
		for (const [file, v] of found) out.push({ rule: "writing", severity: "info", file, message: `${v.length} flagged phrase(s) tolerated by the baseline` });
		out.push(...runRatchet(ctx, WRITING));
		return out;
	},
};

/** Hits in one catalog value, at the YAML severity, minus those the key's description waives. */
function uiFindings(rules: Parameters<typeof lintText>[0], e: Entry, locale: string, description: string): Finding[] {
	const waived = allowsIn(description);
	const out: Finding[] = waived.noReason.map((id) => ({ rule: "writing-allow", severity: "error" as const, file: e.file, line: e.line, key: e.key, message: `allow ${id} needs a reason: writing-lint: allow ${id}: why` }));
	for (const text of paraglideTexts(e.value)) {
		for (const h of lintText(rules, uiProse(text), locale, "ui")) {
			if (waived.ids.has(h.rule.id)) continue;
			out.push({ rule: `writing-${h.rule.id}`, severity: h.rule.severity, file: e.file, line: e.line, key: e.key, message: hitMessage(h) });
		}
	}
	return out;
}

export default checker;
