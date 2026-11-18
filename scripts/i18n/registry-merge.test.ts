// The two shared registries are conflict-free by design: an area names its surface in its own folder (area.json), and the
// ratchet baseline is one self-contained line per file that a branch only lowers (`merge=union`, the reader takes the
// smaller count of a path listed twice). Real temp git repositories, real `git merge`; the first case is the scenario
// that cost a manual resolution per batch PR: two batches add different areas to one surface and fix adjacent files.
import { afterAll, describe, expect, setDefaultTimeout, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { addMessage } from "./add.ts";
import { parseBaseline, readBaseline, renderBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { loadSurfacesConfig } from "./source.ts";
import { BASELINE, check, cleanup, errors, git, makeRepo, must, put, read } from "./testkit.ts";
import { loadCtx } from "./types.ts";

setDefaultTimeout(60_000);

// The resolver reads the environment: run with a clean one, whatever CI or a developer's shell exports.
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

const ATTRIBUTES = readFileSync(join(import.meta.dir, "..", "..", ".gitattributes"), "utf8");

const A = "apps/desktop/src/a.svelte";
const B = "apps/desktop/src/b.svelte";
const C = "apps/desktop/src/c.svelte";
const HARD = (n: number) => `${Array.from({ length: n }, (_, i) => `<p>Old text ${i}</p>`).join("\n")}\n`;

/** A valid repo on `main`: three adjacent files with offenders (3, 2, 4), and the repo's real .gitattributes. */
async function mainRepo() {
	const root = await makeRepo({ [A]: HARD(3), [B]: HARD(2), [C]: HARD(4), ".gitattributes": ATTRIBUTES });
	must(root, "init", "-q", "-b", "main");
	must(root, "add", "-A");
	must(root, "commit", "-q", "-m", "base");
	return root;
}

/** One batch: a new area on the desktop surface whose message is used by a new component, and `fixed` cleaned of offenders. */
function batch(root: string, name: string, area: string, fixed: string) {
	must(root, "checkout", "-q", "-b", name, "main");
	put(root, fixed, "<div></div>\n");
	addMessage(loadCtx(root), { key: `${area}_title`, english: `${area} title`, pt: `Título ${area}`, description: `Title of the ${area} screen`, newArea: true, surface: "desktop" });
	put(root, `apps/desktop/src/${area}.svelte`, `<script>\n  import * as m from "$lib/paraglide/messages";\n</script>\n<h1>{m.${area}_title()}</h1>\n`);
	fix(loadCtx(root));
	must(root, "add", "-A");
	must(root, "commit", "-q", "-m", name);
}

describe("two batches on one surface", () => {
	test("different areas and adjacent baseline lines merge without a conflict, and i18n:check passes with the right counts", async () => {
		const root = await mainRepo();
		expect(readBaseline(root).svelte).toEqual({ [A]: 3, [B]: 2, [C]: 4 });
		batch(root, "batch-a", "billing", A);
		batch(root, "batch-b", "payments", B);
		must(root, "checkout", "-q", "batch-a");
		const merged = git(root, "merge", "--no-edit", "batch-b");
		expect(merged.ok, `${merged.out}\n${merged.err}`).toBe(true);

		expect(loadSurfacesConfig(root).config.surfaces.desktop.areas).toEqual(["app", "billing", "payments"]);
		expect(read(root, "i18n/surfaces.json")).toBe(must(root, "show", "main:i18n/surfaces.json") + "\n");
		// a union merge may leave a duplicated line; the counts are what the reader makes of them
		expect(readBaseline(root).svelte).toEqual({ [A]: 0, [B]: 0, [C]: 4 });
		expect(errors(await check(root))).toEqual([]);

		fix(loadCtx(root));
		const text = read(root, BASELINE);
		expect(text).toBe(renderBaseline(parseBaseline(text)));
		expect(text.split("\n").filter((l) => l.includes(`"${A}"`))).toHaveLength(1);
		expect(errors(await check(root))).toEqual([]);
	});

	test("a fixed file keeps its line at 0 on its branch, so merging never brings a higher count back", async () => {
		const root = await mainRepo();
		batch(root, "batch-a", "billing", A);
		expect(git(root, "show", "batch-a:i18n/hardcoded-baseline.jsonl").out).toContain(`{"ratchet":"svelte","file":"${A}","count":0}`);
		batch(root, "batch-b", "payments", B);
		must(root, "checkout", "-q", "batch-b");
		expect(git(root, "merge", "--no-edit", "batch-a").ok).toBe(true);
		expect(readBaseline(root).svelte?.[A]).toBe(0);
		expect(readBaseline(root).svelte?.[B]).toBe(0);
	});

	test("the same line lowered on both sides keeps the smaller count", () => {
		const dir = mkdtempSync(join(tmpdir(), "i18n-union-"));
		const line = (file: string, n: number) => JSON.stringify({ ratchet: "svelte", file, count: n });
		const write = (...lines: string[]) => writeFileSync(join(dir, "i18n-baseline.jsonl"), `${lines.join("\n")}\n`);
		writeFileSync(join(dir, ".gitattributes"), "i18n-baseline.jsonl merge=union\n");
		must(dir, "init", "-q", "-b", "main");
		write(line("a", 5), line("b", 4), line("c", 3));
		must(dir, "add", "-A");
		must(dir, "commit", "-q", "-m", "base");
		must(dir, "checkout", "-q", "-b", "one");
		write(line("a", 3), line("b", 4), line("c", 3));
		must(dir, "commit", "-qam", "one");
		must(dir, "checkout", "-q", "-b", "two", "main");
		write(line("a", 4), line("b", 0), line("c", 3));
		must(dir, "commit", "-qam", "two");
		const merged = git(dir, "merge", "--no-edit", "one");
		expect(merged.ok, merged.err).toBe(true);
		expect(parseBaseline(readFileSync(join(dir, "i18n-baseline.jsonl"), "utf8")).svelte).toEqual({ a: 3, b: 0, c: 3 });
	});
});

describe("the baseline reader", () => {
	test("a path listed twice counts as the smaller number; blank lines are skipped; order does not matter", () => {
		const text = `${JSON.stringify({ ratchet: "rust", file: "z.rs", count: 2 })}\n\n${JSON.stringify({ ratchet: "rust", file: "a.rs", count: 7 })}\n${JSON.stringify({ ratchet: "rust", file: "z.rs", count: 1 })}\n`;
		expect(parseBaseline(text)).toEqual({ rust: { "a.rs": 7, "z.rs": 1 } });
	});

	test("a malformed line is an error naming the line, not a silently ignored entry", () => {
		expect(() => parseBaseline('{"ratchet":"rust","file":"a.rs","count":1}\n{ nope\n')).toThrow("hardcoded-baseline.jsonl:2");
		expect(() => parseBaseline('{"ratchet":"rust","file":"a.rs","count":-1}\n')).toThrow("expected");
	});

	test("the file is sorted, one line per path, and --prune-baseline is the only thing that deletes a line", async () => {
		const root = await makeRepo({ [A]: HARD(2) });
		fix(loadCtx(root), { baseline: true });
		put(root, A, "<div></div>\n");
		fix(loadCtx(root));
		expect(readBaseline(root).svelte).toEqual({ [A]: 0 });
		fix(loadCtx(root), { pruneBaseline: true });
		expect(readBaseline(root).svelte).toBeUndefined();
		expect(existsSync(join(root, BASELINE))).toBe(true);
	});
});
