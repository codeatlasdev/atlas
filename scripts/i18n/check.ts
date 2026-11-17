// bun run i18n:check [--json] [--surface <name>] [--status] [--info] [--root <dir>]
// The one i18n gate: loads every scripts/i18n/checks/*.ts (a checker is `export default { name, run(ctx), status?(ctx) }`),
// runs them offline, prints one `file:line rule key message` line per finding and exits 1 on any error.
//   --json      machine-readable result for agents: { ok, errors, warnings, info, findings: [...] }
//   --surface   only one surface (desktop, site, rust, docs, ...)
//   --status    per-locale, per-area freshness table instead of findings
//   --info      also print the informational lines (e.g. docs without a translation yet)
import { join } from "node:path";
import { type Checker, type Ctx, type Finding, type StatusRow, formatFinding, loadCtx } from "./types.ts";
import { cmp } from "./util.ts";

const CHECKS_DIR = join(import.meta.dir, "checks");

export async function loadCheckers(dir = CHECKS_DIR): Promise<{ checkers: Checker[]; problems: Finding[] }> {
	const checkers: Checker[] = [];
	const problems: Finding[] = [];
	for (const name of [...new Bun.Glob("*.ts").scanSync(dir)].sort()) {
		const file = join(dir, name);
		const mod = await import(file);
		const c = mod.default;
		if (c && typeof c.name === "string" && typeof c.run === "function") checkers.push(c);
		else problems.push({ rule: "checker", severity: "error", file: name, message: "a checker file must `export default { name: string, run(ctx): Promise<Finding[]> }`" });
	}
	return { checkers, problems };
}

export async function runChecks(ctx: Ctx, checkersDir?: string): Promise<Finding[]> {
	const { checkers, problems } = await loadCheckers(checkersDir);
	const out = [...problems];
	for (const c of checkers) {
		try {
			out.push(...(await c.run(ctx)));
		} catch (e) {
			out.push({ rule: "checker", severity: "error", file: `scripts/i18n/checks/${c.name}.ts`, message: `checker "${c.name}" crashed: ${(e as Error).stack ?? e}` });
		}
	}
	return out.sort((a, b) => cmp(a.file, b.file) || (a.line ?? 0) - (b.line ?? 0) || cmp(a.rule, b.rule) || cmp(a.key ?? "", b.key ?? ""));
}

export async function runStatus(ctx: Ctx, checkersDir?: string): Promise<StatusRow[]> {
	const { checkers } = await loadCheckers(checkersDir);
	const rows: StatusRow[] = [];
	for (const c of checkers) rows.push(...((await c.status?.(ctx)) ?? []));
	return rows.sort((a, b) => cmp(a.surface, b.surface) || cmp(a.locale, b.locale) || cmp(a.area, b.area));
}

export function formatStatus(rows: StatusRow[]): string {
	if (rows.length === 0) return "no translatable surface found";
	const head = ["surface", "locale", "area", "fresh", "outdated", "missing", "total"];
	const body = rows.map((r) => [r.surface, r.locale, r.area, `${r.fresh}`, `${r.outdated}`, `${r.missing}`, `${r.total}`]);
	const w = head.map((h, i) => Math.max(h.length, ...body.map((r) => r[i].length)));
	const line = (r: string[]) => r.map((c, i) => (i < 3 ? c.padEnd(w[i]) : c.padStart(w[i]))).join("  ");
	return [line(head), ...body.map(line)].join("\n");
}

async function main() {
	const args = process.argv.slice(2);
	const opt = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const root = opt("--root") ?? join(import.meta.dir, "..", "..");
	let ctx: Ctx;
	try {
		ctx = loadCtx(root, opt("--surface"));
	} catch (e) {
		console.error(`i18n config: ${(e as Error).message}`);
		process.exit(1);
	}
	if (args.includes("--status")) return console.log(args.includes("--json") ? JSON.stringify(await runStatus(ctx)) : formatStatus(await runStatus(ctx)));
	const findings = await runChecks(ctx);
	const mode = ctx.base
		? `ratchets compare with the merge-base ${ctx.base.mb.slice(0, 9)} (${ctx.base.mode}: ${ctx.base.ref}): ${ctx.base.changed.size} changed file(s) checked, the rest not looked at`
		: "no git base (not a checkout, no origin/main or main, or CI without one; `git fetch origin main` or set ATLAS_BASE): ratchets compare with the committed baseline";
	const count = (s: Finding["severity"]) => findings.filter((f) => f.severity === s).length;
	const [errors, warnings, info] = [count("error"), count("warn"), count("info")];
	if (args.includes("--json")) {
		console.log(JSON.stringify({ ok: errors === 0, errors, warnings, info, ratchets: ctx.base ? { mode: ctx.base.mode, base: ctx.base.ref, mergeBase: ctx.base.mb, changedFiles: ctx.base.changed.size } : "committed baseline", findings }, null, 2));
	} else {
		const show = args.includes("--info") ? findings : findings.filter((f) => f.severity !== "info");
		for (const f of show) console.log(`${f.severity === "error" ? "error" : f.severity === "warn" ? "warn " : "info "} ${formatFinding(f)}`);
		console.error(mode);
		const hidden = info > 0 && !args.includes("--info") ? `, ${info} info hidden (--info)` : "";
		console.log(errors || warnings ? `i18n: ${errors} error(s), ${warnings} warning(s)${hidden}` : `i18n: ok${hidden}`);
	}
	if (errors > 0) process.exit(1);
}

if (import.meta.main) await main();
