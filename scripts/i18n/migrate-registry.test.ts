// `bun run i18n:migrate-registry`: a branch written for the old registry (areas listed in surfaces.json, the nested
// hardcoded-baseline.json) becomes a branch of the new one, also in the middle of a conflicted `git merge origin/main`.
import { afterAll, describe, expect, setDefaultTimeout, test } from "bun:test";
import { existsSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { addMessage } from "./add.ts";
import { applyBranchBaseline, migrateRegistry, readOldRegistry } from "./migrate-registry.ts";
import { type Baseline, readBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { loadSurfacesConfig } from "./source.ts";
import { areaFile, check, cleanup, errors, git, makeRepo, must, put, read } from "./testkit.ts";
import { loadCtx } from "./types.ts";

setDefaultTimeout(60_000);
const AMBIENT = ["ATLAS_BASE", "GITHUB_ACTIONS", "GITHUB_EVENT_NAME", "GITHUB_EVENT_PATH"];
const saved = Object.fromEntries(AMBIENT.map((k) => [k, process.env[k]]));
for (const k of AMBIENT) delete process.env[k];
afterAll(() => {
	for (const k of AMBIENT) {
		if (saved[k] === undefined) delete process.env[k];
		else process.env[k] = saved[k];
	}
	cleanup();
});

const A = "apps/desktop/src/a.svelte";
const B = "apps/desktop/src/b.svelte";
const HARD = (n: number) => `${Array.from({ length: n }, (_, i) => `<p>Old text ${i}</p>`).join("\n")}\n`;

/** Rewrite a repo of the new registry in the old one: area lists and generatedAreas in surfaces.json, the nested baseline. */
function toOldFormat(root: string) {
	const { config } = loadSurfacesConfig(root);
	const list = (xs: string[]) => `[${xs.map((x) => JSON.stringify(x)).join(", ")}]`;
	const generated = Object.entries(config.generatedAreas).map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(", ");
	const surfaces = Object.entries(config.surfaces).map(([name, d]) => {
		const out = (["paraglide", "arb", "rust"] as const).filter((k) => d[k]).map((k) => `"${k}": ${JSON.stringify(d[k])}`);
		return `\t\t${JSON.stringify(name)}: { "areas": ${list([...d.areas].sort())}, ${out.join(", ")} }`;
	});
	put(root, "i18n/surfaces.json", `{\n\t"budget": { "keys": 150, "bytes": 24576 },\n\t"generatedAreas": { ${generated} },\n\t"surfaces": {\n${surfaces.join(",\n")}\n\t}\n}\n`);
	for (const d of readdirSync(join(root, "i18n/messages"))) rmSync(join(root, "i18n/messages", d, "area.json"), { force: true });
	const b = readBaseline(root);
	put(root, "i18n/hardcoded-baseline.json", `${JSON.stringify(Object.fromEntries(Object.entries(b).map(([l, s]) => [l, Object.fromEntries(Object.entries(s ?? {}).filter(([, n]) => n > 0))])), null, "\t")}\n`);
	rmSync(join(root, "i18n/hardcoded-baseline.jsonl"), { force: true });
}

describe("migrate a tree in the old format", () => {
	test("writes every area.json (and the generated command), strips the lists from surfaces.json, converts the baseline; a second run does nothing", async () => {
		const root = await makeRepo({ [A]: HARD(2) });
		put(root, "i18n/messages/cli/area.json", '{\n\t"generated": "UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n",\n\t"surface": "rust"\n}\n');
		const want = readBaseline(root);
		toOldFormat(root);
		expect(loadSurfacesConfig(root).problems.length).toBeGreaterThan(0);

		const r = migrateRegistry(root);
		expect(r.areaFiles.sort()).toEqual(["i18n/messages/app/area.json", "i18n/messages/cli/area.json"]);
		expect(r).toMatchObject({ surfacesRewritten: true, baseline: "converted", unresolved: [] });
		expect(JSON.parse(read(root, "i18n/messages/cli/area.json"))).toEqual({ generated: "UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n", surface: "rust" });
		expect(existsSync(join(root, "i18n/hardcoded-baseline.json"))).toBe(false);
		expect(readBaseline(root)).toEqual(want);
		expect(loadSurfacesConfig(root).problems).toEqual([]);
		expect(errors(await check(root))).toEqual([]);

		const again = migrateRegistry(root);
		expect(again).toMatchObject({ areaFiles: [], surfacesRewritten: false, baseline: "none", unresolved: [] });
	});

	test("an area no list mentions is reported, not guessed", async () => {
		const root = await makeRepo();
		toOldFormat(root);
		put(root, "i18n/messages/stray/en.json", "{}\n");
		const r = migrateRegistry(root);
		expect(r.unresolved).toEqual(["stray"]);
		expect(existsSync(join(root, "i18n/messages/stray/area.json"))).toBe(false);
	});
});

describe("an unplaceable area", () => {
	test("exits with it listed; once it is placed, the rerun still derives the baseline (i18n:fix) although nothing else is left to migrate", async () => {
		const root = await makeRepo({ [A]: HARD(2) });
		toOldFormat(root);
		put(root, "i18n/hardcoded-baseline.json", `${JSON.stringify({ svelte: { [A]: 5 } })}\n`);
		put(root, "i18n/messages/stray/en.json", "{}\n");
		expect(migrateRegistry(root).unresolved).toEqual(["stray"]);
		expect(readBaseline(root).svelte?.[A]).toBe(5); // converted, not yet derived: fix did not run
		put(root, "i18n/messages/stray/area.json", areaFile("desktop"));
		const r = migrateRegistry(root);
		expect(r.unresolved).toEqual([]);
		expect(readBaseline(root).svelte?.[A]).toBe(2);
	});
});

describe("a branch of the old registry merging a main that migrated", () => {
	test("the conflicted merge is resolved by the command: the branch's area, its fixed file and main's own lines all survive", async () => {
		const root = await makeRepo({ [A]: HARD(3), [B]: HARD(2) });
		toOldFormat(root);
		must(root, "init", "-q", "-b", "main");
		must(root, "add", "-A");
		must(root, "commit", "-q", "-m", "old registry");

		// the open batch PR, written for the old registry: a new area listed in surfaces.json, a file fixed in the nested baseline
		must(root, "checkout", "-q", "-b", "batch");
		put(root, A, "<div></div>\n");
		put(root, "apps/desktop/src/billing.svelte", '<script>\n  import * as m from "$lib/paraglide/messages";\n</script>\n<h1>{m.billing_title()}</h1>\n');
		addMessage(loadCtx(root), { key: "billing_title", english: "Billing", pt: "Cobrança", description: "Title of the billing screen", newArea: true, surface: "desktop" });
		fix(loadCtx(root));
		toOldFormat(root);
		must(root, "add", "-A");
		must(root, "commit", "-q", "-m", "batch");

		// main moves to the new registry, and fixes the other file meanwhile
		must(root, "checkout", "-q", "main");
		put(root, B, "<div></div>\n");
		migrateRegistry(root);
		fix(loadCtx(root));
		must(root, "add", "-A");
		must(root, "commit", "-q", "-m", "new registry");

		must(root, "checkout", "-q", "batch");
		const merged = git(root, "merge", "--no-edit", "main");
		expect(merged.ok).toBe(false);
		expect(merged.out + merged.err).toContain("CONFLICT");

		const r = migrateRegistry(root);
		expect(r.unresolved).toEqual([]);
		expect(r.areaFiles).toEqual(["i18n/messages/billing/area.json"]);
		expect(read(root, "i18n/surfaces.json")).not.toContain("<<<<<<<");
		expect(loadSurfacesConfig(root).config.surfaces.desktop.areas).toEqual(["app", "billing"]);
		expect(readBaseline(root).svelte).toEqual({ [A]: 0, [B]: 0 });
		expect(existsSync(join(root, "i18n/hardcoded-baseline.json"))).toBe(false);

		must(root, "add", "-A");
		must(root, "commit", "-q", "-m", "merge main");
		expect(errors(await check(root))).toEqual([]);
		expect(migrateRegistry(root)).toMatchObject({ areaFiles: [], surfacesRewritten: false, baseline: "none" });
	});
});

describe("the pieces", () => {
	test("the old registry is read line by line, so both sides of a conflict are seen", () => {
		const text = [
			"{",
			'\t"budget": { "keys": 100, "bytes": 1000 },',
			'\t"generatedAreas": { "cli_x": "cmd" },',
			'\t"surfaces": {',
			"<<<<<<< HEAD",
			'\t\t"desktop": { "areas": ["a", "b"], "paraglide": "p" },',
			"=======",
			'\t\t"desktop": { "paraglide": "p" },',
			">>>>>>> main",
			'\t\t"rust": { "areas": ["c"], "rust": "crates" }',
			"\t}",
			"}",
		].join("\n");
		const reg = readOldRegistry(text);
		expect(reg.budget).toEqual({ keys: 100, bytes: 1000 });
		expect(reg.outputs).toEqual({ desktop: { paraglide: "p" }, rust: { rust: "crates" } });
		expect([...reg.areaSurface]).toEqual([["a", "desktop"], ["b", "desktop"], ["c", "rust"]]);
		expect([...reg.generated]).toEqual([["cli_x", "cmd"]]);
	});

	test("the branch's baseline lowers main's, a path it dropped goes to 0, a ratchet it has no section for is left alone", () => {
		const main: Baseline = { svelte: { a: 5, b: 4, c: 3 }, rust: { r: 2 } };
		const branch: Baseline = { svelte: { a: 2, c: 9 } };
		expect(applyBranchBaseline(main, branch)).toEqual({ svelte: { a: 2, b: 0, c: 3 }, rust: { r: 2 } });
		// a ratchet the other side introduced (rust-ui) is carried over whole
		expect(applyBranchBaseline(main, { svelte: { a: 5 }, "rust-ui": { u: 4 } })).toEqual({ svelte: { a: 5, b: 0, c: 0 }, rust: { r: 2 }, "rust-ui": { u: 4 } });
	});

	test("writeFileSync of the old file next to the new one is converted by min, never by raising", async () => {
		const root = await makeRepo({ [A]: HARD(3) });
		fix(loadCtx(root), { baseline: true });
		writeFileSync(join(root, "i18n/hardcoded-baseline.json"), `${JSON.stringify({ svelte: { [A]: 99 } })}\n`);
		const r = migrateRegistry(root);
		expect(r.baseline).toBe("applied");
		expect(readBaseline(root).svelte?.[A]).toBe(3);
	});
});
