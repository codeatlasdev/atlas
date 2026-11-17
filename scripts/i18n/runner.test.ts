// The runner: checker discovery by glob, the printed line, --json, --surface, --status, the exit code.
import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadCheckers, runChecks } from "./check.ts";
import { type Finding, formatFinding, loadCtx } from "./types.ts";
import { EN, PT, cleanup, editJson, makeRepo, put } from "./testkit.ts";

afterAll(cleanup);

const CHECK = join(import.meta.dir, "check.ts");
const cli = async (script: string, args: string[]) => {
	const p = Bun.spawn(["bun", script, ...args], { stdout: "pipe", stderr: "pipe", env: { ...process.env, ANTHROPIC_API_KEY: "", I18N_BASE_URL: "" } });
	const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
	return { out, err, code };
};

describe("checker discovery", () => {
	test("every file of checks/ is loaded; the shipped ones are docs-md, hardcoded, messages, platform-lint, supported-sync, writing", async () => {
		const { checkers, problems } = await loadCheckers();
		expect(problems).toEqual([]);
		expect(checkers.map((c) => c.name).sort()).toEqual(["docs-md", "hardcoded", "messages", "platform-lint", "supported-sync", "writing"]);
	});

	test("a new checker file in the directory runs with no edit to the runner", async () => {
		const dir = mkdtempSync(join(tmpdir(), "i18n-checks-"));
		const root = await makeRepo();
		try {
			const ctx = loadCtx(root);
			expect(await runChecks(ctx, dir)).toEqual([]);
			await Bun.write(join(dir, "arb.ts"), `export default { name: "arb", async run(ctx) { return [{ rule: "probe", severity: "error", file: "apps/mobile/lib/l10n/app_en.arb", line: 3, key: "k", message: "from " + ctx.root.length }]; } };\n`);
			const fs = await runChecks(ctx, dir);
			expect(fs).toHaveLength(1);
			expect(formatFinding(fs[0])).toBe(`apps/mobile/lib/l10n/app_en.arb:3 probe k from ${root.length}`);
			await Bun.write(join(dir, "zz.ts"), `export default { name: "zz", async run() { return [{ rule: "probe2", severity: "warn", file: "a", message: "m" }]; } };\n`);
			expect((await runChecks(ctx, dir)).map((f) => f.rule).sort()).toEqual(["probe", "probe2"]);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test("a checker that throws, and a file that is not a checker, are errors, not silent passes", async () => {
		const dir = mkdtempSync(join(tmpdir(), "i18n-checks-"));
		try {
			await Bun.write(join(dir, "boom.ts"), `export default { name: "boom", async run() { throw new Error("kaput"); } };\n`);
			await Bun.write(join(dir, "nothing.ts"), `export const x = 1;\n`);
			const fs = await runChecks(loadCtx(await makeRepo()), dir);
			expect(fs.map((f) => f.rule)).toEqual(["checker", "checker"]);
			expect(fs.map((f) => f.message).join("\n")).toContain("kaput");
			expect(fs.every((f) => f.severity === "error")).toBe(true);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});

describe("bun scripts/i18n/check.ts", () => {
	test("an empty tree exits 0", async () => {
		const root = mkdtempSync(join(tmpdir(), "i18n-empty-"));
		try {
			const r = await cli(CHECK, ["--root", root]);
			expect(r.code).toBe(0);
			expect(r.out.trim()).toBe("i18n: ok");
		} finally {
			rmSync(root, { recursive: true, force: true });
		}
	});

	test("a valid repo exits 0", async () => {
		const root = await makeRepo();
		const r = await cli(CHECK, ["--root", root]);
		expect(r.code).toBe(0);
		expect(r.out).toContain("i18n: ok");
	});

	test("a broken repo prints `file:line rule key message` and exits 1", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_save_button);
		const r = await cli(CHECK, ["--root", root]);
		expect(r.code).toBe(1);
		const line = r.out.split("\n").find((l) => l.includes(" missing "));
		expect(line).toBe(`error ${PT} missing app_save_button no pt-BR translation of "Save": run bun run i18n:translate --locale pt-BR --area app, or write it in ${PT} and run bun run i18n:fix`);
		expect(r.out).toContain("i18n: 2 error(s)");
	});

	test("--json is machine-readable with the same findings and the same exit code", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_save_button);
		const r = await cli(CHECK, ["--root", root, "--json"]);
		expect(r.code).toBe(1);
		const j = JSON.parse(r.out);
		expect(j.ok).toBe(false);
		expect(j.errors).toBe(2);
		expect(j.findings.find((f: Finding) => f.rule === "missing")).toMatchObject({ severity: "error", key: "app_save_button", file: PT });
	});

	test("--surface limits the run to one surface", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_save_button);
		expect((await cli(CHECK, ["--root", root, "--surface", "rust"])).code).toBe(0);
		expect((await cli(CHECK, ["--root", root, "--surface", "docs"])).code).toBe(0);
		expect((await cli(CHECK, ["--root", root, "--surface", "desktop"])).code).toBe(1);
	});

	test("info findings are hidden unless --info, and never fail the run", async () => {
		const root = await makeRepo();
		put(root, "docs/crew.md", "# Crew\n");
		const quiet = await cli(CHECK, ["--root", root]);
		expect(quiet.code).toBe(0);
		expect(quiet.out).toContain("1 info hidden");
		expect(quiet.out).not.toContain("docs-missing");
		expect((await cli(CHECK, ["--root", root, "--info"])).out).toContain("docs-missing");
	});

	test("--status prints a per-locale, per-area table", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_save_button);
		editJson(root, EN, (o) => void (o.app_welcome = "Welcome back, {name}"));
		const r = await cli(CHECK, ["--root", root, "--status"]);
		expect(r.code).toBe(0);
		const row = r.out.split("\n").find((l) => l.startsWith("desktop"));
		expect(row?.split(/\s+/)).toEqual(["desktop", "pt-BR", "app", "3", "1", "1", "5"]);
		expect(r.out).toContain("rust");
		expect(r.out).toContain("docs");
	});

	test("an invalid glossary or locales file is a clear config error with exit 1", async () => {
		const root = await makeRepo();
		put(root, "i18n/glossary.yaml", "- term: Atlas\n");
		const r = await cli(CHECK, ["--root", root]);
		expect(r.code).toBe(1);
		expect(r.err).toContain("needs `dnt: true`");
		put(root, "i18n/glossary.yaml", "- term: Atlas\n  dnt: true\n");
		put(root, "i18n/locales.json", '{ "default": "en", "locales": { "en": { "tier": "required" } } }');
		expect((await cli(CHECK, ["--root", root])).err).toContain('default locale must have tier "source"');
	});
});

describe("bun scripts/i18n/fix.ts", () => {
	test("--restamp of an unknown key fails and changes nothing", async () => {
		const root = await makeRepo();
		const r = await cli(join(import.meta.dir, "fix.ts"), ["--root", root, "--restamp", "app_nope"]);
		expect(r.code).toBe(1);
		expect(r.err).toContain("no such key");
	});
});
